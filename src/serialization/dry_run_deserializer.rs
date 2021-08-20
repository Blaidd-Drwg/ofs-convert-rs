use std::any::Any;
use std::ops::Range;

use crate::fat::{ClusterIdx, FatDentry};
use crate::serialization::{DeserializerInternals, DirectoryWriter, Reader};


pub struct DryRunDeserializerInternals<'a> {
    reader: Reader<'a>,
    free_inodes: usize,
    free_blocks: usize,
    used_inodes: usize,
    used_blocks: usize,
}

impl<'a> DryRunDeserializerInternals<'a> {
    // pub fn dry_run(reader: Reader<'a>, free_inodes: usize, free_blocks: usize) -> Result<(), ()> {
    // let mut instance = Self::new(free_inodes, free_blocks, reader);
    // instance.deserialize_directory_tree();
    // instance.result()
    // }

    // TODO pass partition to constructor instead?
    pub fn new(free_inodes: usize, free_blocks: usize, reader: Reader<'a>) -> Self {
        Self {
            reader,
            free_inodes,
            free_blocks,
            used_inodes: 0,
            used_blocks: 0,
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
        dentry: FatDentry,
        name: String,
        parent_directory_writer: &mut DryRunDirectoryWriter,
    ) -> DryRunDirectoryWriter {
        unimplemented!()
    }

    fn deserialize_regular_file(
        &mut self,
        dentry: FatDentry,
        name: String,
        extents: Vec<Range<ClusterIdx>>,
        parent_directory_writer: &mut DryRunDirectoryWriter,
    ) {
    }
}

pub struct DryRunDirectoryWriter;
impl DirectoryWriter for DryRunDirectoryWriter {}
