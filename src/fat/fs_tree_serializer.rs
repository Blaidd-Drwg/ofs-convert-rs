use std::convert::{TryFrom, TryInto};
use std::ops::Range;
use std::rc::Rc;

use chrono::prelude::*;

use crate::allocator::{AllocatedClusterIdx, Allocator};
use crate::c_wrapper::{c_add_extent, c_build_inode, c_build_lost_found_inode, c_build_root_inode, c_get_inode};
use crate::ext4::{Ext4Dentry, Ext4DentrySized};
use crate::fat::{ClusterIdx, FatDentry, FatFile, FatPartition, ROOT_FAT_IDX};
use crate::ranges::Ranges;
use crate::stream_archiver::{Reader, StreamArchiver};


type Timestamp = u32;

#[derive(Clone, Copy)]
enum FileType {
    Directory(u32), // contains child count
    RegularFile,
}

pub struct FsTreeSerializer<'a> {
    allocator: Rc<Allocator<'a>>, /* Rc to be shared with `self.stream_archiver` and nobody else, otherwise
                                   * `into_deserializer` will panic */
    stream_archiver: StreamArchiver<'a>,
    forbidden_ranges: Ranges<ClusterIdx>, /* ranges that cannot contain any data as they will be overwritten with
                                           * ext4 metadata */
}

/// A slimmed down representation of the relevant components of a FAT dentry for serialization
/// This excludes the file name and the file's data ranges: since they have variable length,
/// they are treated separately.
struct DentryRepresentation {
    access_time: Timestamp,
    create_time: Timestamp,
    mod_time: Timestamp,
}

impl DentryRepresentation {
    pub fn from(dentry: &FatDentry) -> Self {
        Self {
            access_time: fat_time_to_unix_time(dentry.access_date, None),
            create_time: fat_time_to_unix_time(dentry.create_date, Some(dentry.create_time)),
            mod_time: fat_time_to_unix_time(dentry.mod_date, Some(dentry.mod_time)),
        }
    }
}

pub fn fat_time_to_unix_time(date: u16, time: Option<u16>) -> u32 {
    let year = ((date & 0xFE00) >> 9) + 1980;
    let month = (date & 0x1E0) >> 5;
    let day = date & 0x1F;
    let date = Utc.ymd(i32::from(year), u32::from(month), u32::from(day));

    let mut hour = 0;
    let mut minute = 0;
    let mut second = 0;
    if let Some(time) = time {
        hour = (time & 0xF800) >> 11;
        minute = (time & 0x7E0) >> 5;
        second = (time & 0x1F) * 2;
    }

    let datetime = date.and_hms(u32::from(hour), u32::from(minute), u32::from(second));
    u32::try_from(datetime.timestamp()).expect("Timestamp after year 2038 does not fit into 32 bits")
}

impl<'a> FsTreeSerializer<'a> {
    pub fn new(allocator: Allocator<'a>, cluster_size: usize, forbidden_ranges: Ranges<ClusterIdx>) -> Self {
        let allocator = Rc::new(allocator);
        let stream_archiver = StreamArchiver::new(allocator.clone(), cluster_size);
        Self { allocator, stream_archiver, forbidden_ranges }
    }

    pub fn serialize_directory_tree(&mut self, partition: &FatPartition) {
        // SAFETY: safe because `ROOT_FAT_IDX` belongs to the root directory
        let root_child_count = unsafe { partition.dir_content_iter(ROOT_FAT_IDX).count() };
        self.archive_root_child_count(root_child_count.try_into().unwrap());

        // SAFETY: safe because `ROOT_FAT_IDX` belongs to the root directory
        for file in unsafe { partition.dir_content_iter(ROOT_FAT_IDX) } {
            if file.dentry.is_dir() {
                self.serialize_directory(file, partition);
            } else {
                self.archive_file(file, partition);
            }
        }
    }

    fn serialize_directory(&mut self, file: FatFile, partition: &FatPartition) {
        assert!(file.dentry.is_dir());
        let first_fat_idx = file.dentry.first_fat_index();
        // SAFETY: safe because `first_fat_index` belongs to a directory
        let child_count = unsafe { partition.dir_content_iter(first_fat_idx).count() };
        self.archive_directory(file, child_count.try_into().unwrap());

        // SAFETY: safe because `first_fat_index` belongs to a directory
        for file in unsafe { partition.dir_content_iter(first_fat_idx) } {
            if file.dentry.is_dir() {
                self.serialize_directory(file, partition);
            } else {
                self.archive_file(file, partition);
            }
        }
    }

    pub fn archive_root_child_count(&mut self, root_child_count: u32) {
        self.stream_archiver.archive(vec![FileType::Directory(root_child_count)]);
    }

    pub fn archive_file(&mut self, mut file: FatFile, partition: &FatPartition) {
        self.copy_data_to_unforbidden(&mut file, partition);
        self.stream_archiver.archive(vec![FileType::RegularFile]);
        self.stream_archiver.archive(vec![file.dentry]);
        self.stream_archiver.archive(file.name.into_bytes());
        self.stream_archiver.archive(file.data_ranges);
    }

    pub fn archive_directory(&mut self, file: FatFile, child_count: u32) {
        self.stream_archiver.archive(vec![FileType::Directory(child_count)]);
        self.stream_archiver.archive(vec![file.dentry]);
        self.stream_archiver.archive(file.name.into_bytes());
    }

    fn copy_data_to_unforbidden(&self, file: &mut FatFile, partition: &FatPartition) {
        let old_ranges = std::mem::take(&mut file.data_ranges);
        for range in old_ranges {
            for (range_fragment, forbidden) in self.forbidden_ranges.split_overlapping(range) {
                if forbidden {
                    let mut copied_ranges = self.copy_range_to_unforbidden(range_fragment, partition);
                    file.data_ranges.append(&mut copied_ranges);
                } else {
                    file.data_ranges.push(range_fragment);
                }
            }
        }
    }

    fn copy_range_to_unforbidden(
        &self,
        mut range: Range<ClusterIdx>,
        partition: &FatPartition,
    ) -> Vec<Range<ClusterIdx>> {
        let mut copied_fragments = Vec::new();
        while !range.is_empty() {
            let mut allocated = self.allocator.allocate(range.len());
            let allocated_len = allocated.len();
            for (old_cluster_idx, mut new_cluster_idx) in range.clone().zip(allocated.iter_mut()) {
                let old_data_cluster_idx = partition.cluster_idx_to_data_cluster_idx(old_cluster_idx).unwrap();
                let old_cluster = partition.data_cluster(old_data_cluster_idx);
                self.allocator.cluster_mut(&mut new_cluster_idx).copy_from_slice(old_cluster);
            }
            range.start += allocated_len as u32;
            copied_fragments.push(allocated.into());
        }
        copied_fragments
    }

    pub fn into_deserializer(self) -> FsTreeDeserializer<'a> {
        std::mem::drop(self.allocator); // drop the Rc, allowing `self.stream_archiver` to unwrap it
        let (reader, allocator) = self.stream_archiver.into_reader();
        FsTreeDeserializer { reader, allocator: Rc::new(allocator) }
    }
}

pub struct FsTreeDeserializer<'a> {
    reader: Reader<'a>,
    allocator: Rc<Allocator<'a>>,
}

impl<'a> FsTreeDeserializer<'a> {
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
