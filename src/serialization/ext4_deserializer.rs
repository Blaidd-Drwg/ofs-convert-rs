use std::ops::Range;
use std::rc::Rc;

use crate::allocator::{AllocatedClusterIdx, Allocator};
use crate::c_wrapper::{c_add_extent, c_build_inode, c_build_lost_found_inode, c_build_root_inode, c_get_inode};
use crate::ext4::{Ext4Dentry, Ext4DentrySized};
use crate::fat::{ClusterIdx, FatDentry};
use crate::serialization::{FileType, Reader};

pub struct ExtTreeDeserializer<'a> {
    reader: Reader<'a>,
    allocator: Rc<Allocator<'a>>,
}

impl<'a> ExtTreeDeserializer<'a> {
    pub fn new(reader: Reader<'a>, allocator: Allocator<'a>) -> Self {
        Self { reader, allocator: Rc::new(allocator) }
    }

    pub fn deserialize_directory_tree(&mut self) {
        let mut root_dentry_writer = self.build_root();

        for _ in 0..self.read_root_child_count() {
            self.deserialize_file(&mut root_dentry_writer);
        }
    }

    fn deserialize_file(&mut self, parent_dentry_writer: &mut DentryWriter) {
        let file_type = self.reader.next::<FileType>()[0];
        let dentry = self.reader.next::<FatDentry>()[0];
        let name = String::from_utf8(self.reader.next::<u8>()).unwrap();

        let inode_no = c_build_inode(&dentry);
        parent_dentry_writer.add_dentry(Ext4Dentry::new(inode_no, name));

        match file_type {
            FileType::Directory(child_count) => self.deserialize_directory(inode_no, parent_dentry_writer, child_count),
            FileType::RegularFile => self.deserialize_regular_file(inode_no, dentry.file_size as u64),
        }
    }

    fn deserialize_directory(&mut self, inode_no: u32, parent_dentry_writer: &mut DentryWriter, child_count: u32) {
        let mut dentry_writer = DentryWriter::new(inode_no, Rc::clone(&self.allocator));
        self.build_dot_dirs(parent_dentry_writer.inode_no, &mut dentry_writer);

        for _ in 0..child_count {
            self.deserialize_file(&mut dentry_writer);
        }
    }

    fn deserialize_regular_file(&mut self, inode_no: u32, size: u64) {
        let extents = self.reader.next::<Range<ClusterIdx>>();
        Self::add_extents(inode_no, extents);
        c_get_inode(inode_no).set_size(size);
    }

    fn build_root(&self) -> DentryWriter<'a> {
        let root_inode_no = c_build_root_inode();
        let mut dentry_writer = DentryWriter::new(root_inode_no, Rc::clone(&self.allocator));
        self.build_dot_dirs(root_inode_no, &mut dentry_writer);
        self.build_lost_found(&mut dentry_writer);
        dentry_writer
    }

    fn build_lost_found(&self, root_dentry_writer: &mut DentryWriter) {
        let inode_no = c_build_lost_found_inode();
        let dentry = Ext4Dentry::new(inode_no, "lost+found".to_string());

        root_dentry_writer.add_dentry(dentry);
        let mut dentry_writer = DentryWriter::new(inode_no, Rc::clone(&self.allocator));
        self.build_dot_dirs(root_dentry_writer.inode_no, &mut dentry_writer);
    }

    fn build_dot_dirs(&self, parent_inode_no: u32, dentry_writer: &mut DentryWriter) {
        let dot_dentry = Ext4Dentry::new(dentry_writer.inode_no, ".".to_string());
        dentry_writer.add_dentry(dot_dentry);
        c_get_inode(dentry_writer.inode_no).increment_link_count();

        let dot_dot_dentry = Ext4Dentry::new(parent_inode_no, "..".to_string());
        dentry_writer.add_dentry(dot_dot_dentry);
        c_get_inode(parent_inode_no).increment_link_count();
    }

    fn read_root_child_count(&mut self) -> u32 {
        if let FileType::Directory(child_count) = self.reader.next::<FileType>()[0] {
            child_count
        } else {
            panic!("First StreamArchiver entry is not root directory child count");
        }
    }

    fn add_extents(inode_no: u32, extents: Vec<Range<ClusterIdx>>) {
        let mut logical_block = 0;
        for extent in extents {
            c_add_extent(inode_no, extent.start, logical_block, extent.len() as u16);
            logical_block += extent.len() as u32;
        }
    }
}

struct DentryWriter<'a> {
    inode_no: u32,
    block_size: usize,
    position_in_block: usize,
    allocator: Rc<Allocator<'a>>,
    block: AllocatedClusterIdx,
    previous_dentry: Option<&'a mut Ext4DentrySized>,
    block_count: usize,
}

impl<'a> DentryWriter<'a> {
    pub fn new(inode_no: u32, allocator: Rc<Allocator<'a>>) -> Self {
        let block = allocator.allocate_one();
        c_add_extent(inode_no, block.as_cluster_idx(), 0, 1);
        c_get_inode(inode_no).increment_size(allocator.block_size() as u64);

        Self {
            inode_no,
            block_size: allocator.block_size(),
            position_in_block: 0,
            allocator,
            block,
            previous_dentry: None,
            block_count: 1,
        }
    }

    fn add_dentry(&mut self, dentry: Ext4Dentry) {
        if dentry.dentry_len() as usize > self.remaining_space() {
            self.allocate_block();
        }

        let name = dentry.serialize_name();
        let block = self.allocator.cluster_mut(&mut self.block);
        let dentry_ptr = unsafe { block.as_mut_ptr().add(self.position_in_block) as *mut Ext4DentrySized };
        unsafe {
            dentry_ptr.write_unaligned(dentry.inner);
            let name_ptr = dentry_ptr.add(1) as *mut u8;
            name_ptr.copy_from_nonoverlapping(name.as_ptr(), name.len());
        }

        self.position_in_block += dentry.dentry_len() as usize;
        self.previous_dentry = unsafe { Some(&mut *dentry_ptr) };
    }

    fn remaining_space(&self) -> usize {
        self.block_size - self.position_in_block
    }

    fn allocate_block(&mut self) {
        let remaining_space = self.remaining_space();
        if let Some(previous_dentry) = self.previous_dentry.as_mut() {
            previous_dentry.dentry_len += remaining_space as u16;
        }

        self.block = self.allocator.allocate_one();

        self.position_in_block = 0;
        self.block_count += 1;
        self.previous_dentry = None;

        c_add_extent(self.inode_no, self.block.as_cluster_idx(), self.block_count as u32 - 1, 1);
        c_get_inode(self.inode_no).increment_size(self.block_size as u64);
    }
}

impl Drop for DentryWriter<'_> {
    fn drop(&mut self) {
        let remaining_space = self.remaining_space();
        if let Some(previous_dentry) = self.previous_dentry.as_mut() {
            previous_dentry.dentry_len += remaining_space as u16;
        }
    }
}
