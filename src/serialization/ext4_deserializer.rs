use std::any::Any;
use std::marker::PhantomData;
use std::ops::Range;
use std::rc::Rc;

use anyhow::Result;

use crate::allocator::{AllocatedClusterIdx, Allocator};
use crate::ext4::{Ext4Dentry, Ext4DentrySized, Ext4Fs, Extent, Inode, SuperBlock};
use crate::fat::{ClusterIdx, FatDentry, FatFs};
use crate::serialization::{Deserializer, DeserializerInternals, DirectoryWriter, DryRunDeserializer, Reader};


pub type Ext4TreeDeserializer<'a> = Deserializer<'a, Ext4TreeDeserializerInternals<'a>>;

impl<'a> Ext4TreeDeserializer<'a> {
    pub fn new(reader: Reader<'a>, allocator: Allocator<'a>, fat_fs: Ext4Fs<'a>) -> Self {
        Self {
            internals: Ext4TreeDeserializerInternals::new(reader, allocator, fat_fs),
            _lifetime: PhantomData,
        }
    }

    pub fn new_with_dry_run(reader: Reader<'a>, allocator: Allocator<'a>, fat_fs: FatFs<'a>) -> Result<Self> {
        let free_inodes = SuperBlock::from(fat_fs.boot_sector())?.free_inode_count();
        let free_blocks = allocator.free_block_count();
        DryRunDeserializer::dry_run(reader.clone(), free_inodes, free_blocks, fat_fs.cluster_size())?;
        Ok(Self::new(reader, allocator, fat_fs.into_ext4()))
    }
}

pub struct Ext4TreeDeserializerInternals<'a> {
    allocator: Rc<Allocator<'a>>,
    reader: Reader<'a>,
    ext_fs: Ext4Fs<'a>,
}

impl<'a> DeserializerInternals<'a> for Ext4TreeDeserializerInternals<'a> {
    type D = DentryWriter<'a>;

    fn build_root(&mut self) -> DentryWriter<'a> {
        let root_inode = unsafe { self.ext_fs.build_root_inode() };
        let mut dentry_writer = DentryWriter::new(root_inode, Rc::clone(&self.allocator), &mut self.ext_fs);
        self.build_root_dot_dirs(&mut dentry_writer);
        self.build_lost_found(&mut dentry_writer);
        dentry_writer
    }

    fn deserialize_directory(
        &mut self,
        dentry: FatDentry,
        name: String,
        parent_directory_writer: &mut DentryWriter<'a>,
    ) -> DentryWriter<'a> {
        let inode = self.build_file(dentry, name, parent_directory_writer);
        let mut dentry_writer = DentryWriter::new(inode, Rc::clone(&self.allocator), &mut self.ext_fs);
        self.build_dot_dirs(&mut parent_directory_writer.inode, &mut dentry_writer);
        dentry_writer
    }

    fn deserialize_regular_file(
        &mut self,
        dentry: FatDentry,
        name: String,
        extents: Vec<Range<ClusterIdx>>,
        parent_directory_writer: &mut DentryWriter,
    ) {
        let mut inode = self.build_file(dentry, name, parent_directory_writer);
        self.ext_fs.set_extents(&mut inode, extents, &self.allocator);
        inode.set_size(dentry.file_size as u64);
    }

    fn read_next<T: Any>(&mut self) -> Vec<T> {
        self.reader.next::<T>()
    }
}

impl<'a> Ext4TreeDeserializerInternals<'a> {
    pub fn new(reader: Reader<'a>, allocator: Allocator<'a>, ext_fs: Ext4Fs<'a>) -> Self {
        Self { reader, allocator: Rc::new(allocator), ext_fs }
    }

    fn build_file(&mut self, dentry: FatDentry, name: String, parent_dentry_writer: &mut DentryWriter) -> Inode<'a> {
        let mut inode = self.ext_fs.allocate_inode(dentry.is_dir());
        inode.init_from_dentry(dentry);
        parent_dentry_writer.add_dentry(Ext4Dentry::new(inode.inode_no, name), &mut self.ext_fs);
        inode
    }

    fn build_lost_found(&mut self, root_dentry_writer: &mut DentryWriter) {
        let inode = self.ext_fs.build_lost_found_inode();
        let dentry = Ext4Dentry::new(inode.inode_no, "lost+found".to_string());

        root_dentry_writer.add_dentry(dentry, &mut self.ext_fs);
        let mut dentry_writer = DentryWriter::new(inode, Rc::clone(&self.allocator), &mut self.ext_fs);
        self.build_dot_dirs(&mut root_dentry_writer.inode, &mut dentry_writer);
    }

    fn build_dot_dirs(&mut self, parent_inode: &mut Inode, dentry_writer: &mut DentryWriter) {
        let dot_dentry = Ext4Dentry::new(dentry_writer.inode.inode_no, ".".to_string());
        dentry_writer.add_dentry(dot_dentry, &mut self.ext_fs);
        dentry_writer.inode.increment_link_count();

        let dot_dot_dentry = Ext4Dentry::new(parent_inode.inode_no, "..".to_string());
        dentry_writer.add_dentry(dot_dot_dentry, &mut self.ext_fs);
        parent_inode.increment_link_count();
    }

    // same as `build_dot_dirs` except `parent_inode` would alias `dentry_writer.inode`
    fn build_root_dot_dirs(&mut self, dentry_writer: &mut DentryWriter) {
        let dot_dentry = Ext4Dentry::new(dentry_writer.inode.inode_no, ".".to_string());
        dentry_writer.add_dentry(dot_dentry, &mut self.ext_fs);
        dentry_writer.inode.increment_link_count();

        let dot_dot_dentry = Ext4Dentry::new(dentry_writer.inode.inode_no, "..".to_string());
        dentry_writer.add_dentry(dot_dot_dentry, &mut self.ext_fs);
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
    pub fn new(mut inode: Inode<'a>, allocator: Rc<Allocator<'a>>, ext_fs: &mut Ext4Fs) -> Self {
        let block = allocator.allocate_one();
        let extent = Extent::new(block.as_cluster_idx()..block.as_cluster_idx() + 1, 0);
        ext_fs.register_extent(&mut inode, extent, &allocator);
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

    fn add_dentry(&mut self, dentry: Ext4Dentry, ext_fs: &mut Ext4Fs) {
        if dentry.dentry_len() as usize > self.remaining_space() {
            self.allocate_block(ext_fs);
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

    fn allocate_block(&mut self, ext_fs: &mut Ext4Fs) {
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
        ext_fs.register_extent(&mut self.inode, extent, &self.allocator);
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
