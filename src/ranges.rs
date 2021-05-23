use std::ops::Range;


// a set of non-overlapping ranges
pub struct Ranges<Idx: Ord + Copy> {
    ranges: Vec<Range<Idx>>, // invariant: non-overlapping, sorted
}

impl<Idx: Ord + Copy> Ranges<Idx> {
    pub fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    /// Inserts `range` into `self.ranges` in the correct position and merging it with other ranges
    /// in case they overlap.
    pub fn insert(&mut self, range: Range<Idx>) {
        let first_merge_candidate_index = self.first_overlap_candidate(&range);

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

    /// Splits up a range into a Vec of subranges and a bool. Either every element in a subrange is contained in a range from `self.ranges` and the bool is true, or no element in a subrange is contained in such a range and the bool is false.
    pub fn split_overlapping(&self, range: Range<Idx>) -> Vec<(Range<Idx>, bool)> {
        unimplemented!()
    }

    /// Returns the index of the first range that could overlap `range` (that is, the first
    /// range that ends after `range` starts).
    fn first_overlap_candidate(&self, range: &Range<Idx>) -> usize {
        match self.ranges.binary_search_by_key(&range.start, |candidate| candidate.end,) {
            Ok(result) | Err(result) => result
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_range() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(3..5);
        assert_eq!(ranges.ranges, vec![0..2, 3..5, 6..9, 11..14]);
    }

    #[test]
    fn pushes_range() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(15..16);
        assert_eq!(ranges.ranges, vec![0..2, 6..9, 11..14, 15..16]);
    }

    #[test]
    fn merges_subrange() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(5..10);
        assert_eq!(ranges.ranges, vec![0..2, 5..10, 11..14]);
    }

    #[test]
    fn merges_superrange() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(7..9);
        assert_eq!(ranges.ranges, vec![0..2, 6..9, 11..14]);
    }

    #[test]
    fn merges_multiple_subranges() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(5..15);
        assert_eq!(ranges.ranges, vec![0..2, 5..15]);
    }

    #[test]
    fn merges_multiple_ranges() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(8..12);
        assert_eq!(ranges.ranges, vec![0..2, 6..14]);
    }

    #[test]
    fn merges_ranges_at_edges() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        ranges.insert(9..11);
        assert_eq!(ranges.ranges, vec![0..2, 6..14]);
    }
}
