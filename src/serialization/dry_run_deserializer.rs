use std::any::Any;
use std::marker::PhantomData;
use std::ops::Range;

use anyhow::{bail, Context, Result};

use crate::ext4::{BlockCount, BlockSize, Ext4Dentry, Extent, ExtentTree, InodeCount};
use crate::fat::ClusterIdx;
use crate::serialization::{DentryRepresentation, Deserializer, DeserializerInternals, DirectoryWriter, Reader};
use crate::util::FromU32;
use crate::BlockIdx;


pub type DryRunDeserializer<'a> = Deserializer<'a, DryRunDeserializerInternals<'a>>;

/// A mock version of `Ext4TreeDeserializer` which triggers all cases in which the actual deserializer would bail
/// mid-conversion, leaving the file system inconsistent. Running `DryRunDeserializer` does not mutate the partition.
/// The errors that will be caught are:
/// - Insufficient free blocks to create an extent tree
/// - Insufficient free blocks to create a dentry
/// - Insufficient free inodes in the new ext4 file system
/// - File name too long
/// - Regular file has more than u32::MAX blocks
/// - Directory has more than u32::MAX blocks
impl<'a> DryRunDeserializer<'a> {
    pub fn dry_run(
        reader: Reader<'a>,
        free_inodes: InodeCount,
        free_blocks: BlockCount,
        block_size: BlockSize,
    ) -> Result<()> {
        let mut instance = Self {
            internals: DryRunDeserializerInternals::new(reader, block_size),
            _lifetime: PhantomData,
        };
        instance.deserialize_directory_tree()?;
        instance.internals.result(free_inodes, free_blocks)
    }
}

pub struct DryRunDeserializerInternals<'a> {
    reader: Reader<'a>,
    used_inodes: InodeCount,
    used_blocks: BlockCount,
    block_size: BlockSize,
}

impl<'a> DryRunDeserializerInternals<'a> {
    pub fn new(reader: Reader<'a>, block_size: BlockSize) -> Self {
        Self { reader, used_inodes: 0, used_blocks: 0, block_size }
    }

    // We perform the entire dry run and return a Result only afterward instead of bailing as soon a we know it will
    // fail. This is better because it lets the user know the required inode/block count.
    fn result(&self, free_inodes: InodeCount, free_blocks: BlockCount) -> Result<()> {
        let enough_inodes = self.used_inodes <= free_inodes;
        let enough_blocks = self.used_blocks <= free_blocks;
        match (enough_inodes, enough_blocks) {
            (true, true) => Ok(()),
            (true, false) => bail!("{} free blocks required but only {} available", self.used_blocks, free_blocks),
            (false, true) => bail!("{} free inodes required but only {} available", self.used_inodes, free_inodes),
            (false, false) => bail!(
                "{} free blocks required but only {} available; {} inodes required but only {} available",
                self.used_blocks,
                free_blocks,
                self.used_inodes,
                free_inodes
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
        let mut dir_writer = DryRunDirectoryWriter::new(self.block_size);
        self.used_blocks += dir_writer.add_dot_dirs()?;
        self.build_directory("lost+found".to_string(), &mut dir_writer)?;
        Ok(dir_writer)
    }

    fn deserialize_directory(
        &mut self,
        _dentry: DentryRepresentation,
        name: String,
        parent_directory_writer: &mut DryRunDirectoryWriter,
    ) -> Result<DryRunDirectoryWriter> {
        self.build_directory(name, parent_directory_writer)
    }

    fn deserialize_regular_file(
        &mut self,
        _dentry: DentryRepresentation,
        name: String,
        data_ranges: Vec<Range<ClusterIdx>>,
        parent_directory_writer: &mut DryRunDirectoryWriter,
    ) -> Result<()> {
        self.build_regular_file(name, parent_directory_writer, data_ranges)
    }
}

impl<'a> DryRunDeserializerInternals<'a> {
    fn build_directory(
        &mut self,
        name: String,
        parent_directory_writer: &mut DryRunDirectoryWriter,
    ) -> Result<DryRunDirectoryWriter> {
        let mut dir_writer = DryRunDirectoryWriter::new(self.block_size);
        self.used_inodes += 1;
        self.used_blocks += parent_directory_writer.add_dentry(&Ext4Dentry::new(0, name)?)?;
        self.used_blocks += dir_writer.add_dot_dirs()?;
        Ok(dir_writer)
    }

    fn build_regular_file(
        &mut self,
        name: String,
        parent_directory_writer: &mut DryRunDirectoryWriter,
        data_ranges: Vec<Range<ClusterIdx>>,
    ) -> Result<()> {
        self.used_inodes += 1;
        self.used_blocks += parent_directory_writer.add_dentry(&Ext4Dentry::new(0, name)?)?;
        let data_ranges_iter = data_ranges
            .into_iter()
            .map(|range| BlockIdx::fromx(range.start)..BlockIdx::fromx(range.end));
        let extents = Extent::from_ranges(data_ranges_iter)?;
        self.used_blocks += ExtentTree::required_block_count(extents.len(), self.block_size);
        Ok(())
    }
}

pub struct DryRunDirectoryWriter {
    used_dentry_blocks: u32, // a file's block count must fit into a u32
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

    fn add_dot_dirs(&mut self) -> Result<usize> {
        let mut added_blocks = self.add_dentry(&Ext4Dentry::new(0, ".".to_string()).unwrap())?;
        added_blocks += self.add_dentry(&Ext4Dentry::new(0, "..".to_string()).unwrap())?;
        Ok(added_blocks)
    }

    fn add_dentry(&mut self, dentry: &Ext4Dentry) -> Result<usize> {
        let old_used_blocks = self.used_blocks();
        if u32::from(dentry.dentry_len()) > self.remaining_space() {
            self.used_dentry_blocks = self
                .used_dentry_blocks
                .checked_add(1)
                .context("Directory contains too many files")?;
            // This only fails with billions of files, so it's just a formality.
            self.position_in_block = 0;
            self.used_extent_blocks =
                ExtentTree::required_block_count(BlockCount::fromx(self.used_dentry_blocks), self.block_size);
        }
        self.position_in_block += u32::from(dentry.dentry_len());

        Ok(self.used_blocks() - old_used_blocks)
    }

    fn used_blocks(&self) -> usize {
        BlockCount::fromx(self.used_dentry_blocks) + self.used_extent_blocks
    }

    fn remaining_space(&self) -> u32 {
        self.block_size - self.position_in_block
    }
}
