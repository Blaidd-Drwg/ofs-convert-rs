use std::iter::IntoIterator;
use std::ops::Range;


/// A set of non-overlapping ranges
#[derive(Clone, Debug, Default)]
pub struct Ranges<Idx: Ord + Copy> {
    /// invariant: non-overlapping, sorted
    ranges: Vec<Range<Idx>>,
}

#[derive(PartialEq, Debug)]
pub enum NotCoveredRange<T> {
    /// a range with bounded start and end
    Bounded(Range<T>),
    /// a range with a bounded start and unbounded end
    Unbounded(T),
}

impl<Idx: Ord + Copy> Ranges<Idx> {
    pub fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    pub fn from(ranges: impl IntoIterator<Item = Range<Idx>>) -> Self {
        let mut instance = Self::new();
        for range in ranges {
            instance.insert(range);
        }
        instance
    }

    /// Inserts `range` into `self.ranges` in the correct position and merging it with other ranges
    /// in case they overlap.
    pub fn insert(&mut self, range: Range<Idx>) {
        let first_merge_candidate_index = self.first_merge_candidate(&range);

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

    /// Returns the first range of non-covered items starting at or after `x`, whose end can either
    /// be bounded or unbounded.
    pub fn next_not_covered(&self, x: Idx) -> NotCoveredRange<Idx> {
        let overlap_candidate_idx = self.first_overlap_candidate(&(x..x));
        if overlap_candidate_idx == self.ranges.len() {
            return NotCoveredRange::Unbounded(x);
        }

        let overlap_candidate = self.ranges[overlap_candidate_idx].clone();
        if overlap_candidate.start > x {
            NotCoveredRange::Bounded(x..overlap_candidate.start)
        } else {
            self.next_not_covered(overlap_candidate.end)
        }
    }

    /// Splits up a range into a Vec of subranges and a bool. Either every element in a subrange is contained in a range
    /// from `self.ranges` and the bool is true, or no element in a subrange is contained in such a range and the bool
    /// is false.
    pub fn split_overlapping(&self, range: Range<Idx>) -> Vec<(Range<Idx>, bool)> {
        let mut remaining_range = range;
        let mut overlap_candidate_idx = self.first_overlap_candidate(&remaining_range);
        let mut result = Vec::new();

        while !remaining_range.is_empty() && overlap_candidate_idx < self.ranges.len() {
            let overlap_candidate = self.ranges[overlap_candidate_idx].clone();
            if overlap_candidate.start > remaining_range.start {
                // the first subrange of `remaining_range` is non-overlapping
                let non_overlap_range = remaining_range.start..(overlap_candidate.start.min(remaining_range.end));
                remaining_range.start = non_overlap_range.end;
                result.push((non_overlap_range, false));
                // we have not handled any overlapping subranges yet, the overlap candidate doesn't change
            } else {
                // the first subrange of `remaining_range` is overlapping
                let overlap_range = remaining_range.start..(overlap_candidate.end.min(remaining_range.end));
                remaining_range.start = overlap_range.end;
                result.push((overlap_range, true));
                // we have handled the overlapping subrange, get next overlap_candidate
                overlap_candidate_idx += 1;
            }
        }

        // there are no overlap candidates left, `remaining_range` is non-overlapping
        if !remaining_range.is_empty() {
            result.push((remaining_range, false));
        }
        result
    }

    /// Returns the index in `self.ranges` of the first range that ends at or after `range.start`. If there is none,
    /// returns `self.ranges.len()`.
    fn first_merge_candidate(&self, range: &Range<Idx>) -> usize {
        match self.ranges.binary_search_by_key(&range.start, |candidate| candidate.end) {
            Ok(result) | Err(result) => result,
        }
    }

    /// Returns the index in `self.ranges` of the first range that ends after `range.start`. If there is none, returns
    /// `self.ranges.len()`.
    fn first_overlap_candidate(&self, range: &Range<Idx>) -> usize {
        match self.ranges.binary_search_by_key(&range.start, |candidate| candidate.end) {
            Ok(result) => result + 1, // `self.ranges[result]` ends 1 before `range` start, so we want the next range
            Err(result) => result,
        }
    }
}

impl<'a, Idx: Ord + Copy> IntoIterator for &'a Ranges<Idx> {
    type Item = &'a Range<Idx>;
    type IntoIter = std::slice::Iter<'a, Range<Idx>>;
    fn into_iter(self) -> Self::IntoIter {
        (&self.ranges).iter()
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

    #[test]
    fn not_covered_start() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        assert_eq!(ranges.next_not_covered(2), NotCoveredRange::Bounded(2..6));
    }

    #[test]
    fn not_covered_middle() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        assert_eq!(ranges.next_not_covered(4), NotCoveredRange::Bounded(4..6));
    }

    #[test]
    fn not_covered_covered() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        assert_eq!(ranges.next_not_covered(0), NotCoveredRange::Bounded(2..6));
    }

    #[test]
    fn not_covered_unbounded() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        assert_eq!(ranges.next_not_covered(12), NotCoveredRange::Unbounded(14));
    }

    #[test]
    fn not_covered_when_empty() {
        let mut ranges = Ranges { ranges: Vec::new() };
        assert_eq!(ranges.next_not_covered(5), NotCoveredRange::Unbounded(5));
    }

    #[test]
    fn split_overlapping_short() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        assert_eq!(ranges.split_overlapping(5..7), vec![(5..6, false), (6..7, true)]);
    }

    #[test]
    fn split_overlapping_long() {
        let mut ranges = Ranges { ranges: vec![0..2, 6..9, 11..14] };
        assert_eq!(
            ranges.split_overlapping(0..19),
            vec![(0..2, true), (2..6, false), (6..9, true), (9..11, false), (11..14, true), (14..19, false)]
        );
    }
}
