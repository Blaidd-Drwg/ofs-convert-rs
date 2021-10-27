use std::iter::Peekable;

use itertools::free::join;

use crate::fat::{FatFile, FatFs, FatPseudoDentry, FatTableIndex};
use crate::util::ExactAlign;

pub struct FatFileIter<'a, I>
where I: Iterator<Item = &'a FatPseudoDentry>
{
    pseudo_dentry_iter: Peekable<I>,
    fat_fs: &'a FatFs<'a>,
}

impl<'a> FatFileIter<'a, FatPseudoDentryIter<'a, FatIdxIter<'a>>> {
    /// SAFETY: safe if `start_fat_idx` belongs to a directory
    pub unsafe fn new(start_fat_idx: FatTableIndex, fat_fs: &'a FatFs<'a>) -> Self {
        let pseudo_dentry_iter = unsafe { FatPseudoDentryIter::new(start_fat_idx, fat_fs) };
        Self::from_pseudo_dentry_iter(pseudo_dentry_iter, fat_fs)
    }
}

impl<'a, I> FatFileIter<'a, I>
where I: Iterator<Item = &'a FatPseudoDentry>
{
    pub fn from_pseudo_dentry_iter(pseudo_dentry_iter: I, fat_fs: &'a FatFs<'a>) -> Self {
        Self {
            pseudo_dentry_iter: pseudo_dentry_iter.peekable(),
            fat_fs,
        }
    }
}

impl<'a, I> Iterator for FatFileIter<'a, I>
where I: Iterator<Item = &'a FatPseudoDentry>
{
    type Item = FatFile;
    fn next(&mut self) -> Option<Self::Item> {
        let file_name;
        let dentry;
        if self.pseudo_dentry_iter.peek()?.is_dentry() {
            dentry = self.pseudo_dentry_iter.next().unwrap().as_dentry().unwrap();
            file_name = dentry.read_short_file_name();
        } else {
            file_name = self.read_long_file_name();
            dentry = self
                .pseudo_dentry_iter
                .next()?
                .as_dentry()
                .expect("FAT long file name not followed by a dentry");
        }

        let file = FatFile {
            name: file_name,
            dentry: *dentry,
            data_ranges: self.fat_fs.data_ranges(dentry.first_fat_index()),
        };
        Some(file)
    }

    fn count(self) -> usize {
        self.pseudo_dentry_iter.filter(|pseudo_dentry| pseudo_dentry.is_dentry()).count()
    }
}

impl<'a, I> FatFileIter<'a, I>
where I: Iterator<Item = &'a FatPseudoDentry>
{
    /// PANICS: Panics if `self.pseudo_dentry_iter.next()` is not a `LongFileName`
    fn read_long_file_name(&mut self) -> String {
        let first_entry = self.pseudo_dentry_iter.next().unwrap().as_long_file_name().unwrap();
        let mut file_name_components = vec![first_entry.to_utf8_string()];

        let mut lfn_entries = vec![first_entry.to_utf16_string()];

        let remaining_entry_count = first_entry.sequence_no() - 1; // we already have read one entry and the sequence number is 1-based
        for _ in 0..remaining_entry_count {
            let long_file_name = self
                .pseudo_dentry_iter
                .next()
                .and_then(FatPseudoDentry::as_long_file_name)
                .expect("FAT filesystem contains malformed LFN entry");
            file_name_components.push(long_file_name.to_utf8_string());
            lfn_entries.push(long_file_name.to_utf16_string());
        }
        join(file_name_components.into_iter().rev(), "")
    }
}

/// Given the index of a directory's initial data cluster, iterates over the directory's valid
/// pseudo-dentries (excluding the '.' and '..' directories.
pub struct FatPseudoDentryIter<'a, I>
where I: Iterator<Item = FatTableIndex>
{
    fat_idx_iter: I,
    current_cluster: Option<&'a [FatPseudoDentry]>,
    current_dentry_idx: usize,
    fat_fs: &'a FatFs<'a>,
}

impl<'a> FatPseudoDentryIter<'a, FatIdxIter<'a>> {
    /// SAFETY: Safe if `start_fat_idx` belongs to a directory
    pub unsafe fn new(start_fat_idx: FatTableIndex, fat_fs: &'a FatFs<'a>) -> Self {
        let fat_idx_iter = FatIdxIter::new(start_fat_idx, fat_fs.fat_table());
        unsafe { Self::from_cluster_iter(fat_idx_iter, fat_fs) }
    }
}

impl<'a, I> FatPseudoDentryIter<'a, I>
where I: Iterator<Item = FatTableIndex>
{
    /// SAFETY: Safe only if `fat_idx_iter` iterates only over clusters belonging to a directory
    pub unsafe fn from_cluster_iter(fat_idx_iter: I, fat_fs: &'a FatFs<'a>) -> Self {
        let mut instance = Self {
            fat_idx_iter,
            current_cluster: None,
            current_dentry_idx: 0,
            fat_fs,
        };
        instance.get_next_cluster();
        instance
    }


    /// Possibly invalid or dot dir
    fn try_next(&mut self) -> Option<&'a FatPseudoDentry> {
        self.current_cluster?;
        if self.current_dentry_idx >= self.fat_fs.dentries_per_cluster() {
            self.get_next_cluster();
            self.current_dentry_idx = 0;
        }

        let dentry = &self.current_cluster?[self.current_dentry_idx];
        self.current_dentry_idx += 1;
        Some(dentry)
    }

    fn get_next_cluster(&mut self) {
        self.current_cluster = self.fat_idx_iter.next().map(|fat_idx| {
            let cluster = self.fat_fs.data_cluster(fat_idx.to_data_cluster_idx());
            // SAFETY: safe, since directory data is a sequence of pseudo-dentries
            let dentries = unsafe { cluster.exact_align_to::<FatPseudoDentry>() };
            assert_eq!(dentries.len(), self.fat_fs.dentries_per_cluster());
            dentries
        });
    }
}
impl<'a, I> Iterator for FatPseudoDentryIter<'a, I>
where I: Iterator<Item = FatTableIndex>
{
    type Item = &'a FatPseudoDentry;
    fn next(&mut self) -> Option<Self::Item> {
        let mut dentry = self.try_next();
        while dentry.is_some() && dentry.unwrap().should_be_ignored() {
            dentry = self.try_next();
        }

        dentry.filter(|dentry| !dentry.is_dir_table_end())
    }
}


/// Given the index of a file's initial data cluster, iterates over the file's data cluster indices.
pub struct FatIdxIter<'a> {
    current_fat_idx: FatTableIndex,
    fat_table: &'a [FatTableIndex],
}

impl<'a> FatIdxIter<'a> {
    pub fn new(start_fat_idx: FatTableIndex, fat_table: &'a [FatTableIndex]) -> Self {
        Self { current_fat_idx: start_fat_idx, fat_table }
    }
}

impl<'a> Iterator for FatIdxIter<'a> {
    type Item = FatTableIndex;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_fat_idx.is_chain_end() || self.current_fat_idx.is_zero_length_file() {
            None
        } else {
            let result = self.current_fat_idx;
            self.current_fat_idx = self.fat_table[result];
            Some(result)
        }
    }
}
