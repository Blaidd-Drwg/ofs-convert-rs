use crate::fat::{FatPartition, FatPseudoDentry, ClusterIdx, FatFile};
use std::iter::Peekable;
use itertools::free::join;

pub struct FatFileIter<'a, I> where I: Iterator<Item = &'a FatPseudoDentry> {
    pseudo_dentry_iter: Peekable<I>,
}

impl<'a, I> FatFileIter<'a, I> where I: Iterator<Item = &'a FatPseudoDentry> {
    pub fn new(pseudo_dentry_iter: I) -> Self {
        Self { pseudo_dentry_iter: pseudo_dentry_iter.peekable() }
    }
}

impl<'a, I> Iterator for FatFileIter<'a, I> where I: Iterator<Item = &'a FatPseudoDentry> {
    type Item = FatFile;
    fn next(&mut self) -> Option<Self::Item> {
        let file_name;
        let dentry;
        if self.pseudo_dentry_iter.peek()?.is_long_file_name() {
            file_name = self.read_long_file_name();
            dentry = self.pseudo_dentry_iter.next()?.as_dentry().unwrap();
        } else {
            dentry = self.pseudo_dentry_iter.next().unwrap().as_dentry().unwrap();
            file_name = dentry.read_short_file_name();
        }

        let file = FatFile {
            name: file_name,
            dentry: *dentry,
            data: Vec::new(), // TODO
        };
        Some(file)
    }
}

/// Caller must ensure that self.pseudo_dentry_iter.next() is a LongFileName
impl<'a, I> FatFileIter<'a, I> where I: Iterator<Item = &'a FatPseudoDentry> {
    fn read_long_file_name(&mut self) -> String {
        let first_entry = self.pseudo_dentry_iter.next().unwrap().as_long_file_name().unwrap();
        let mut file_name_components = vec![first_entry.to_utf8_string()];

        let remaining_entry_count = first_entry.sequence_no() - 1; // we already have read one entry and the sequence number is 1-based
        for _ in 0..remaining_entry_count {
            let long_file_name = self.pseudo_dentry_iter
                .next()
                .and_then(|pseudo_dentry| pseudo_dentry.as_long_file_name())
                .expect("FAT partition contains malformed LFN entry");
            file_name_components.push(long_file_name.to_utf8_string());
        }
        join(file_name_components.into_iter().rev(), "")
    }
}

pub struct FatPseudoDentryIter<'a, I> where I: Iterator<Item = ClusterIdx> {
    cluster_idx_iter: I,
    current_cluster: Option<&'a [FatPseudoDentry]>,
    current_dentry_idx: usize,
    partition: &'a FatPartition<'a>,
    dentries_per_cluster: usize,
}

impl<'a, I> Iterator for FatPseudoDentryIter<'a, I> where I: Iterator<Item = ClusterIdx> {
    type Item = &'a FatPseudoDentry;
    fn next(&mut self) -> Option<Self::Item> {
        let mut dentry = self.try_next();
        while dentry.is_some() && dentry.unwrap().should_be_ignored() {
            dentry = self.try_next();
        }

        if dentry.is_some() && dentry.unwrap().is_dir_table_end() {
            return None;
        }

        dentry
    }
}


impl<'a, I> FatPseudoDentryIter<'a, I> where I: Iterator<Item = ClusterIdx> {
    pub fn new(cluster_idx_iter: I, partition: &'a FatPartition<'a>, dentries_per_cluster: usize) -> Self {
        let mut cluster_idx_iter = cluster_idx_iter;
        let cluster_idx = cluster_idx_iter.next();
        let current_cluster = cluster_idx.and_then(|cluster_idx| unsafe {
            let cluster = partition.cluster(cluster_idx);
            let (_, dentries, _) = cluster.align_to::<FatPseudoDentry>();
            Some(dentries)
        });
        Self {
            cluster_idx_iter,
            current_cluster,
            current_dentry_idx: 0,
            partition,
            dentries_per_cluster,
        }
    }

    /// Possibly invalid or dot dir
    fn try_next(&mut self) -> Option<&'a FatPseudoDentry> {
        if self.current_cluster.is_none() {
            return None;
        }

        if self.current_dentry_idx >= self.dentries_per_cluster {
            let cluster_idx = self.cluster_idx_iter.next()?;
            let new_cluster = self.partition.cluster(cluster_idx);
            unsafe {
                let (_, dentries, _) = new_cluster.align_to::<FatPseudoDentry>();
                self.current_cluster = Some(dentries);
            }
            self.current_dentry_idx = 0;
        }
        let dentry = &self.current_cluster.unwrap()[self.current_dentry_idx];
        self.current_dentry_idx += 1;
        Some(dentry)
    }
}
