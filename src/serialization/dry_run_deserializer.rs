use std::any::Any;
use std::ops::Range;

use crate::ext4::{Ext4Dentry, ExtentTree};
use crate::fat::{ClusterIdx, FatDentry};
use crate::serialization::{Deserializer, DeserializerInternals, DirectoryWriter, Reader};


pub type DryRunDeserializer<'a> = Deserializer<'a, DryRunDeserializerInternals<'a>>;

impl<'a> DryRunDeserializer<'a> {
    pub fn dry_run() -> Result<(), ()> {
        Ok(())
    }
}

pub struct DryRunDeserializerInternals<'a> {
    reader: Reader<'a>,
    free_inodes: usize,
    free_blocks: usize,
    used_inodes: usize,
    used_blocks: usize,
    block_size: usize,
}

impl<'a> DryRunDeserializerInternals<'a> {
    // TODO pass partition to constructor instead?
    pub fn new(free_inodes: usize, free_blocks: usize, block_size: usize, reader: Reader<'a>) -> Self {
        Self {
            reader,
            free_inodes,
            free_blocks,
            used_inodes: 0,
            used_blocks: 0,
            block_size,
        }
    }

    fn result(&self) -> Result<(), ()> {
        if self.used_inodes <= self.free_inodes && self.used_blocks <= self.free_blocks {
            Ok(())
        } else {
            Err(())
        }
    }
}

impl<'a> DeserializerInternals<'a> for DryRunDeserializerInternals<'a> {
    type D = DryRunDirectoryWriter;

    fn read_next<T: Any>(&mut self) -> Vec<T> {
        self.reader.next::<T>()
    }

    fn build_root(&mut self) -> DryRunDirectoryWriter {
        unimplemented!()
    }

    fn deserialize_directory(
        &mut self,
        _dentry: FatDentry,
        name: String,
        parent_directory_writer: &mut DryRunDirectoryWriter,
    ) -> DryRunDirectoryWriter {
        self.build_file(name, parent_directory_writer);
        // new writer
        unimplemented!()
    }

    fn deserialize_regular_file(
        &mut self,
        _dentry: FatDentry,
        name: String,
        extents: Vec<Range<ClusterIdx>>,
        parent_directory_writer: &mut DryRunDirectoryWriter,
    ) {
        self.build_file(name, parent_directory_writer);
        self.used_blocks += ExtentTree::required_block_count(extents.len(), self.block_size);
    }
}

// TODO test `required_block_count`
impl<'a> DryRunDeserializerInternals<'a> {
    fn build_file(&mut self, name: String, parent_directory_writer: &mut DryRunDirectoryWriter) {
        self.used_inodes += 1;
        self.used_blocks += parent_directory_writer.add_dentry(&Ext4Dentry::new(0, name));
    }
}

pub struct DryRunDirectoryWriter {
    used_dentry_blocks: usize,
    used_extent_blocks: usize,
    block_size: usize,
    position_in_block: usize,
}

impl DirectoryWriter for DryRunDirectoryWriter {}

impl DryRunDirectoryWriter {
    fn new(block_size: usize) -> Self {
        Self {
            used_dentry_blocks: 0,
            used_extent_blocks: 0,
            block_size,
            position_in_block: 0,
        }
    }

    fn add_dentry(&mut self, dentry: &Ext4Dentry) -> usize {
        let old_used_blocks = self.used_blocks();
        if dentry.dentry_len() as usize > self.remaining_space() {
            self.used_dentry_blocks += 1;
            self.position_in_block = 0;
            self.used_extent_blocks = ExtentTree::required_block_count(self.used_dentry_blocks, self.block_size);
        }
        self.position_in_block += dentry.dentry_len() as usize;

        self.used_blocks() - old_used_blocks
    }

    fn used_blocks(&self) -> usize {
        self.used_dentry_blocks + self.used_extent_blocks
    }

    fn remaining_space(&self) -> usize {
        self.block_size - self.position_in_block
    }
}
