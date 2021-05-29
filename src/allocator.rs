use crate::ranges::{Ranges, NotCoveredRange};
use crate::fat::{ClusterIdx, FatPartition};
use crate::partition::Partition;
use std::ops::Range;
use std::io;

// TODO ensure DataClusterIdx can also not be constructed
#[derive(Clone, Copy)]
pub struct AllocatedClusterIdx(ClusterIdx);
impl AllocatedClusterIdx {
    pub fn to_ne_bytes(self) -> [u8; 4] {
        self.0.to_ne_bytes()
    }
}

impl From<AllocatedClusterIdx> for usize {
    fn from(idx: AllocatedClusterIdx) -> Self {
        idx.0 as usize
    }
}

pub struct Allocator<'a> {
    partition_data: &'a mut [u8],
    cursor: ClusterIdx, // TODO document
    used_ranges: Ranges<ClusterIdx>, // clusters used by FAT, overwriting will make FAT inconsistent
    forbidden_ranges: Ranges<ClusterIdx>, // clusters reserved for ext4 metadata
    cluster_size: usize,
}

// need ranges for: stream archiver; move data
// move data: check if intersect with forbidden, move there
// stream archiver: return first that is not forbidden or in use

// A cluster can be forbidden, in use, or free. Forbidden clusters will be overwritten with ext4
// metadata later on, so they cannot contain any data. In use clusters already contain data. Free
// clusters can be used to store new data.

// cursor from start to finish (option for later on: smarter allocation system)
// interface:
    // allocate(x): return at least x free clusters, needed for archiver and moving
    // forbid(x..y), marks block group header range as forbidden
    // determine_forbidden(x..y), returns Vec<(subrange, forbidden)>
impl<'a> Allocator<'a> {
    pub fn new(partition: &'a Partition, fat_partition: &FatPartition) -> Self {
        // SAFETY: we crate a mutable reference to data that is already borrowed by
        // `fat_partition`. To avoid TODO, we divide the partition into used clusters (i.e. the
        // reserved clusters, the FAT clusters, and the data clusters that contain data) and unused
        // clusters (i.e. the data clusters that contain no data). The FAT partition will only ever
        // read used clusters. The allocator will only ever read and write unused clusters.
        let partition_data = unsafe {
            let data_ptr = partition.as_ptr() as *mut u8;
            std::slice::from_raw_parts_mut(data_ptr, partition.len())
        };
        let used_ranges = fat_partition.used_ranges();
        Self {
            partition_data,
            cursor: 0,
            used_ranges,
            forbidden_ranges: Ranges::new(),
            cluster_size: fat_partition.cluster_size(),
        }
    }

    pub fn forbid(&mut self, range: Range<ClusterIdx>) {
        self.forbidden_ranges.insert(range);
    }

    // returns a memory slice with 1 <= `slice.len()` <= `max_length`
    // TODO error handling
    pub fn allocate(&mut self, max_length: usize) -> Range<AllocatedClusterIdx> {
        let free_range = self.find_next_free_range().expect("Oh no, no more free blocks :(((");
        let range_end = free_range.end.min(free_range.start + max_length as u32);
        self.cursor = range_end;
        AllocatedClusterIdx(free_range.start)..AllocatedClusterIdx(range_end)
    }

    pub fn cluster(&self, idx: AllocatedClusterIdx) -> &[u8] {
        let start_byte = self.cluster_size * usize::from(idx);
        &self.partition_data[start_byte..start_byte+self.cluster_size]
    }

    pub fn cluster_mut(&mut self, idx: AllocatedClusterIdx) -> &mut [u8] {
        let start_byte = self.cluster_size * usize::from(idx);
        &mut self.partition_data[start_byte..start_byte+self.cluster_size]
    }

    /// Returns the next range at or after `self.cursor` that is neither used nor forbidden, or an
    /// Error if such a range does not exist.
    fn find_next_free_range(&self) -> Result<Range<ClusterIdx>, io::Error> {
        let max_cluster_idx = 0; // TODO
        let mut cursor = self.cursor;
        loop {
            let non_used_range = self.used_ranges.next_not_covered(cursor);
            let non_used_range = match non_used_range {
                NotCoveredRange::Bounded(range) => range,
                NotCoveredRange::Unbounded(start) => start..max_cluster_idx,
            };

            if non_used_range.is_empty() {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "No free clusters left in the partition"));
            }

            let non_forbidden_range = self.forbidden_ranges
                .split_overlapping(non_used_range.clone())
                .into_iter()
                .find(|(_range, is_forbidden)| !is_forbidden)
                .map(|(range, _is_ok)| range);

            if let Some(non_forbidden_range) = non_forbidden_range {
                return Ok(non_forbidden_range);
            } else {
                cursor = non_used_range.end;
            }
        }
    }
}
