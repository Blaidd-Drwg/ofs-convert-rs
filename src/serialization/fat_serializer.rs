use std::convert::{TryFrom, TryInto};
use std::ops::Range;
use std::rc::Rc;

use chrono::prelude::*;

use crate::allocator::Allocator;
use crate::fat::{ClusterIdx, FatDentry, FatFile, FatPartition, ROOT_FAT_IDX};
use crate::ranges::Ranges;
use crate::serialization::{ExtTreeDeserializer, FileType, StreamArchiver};


type Timestamp = u32;

pub struct FatTreeSerializer<'a> {
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

impl<'a> FatTreeSerializer<'a> {
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

    pub fn into_deserializer(self) -> ExtTreeDeserializer<'a> {
        std::mem::drop(self.allocator); // drop the Rc, allowing `self.stream_archiver` to unwrap it
        let (reader, allocator) = self.stream_archiver.into_reader();
        ExtTreeDeserializer::new(reader, allocator)
    }
}
