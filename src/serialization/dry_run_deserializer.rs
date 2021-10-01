use std::any::Any;
use std::marker::PhantomData;
use std::ops::Range;

use anyhow::{bail, Result};

use crate::ext4::{BlockCount, BlockSize, Ext4Dentry, ExtentTree, InodeCount};
use crate::fat::{ClusterIdx, FatDentry};
use crate::serialization::{Deserializer, DeserializerInternals, DirectoryWriter, Reader};


pub type DryRunDeserializer<'a> = Deserializer<'a, DryRunDeserializerInternals<'a>>;

/// A mock version of `Ext4TreeDeserializer` which triggers all cases in which the actual deserializer would bail
/// mid-conversion, leaving the file system inconsistent. Running `DryRunDeserializer` does not mutate the partition.
/// The errors that will be caught are:
/// - File name too long
/// - Insufficient free space in file system
/// - Insufficient free inodes in the new ext4 file system
impl<'a> DryRunDeserializer<'a> {
    pub fn dry_run(
        reader: Reader<'a>,
        free_inodes: InodeCount,
        free_blocks: BlockCount,
        block_size: BlockSize,
    ) -> Result<()> {
        let mut instance = Self {
            internals: DryRunDeserializerInternals::new(reader, free_inodes, free_blocks, block_size),
            _lifetime: PhantomData,
        };
        instance.deserialize_directory_tree()?;
        instance.internals.result()
    }
}

pub struct DryRunDeserializerInternals<'a> {
    reader: Reader<'a>,
    free_inodes: InodeCount,
    free_blocks: BlockCount,
    used_inodes: InodeCount,
    used_blocks: BlockCount,
    block_size: BlockSize,
}

impl<'a> DryRunDeserializerInternals<'a> {
    // TODO pass fs to constructor instead?
    pub fn new(reader: Reader<'a>, free_inodes: InodeCount, free_blocks: BlockCount, block_size: BlockSize) -> Self {
        Self {
            reader,
            free_inodes,
            free_blocks,
            used_inodes: 1, // lost+found
            used_blocks: 1, // lost+found
            block_size,
        }
    }

    fn result(&self) -> Result<()> {
        let enough_inodes = self.used_inodes <= self.free_inodes;
        let enough_blocks = self.used_blocks <= self.free_blocks;
        match (enough_inodes, enough_blocks) {
            (true, true) => Ok(()),
            (true, false) => bail!(
                "{} free blocks required but only {} available",
                self.used_blocks,
                self.free_blocks
            ),
            (false, true) => bail!(
                "{} free inodes required but only {} available",
                self.used_inodes,
                self.free_inodes
            ),
            (false, false) => bail!(
                "{} free blocks required but only {} available; {} inodes required but only {} available",
                self.used_blocks,
                self.free_blocks,
                self.used_inodes,
                self.free_inodes
            ),
        }
    }
}

impl<'a> DeserializerInternals<'a> for DryRunDeserializerInternals<'a> {
    type D = DryRunDirectoryWriter;

    fn read_next<T: Any>(&mut self) -> Vec<T> {
        self.reader.next::<T>()
    }

    fn build_root(&mut self) -> Result<DryRunDirectoryWriter> {
        Ok(DryRunDirectoryWriter::new(self.block_size))
    }

    fn deserialize_directory(
        &mut self,
        _dentry: FatDentry,
        name: String,
        parent_directory_writer: &mut DryRunDirectoryWriter,
    ) -> Result<DryRunDirectoryWriter> {
        self.build_file(name, parent_directory_writer)?;
        Ok(DryRunDirectoryWriter::new(self.block_size))
    }

    fn deserialize_regular_file(
        &mut self,
        _dentry: FatDentry,
        name: String,
        extents: Vec<Range<ClusterIdx>>,
        parent_directory_writer: &mut DryRunDirectoryWriter,
    ) -> Result<()> {
        self.build_file(name, parent_directory_writer)?;
        self.used_blocks += ExtentTree::required_block_count(extents.len(), self.block_size);
        Ok(())
    }
}

// TODO test `required_block_count`
impl<'a> DryRunDeserializerInternals<'a> {
    fn build_file(&mut self, name: String, parent_directory_writer: &mut DryRunDirectoryWriter) -> Result<()> {
        self.used_inodes += 1;
        self.used_blocks += parent_directory_writer.add_dentry(&Ext4Dentry::new(0, name)?);
        Ok(())
    }
}

pub struct DryRunDirectoryWriter {
    used_dentry_blocks: BlockCount,
    used_extent_blocks: BlockCount,
    block_size: BlockSize,
    position_in_block: u32,
}

impl DirectoryWriter for DryRunDirectoryWriter {}

impl DryRunDirectoryWriter {
    fn new(block_size: BlockSize) -> Self {
        Self {
            used_dentry_blocks: 0,
            used_extent_blocks: 0,
            block_size,
            position_in_block: block_size, // to model the first block being allocated immediately
        }
    }

    fn add_dentry(&mut self, dentry: &Ext4Dentry) -> usize {
        let old_used_blocks = self.used_blocks();
        if u32::from(dentry.dentry_len()) > self.remaining_space() {
            self.used_dentry_blocks += 1;
            self.position_in_block = 0;
            self.used_extent_blocks = ExtentTree::required_block_count(self.used_dentry_blocks, self.block_size);
        }
        self.position_in_block += u32::from(dentry.dentry_len());

        self.used_blocks() - old_used_blocks
    }

    fn used_blocks(&self) -> usize {
        self.used_dentry_blocks + self.used_extent_blocks
    }

    fn remaining_space(&self) -> u32 {
        self.block_size - self.position_in_block
    }
}
