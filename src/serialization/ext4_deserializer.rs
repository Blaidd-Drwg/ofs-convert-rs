use std::any::Any;
use std::ops::Range;
use std::rc::Rc;

use crate::allocator::{AllocatedClusterIdx, Allocator};
use crate::ext4::{Ext4Dentry, Ext4DentrySized, Ext4Partition, Extent, Inode};
use crate::fat::{ClusterIdx, FatDentry};
use crate::serialization::{FileType, Reader};


pub trait DirectoryWriter {}


pub trait AbstractDeserializer<'a, D: DirectoryWriter> {
    fn build_root(&self, partition: &mut Ext4Partition<'a>) -> D;

    fn deserialize_directory(
        &mut self,
        dentry: FatDentry,
        name: String,
        parent_directory_writer: &mut D,
        partition: &mut Ext4Partition<'a>,
    ) -> D;

    fn deserialize_regular_file(
        &mut self,
        dentry: FatDentry,
        name: String,
        extents: Vec<Range<ClusterIdx>>,
        parent_directory_writer: &mut D,
        partition: &mut Ext4Partition<'a>,
    );

    fn read_next<T: Any>(&mut self) -> Vec<T>;

    fn deserialize_directory_tree(&mut self, partition: &mut Ext4Partition<'a>) {
        let mut root_directory_writer = self.build_root(partition);

        for _ in 0..self.read_root_child_count() {
            self.deserialize_file(&mut root_directory_writer, partition);
        }
    }

    fn deserialize_file(&mut self, parent_directory_writer: &mut D, partition: &mut Ext4Partition<'a>) {
        let file_type = self.read_next::<FileType>()[0];
        let dentry = self.read_next::<FatDentry>()[0];
        let name = String::from_utf8(self.read_next::<u8>()).unwrap();

        match file_type {
            FileType::Directory(child_count) => {
                let mut directory_writer = self.deserialize_directory(dentry, name, parent_directory_writer, partition);
                for _ in 0..child_count {
                    self.deserialize_file(&mut directory_writer, partition);
                }
            }
            FileType::RegularFile => {
                let extents = self.read_next::<Range<ClusterIdx>>();
                self.deserialize_regular_file(dentry, name, extents, parent_directory_writer, partition);
            }
        }
    }

    fn read_root_child_count(&mut self) -> u32 {
        if let FileType::Directory(child_count) = self.read_next::<FileType>()[0] {
            child_count
        } else {
            panic!("First StreamArchiver entry is not root directory child count");
        }
    }
}


pub struct Ext4TreeDeserializer<'a> {
    allocator: Rc<Allocator<'a>>,
    reader: Reader<'a>,
}

impl<'a> AbstractDeserializer<'a, DentryWriter<'a>> for Ext4TreeDeserializer<'a> {
    fn read_next<T: Any>(&mut self) -> Vec<T> {
        self.reader.next::<T>()
    }

    fn deserialize_directory(
        &mut self,
        dentry: FatDentry,
        name: String,
        parent_directory_writer: &mut DentryWriter<'a>,
        partition: &mut Ext4Partition<'a>,
    ) -> DentryWriter<'a> {
        let inode = self.build_file(dentry, name, parent_directory_writer, partition);
        let mut dentry_writer = DentryWriter::new(inode, Rc::clone(&self.allocator), partition);
        Self::build_dot_dirs(&mut parent_directory_writer.inode, &mut dentry_writer, partition);
        dentry_writer
    }

    fn deserialize_regular_file(
        &mut self,
        dentry: FatDentry,
        name: String,
        extents: Vec<Range<ClusterIdx>>,
        parent_directory_writer: &mut DentryWriter,
        partition: &mut Ext4Partition<'a>,
    ) {
        let mut inode = self.build_file(dentry, name, parent_directory_writer, partition);
        partition.set_extents(&mut inode, extents, &self.allocator);
        inode.set_size(dentry.file_size as u64);
    }

    fn build_root(&self, partition: &mut Ext4Partition<'a>) -> DentryWriter<'a> {
        let root_inode = unsafe { partition.build_root_inode() };
        let mut dentry_writer = DentryWriter::new(root_inode, Rc::clone(&self.allocator), partition);
        Self::build_root_dot_dirs(&mut dentry_writer, partition);
        self.build_lost_found(&mut dentry_writer, partition);
        dentry_writer
    }
}

impl<'a> Ext4TreeDeserializer<'a> {
    pub fn new(reader: Reader<'a>, allocator: Allocator<'a>) -> Self {
        Self { reader, allocator: Rc::new(allocator) }
    }

    fn build_file(
        &mut self,
        dentry: FatDentry,
        name: String,
        parent_dentry_writer: &mut DentryWriter,
        partition: &mut Ext4Partition<'a>,
    ) -> Inode<'a> {
        let mut inode = partition.allocate_inode(dentry.is_dir());
        inode.init_from_dentry(dentry);
        parent_dentry_writer.add_dentry(Ext4Dentry::new(inode.inode_no, name), partition);
        inode
    }

    fn build_lost_found(&self, root_dentry_writer: &mut DentryWriter, partition: &mut Ext4Partition) {
        let inode = partition.build_lost_found_inode();
        let dentry = Ext4Dentry::new(inode.inode_no, "lost+found".to_string());

        root_dentry_writer.add_dentry(dentry, partition);
        let mut dentry_writer = DentryWriter::new(inode, Rc::clone(&self.allocator), partition);
        Self::build_dot_dirs(&mut root_dentry_writer.inode, &mut dentry_writer, partition);
    }

    fn build_dot_dirs(parent_inode: &mut Inode, dentry_writer: &mut DentryWriter, partition: &mut Ext4Partition) {
        let dot_dentry = Ext4Dentry::new(dentry_writer.inode.inode_no, ".".to_string());
        dentry_writer.add_dentry(dot_dentry, partition);
        dentry_writer.inode.increment_link_count();

        let dot_dot_dentry = Ext4Dentry::new(parent_inode.inode_no, "..".to_string());
        dentry_writer.add_dentry(dot_dot_dentry, partition);
        parent_inode.increment_link_count();
    }

    // same as `build_dot_dirs` except `parent_inode` would alias `dentry_writer.inode`
    fn build_root_dot_dirs(dentry_writer: &mut DentryWriter, partition: &mut Ext4Partition) {
        let dot_dentry = Ext4Dentry::new(dentry_writer.inode.inode_no, ".".to_string());
        dentry_writer.add_dentry(dot_dentry, partition);
        dentry_writer.inode.increment_link_count();

        let dot_dot_dentry = Ext4Dentry::new(dentry_writer.inode.inode_no, "..".to_string());
        dentry_writer.add_dentry(dot_dot_dentry, partition);
        dentry_writer.inode.increment_link_count();
    }
}

pub struct DentryWriter<'a> {
    inode: Inode<'a>,
    block_size: usize,
    position_in_block: usize,
    allocator: Rc<Allocator<'a>>,
    block: AllocatedClusterIdx,
    previous_dentry: Option<&'a mut Ext4DentrySized>,
    block_count: usize,
}

impl<'a> DentryWriter<'a> {
    pub fn new(mut inode: Inode<'a>, allocator: Rc<Allocator<'a>>, partition: &mut Ext4Partition) -> Self {
        let block = allocator.allocate_one();
        let extent = Extent::new(block.as_cluster_idx()..block.as_cluster_idx() + 1, 0);
        partition.register_extent(&mut inode, extent, &allocator);
        inode.increment_size(allocator.block_size() as u64);

        Self {
            inode,
            block_size: allocator.block_size(),
            /// Invariant: `position_in_block <= block_size`
            position_in_block: 0,
            allocator,
            block,
            previous_dentry: None,
            block_count: 1,
        }
    }

    fn add_dentry(&mut self, dentry: Ext4Dentry, partition: &mut Ext4Partition) {
        if dentry.dentry_len() as usize > self.remaining_space() {
            self.allocate_block(partition);
        }

        let name = dentry.serialize_name();
        let block = self.allocator.cluster_mut(&mut self.block);
        // SAFETY: Safe because by the invariant on `position_in_block` this still points inside the block.
        let dentry_ptr = unsafe { block.as_mut_ptr().add(self.position_in_block) as *mut Ext4DentrySized };
        // SAFETY: Safe because we made sure that the remaining space is sufficient for the entire dentry. Further,
        // `block` is 4-aligned and `dentry.dentry_len` is always a multiple of 4, so `dentry_ptr` is 4-aligned.
        unsafe {
            dentry_ptr.write(dentry.inner);
            let name_ptr = dentry_ptr.add(1) as *mut u8;
            name_ptr.copy_from_nonoverlapping(name.as_ptr(), name.len());
        }

        self.position_in_block += dentry.dentry_len() as usize;
        // SAFETY: It's the pointer we just wrote to, so it's valid, aligned and initialized.
        self.previous_dentry = unsafe { Some(&mut *dentry_ptr) };
    }

    fn remaining_space(&self) -> usize {
        self.block_size - self.position_in_block
    }

    fn allocate_block(&mut self, partition: &mut Ext4Partition) {
        let remaining_space = self.remaining_space();
        if let Some(previous_dentry) = self.previous_dentry.as_mut() {
            previous_dentry.increment_dentry_len(remaining_space as u16);
        }

        self.block = self.allocator.allocate_one();

        self.position_in_block = 0;
        self.block_count += 1;
        self.previous_dentry = None;

        let extent = Extent::new(
            self.block.as_cluster_idx()..self.block.as_cluster_idx() + 1,
            self.block_count as u32 - 1,
        );
        partition.register_extent(&mut self.inode, extent, &self.allocator);
        self.inode.increment_size(self.block_size as u64);
    }
}

impl DirectoryWriter for DentryWriter<'_> {}

impl Drop for DentryWriter<'_> {
    fn drop(&mut self) {
        let remaining_space = self.remaining_space();
        if let Some(previous_dentry) = self.previous_dentry.as_mut() {
            previous_dentry.increment_dentry_len(remaining_space as u16);
        }
    }
}
