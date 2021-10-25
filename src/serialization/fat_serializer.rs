use std::cell::RefCell;
use std::convert::TryInto;
use std::ops::Range;
use std::rc::Rc;

use anyhow::Result;

use crate::allocator::Allocator;
use crate::fat::{ClusterIdx, DataClusterIdx, FatDentry, FatFile, FatFs, FatTableIndex, ROOT_FAT_IDX};
use crate::ranges::Ranges;
use crate::serialization::{DentryRepresentation, Ext4TreeDeserializer, FileType, StreamArchiver};
use crate::util::FromU32;


pub struct FatTreeSerializer<'a> {
    fat_fs: FatFs<'a>,
    allocator: Rc<Allocator<'a>>, /* Rc to be shared with `self.stream_archiver` and nobody else, otherwise
                                   * `into_deserializer` will panic */
    stream_archiver: RefCell<StreamArchiver<'a>>, /* `serialize_directory` borrows `self.fat_fs` twice, so it has
                                                   * to borrow `self` immutably. However, it also needs to mutate
                                                   * `self.stream_archiver`, so we wrap it in a RefCell. */
    forbidden_ranges: Ranges<ClusterIdx>, /* ranges that cannot contain any data as they will be overwritten with
                                           * ext4 metadata */
}

impl<'a> FatTreeSerializer<'a> {
    pub fn new(allocator: Allocator<'a>, fat_fs: FatFs<'a>, forbidden_ranges: Ranges<ClusterIdx>) -> Self {
        let allocator = Rc::new(allocator);
        let stream_archiver = StreamArchiver::new(allocator.clone(), usize::fromx(fat_fs.cluster_size()));
        Self {
            allocator,
            fat_fs,
            stream_archiver: RefCell::new(stream_archiver),
            forbidden_ranges,
        }
    }

    pub fn serialize_directory_tree(&mut self) -> Result<()> {
        // SAFETY: safe because `ROOT_FAT_IDX` belongs to the root directory
        let root_child_count = unsafe { self.fat_fs.dir_content_iter(ROOT_FAT_IDX).count() };
        self.archive_root_child_count(root_child_count.try_into().unwrap())?;
        // SAFETY: safe because `ROOT_FAT_IDX` belongs to the root directory
        unsafe { self.serialize_directory_content(ROOT_FAT_IDX) }
    }

    fn serialize_directory(&self, file: FatFile) -> Result<()> {
        assert!(file.dentry.is_dir());
        let first_fat_idx = file.dentry.first_fat_index();
        // SAFETY: safe because `first_fat_index` belongs to a directory
        let child_count = unsafe { self.fat_fs.dir_content_iter(first_fat_idx).count() };
        self.archive_directory(file, child_count.try_into().unwrap())?;
        // SAFETY: safe because `first_fat_index` belongs to a directory
        unsafe {
            self.serialize_directory_content(first_fat_idx)?;
        }
        Ok(())
    }

    /// SAFETY: safe if `first_fat_idx` points to a cluster belonging to a directory
    unsafe fn serialize_directory_content(&self, first_fat_idx: FatTableIndex) -> Result<()> {
        // SAFETY: safe because `first_fat_index` belongs to a directory
        for file in self.fat_fs.dir_content_iter(first_fat_idx) {
            if file.dentry.is_dir() {
                self.serialize_directory(file)?;
            } else {
                let non_overlapping = self.make_file_non_overlapping(file)?;
                self.archive_regular_file(non_overlapping)?;
            }
        }
        Ok(())
    }

    fn archive_root_child_count(&self, root_child_count: u32) -> Result<()> {
        let mut archiver = self.stream_archiver.borrow_mut();
        archiver.archive(vec![FileType::Directory(root_child_count)])?;
        Ok(())
    }

    fn archive_regular_file(&self, file: NonOverlappingFatFile) -> Result<()> {
        let mut archiver = self.stream_archiver.borrow_mut();
        archiver.archive(vec![FileType::RegularFile])?;
        archiver.archive(vec![DentryRepresentation::from(file.dentry)?])?;
        archiver.archive(file.name.into_bytes())?;
        archiver.archive(file.data_ranges)?;
        Ok(())
    }

    fn archive_directory(&self, file: FatFile, child_count: u32) -> Result<()> {
        let mut archiver = self.stream_archiver.borrow_mut();
        archiver.archive(vec![FileType::Directory(child_count)])?;
        archiver.archive(vec![DentryRepresentation::from(file.dentry)?])?;
        archiver.archive(file.name.into_bytes())?;
        Ok(())
    }

    fn make_file_non_overlapping(&self, file: FatFile) -> Result<NonOverlappingFatFile> {
        let mut non_overlapping = NonOverlappingFatFile::new(file.name, file.dentry);

        for mut data_cluster_range in file.data_ranges {
            let start_cluster_idx = self.fat_fs.cluster_from_data_cluster(*data_cluster_range.start());
            let end_cluster_idx = self.fat_fs.cluster_from_data_cluster(*data_cluster_range.end()) + 1;
            let cluster_range = start_cluster_idx..end_cluster_idx;

            for (range_fragment, forbidden) in self.forbidden_ranges.split_overlapping(cluster_range) {
                if forbidden {
                    let mut copied_ranges =
                        self.copy_data_to_new_clusters(&mut data_cluster_range, range_fragment.len())?;
                    non_overlapping.data_ranges.append(&mut copied_ranges);
                } else {
                    data_cluster_range
                        .advance_by(range_fragment.len())
                        .expect("data_cluster_range is shorter than the sum of range_fragment lengths");
                    non_overlapping.data_ranges.push(range_fragment);
                }
            }
        }
        Ok(non_overlapping)
    }

    /// Given an iterator over `DataClusterIdx`s, copy the first `len` to newly allocated clusters and return these
    /// clusters' `ClusterIdx`s. `iter` must have at least `len` elements.
    fn copy_data_to_new_clusters<I: Iterator<Item = DataClusterIdx>>(
        &self,
        mut iter: &mut I,
        mut len: usize,
    ) -> Result<Vec<Range<ClusterIdx>>> {
        let mut copied_fragments = Vec::new();
        while len > 0 {
            let mut allocated = self.allocator.allocate(len)?;
            // zip in this order: this way, when `allocated` is empty, `iter.next()` is not called, and we consume
            // exactly `allocated.len()` elements from `iter`.
            for (mut new_cluster_idx, old_data_cluster_idx) in allocated.iter_mut().zip(&mut iter) {
                let old_cluster = self.fat_fs.data_cluster(old_data_cluster_idx);
                self.allocator.cluster_mut(&mut new_cluster_idx).copy_from_slice(old_cluster);
            }
            len -= allocated.len();
            copied_fragments.push(allocated.into());
        }
        Ok(copied_fragments)
    }

    /// SAFETY: Safe if no block in `SuperBlock::from(self.fat_fs.boot_sector).block_group_overhead_ranges()` is
    /// accessed for the duration of the lifetime 'a
    pub unsafe fn into_deserializer(self) -> Result<Ext4TreeDeserializer<'a>> {
        std::mem::drop(self.allocator); // drop the Rc, allowing `self.stream_archiver` to unwrap it
        let (reader, allocator) = self.stream_archiver.into_inner().into_reader()?;
        Ext4TreeDeserializer::new_with_dry_run(reader, allocator, self.fat_fs)
    }
}

struct NonOverlappingFatFile {
    pub name: String,
    pub dentry: FatDentry,
    pub data_ranges: Vec<Range<ClusterIdx>>,
}

impl NonOverlappingFatFile {
    pub fn new(name: String, dentry: FatDentry) -> Self {
        Self { name, dentry, data_ranges: Vec::new() }
    }
}
