use crate::fat::ClusterIdx;
use std::ops::Range;


// a set of non-overlapping ranges
struct ClusterRanges {
    ranges: Vec<Range<ClusterIdx>>, // invariant: non-overlapping, sorted
}

impl ClusterRanges {
    pub fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    /// Inserts `range` into `self.ranges` in the correct position and merging it with other ranges
    /// in case they overlap.
    pub fn insert(&mut self, range: Range<ClusterIdx>) {
        // the index of the first range which ends after `range` starts
        let first_merge_candidate_index = match self.ranges.binary_search_by_key(&range.start, |candidate| candidate.end,) {
            Ok(result) | Err(result) => result
        };

        // every range ends before `range` starts, so we can simply append it
        if first_merge_candidate_index == self.ranges.len() {
            self.ranges.push(range);
            return;
        }

        let mut overlapping_ranges = first_merge_candidate_index..first_merge_candidate_index;
        while overlapping_ranges.end < self.ranges.len() && self.ranges[overlapping_ranges.end].start <= range.end {
            overlapping_ranges.end += 1;
        }

        // no range overlaps `range`, so we can simply insert it
        if overlapping_ranges.is_empty() {
            self.ranges.insert(overlapping_ranges.start, range);
            return;
        }

        // one or more ranges overlap `range`, we merge them into one
        let merged_range_start = self.ranges[overlapping_ranges.start].start.min(range.start);
        let merged_range_end = self.ranges[overlapping_ranges.end - 1].end.max(range.end);
        let merged_range = merged_range_start..merged_range_end;

        // replace the first of the overlapping ranges with `merged_range` and delete the rest
        self.ranges[overlapping_ranges.start] = merged_range;
        overlapping_ranges.start += 1;
        self.ranges.drain(overlapping_ranges);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_range() {
        let mut ranges = ClusterRanges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(3..5);
        assert_eq!(ranges.ranges, vec![0..2, 3..5, 6..9, 11..14]);
    }

    #[test]
    fn pushes_range() {
        let mut ranges = ClusterRanges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(15..16);
        assert_eq!(ranges.ranges, vec![0..2, 6..9, 11..14, 15..16]);
    }

    #[test]
    fn merges_subrange() {
        let mut ranges = ClusterRanges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(5..10);
        assert_eq!(ranges.ranges, vec![0..2, 5..10, 11..14]);
    }

    #[test]
    fn merges_superrange() {
        let mut ranges = ClusterRanges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(7..9);
        assert_eq!(ranges.ranges, vec![0..2, 6..9, 11..14]);
    }

    #[test]
    fn merges_multiple_subranges() {
        let mut ranges = ClusterRanges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(5..15);
        assert_eq!(ranges.ranges, vec![0..2, 5..15]);
    }

    #[test]
    fn merges_multiple_ranges() {
        let mut ranges = ClusterRanges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(8..12);
        assert_eq!(ranges.ranges, vec![0..2, 6..14]);
    }

    #[test]
    fn merges_ranges_at_edges() {
        let mut ranges = ClusterRanges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(9..11);
        assert_eq!(ranges.ranges, vec![0..2, 6..14]);
    }
}
