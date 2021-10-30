use num::Integer;

pub struct Bitmap<'a> {
    data: &'a mut [u8],
}

impl<'a> Bitmap<'a> {
    /// PANICS: Panics is `self.len()` would overflow usize
    pub fn new(data: &'a mut [u8]) -> Self {
        assert!(data.len().checked_mul(8).is_some());
        Self { data }
    }

    /// PANICS: Panics if `idx` out of bounds
    pub fn get(&self, idx: usize) -> bool {
        let (data_idx, bit_idx) = idx.div_rem(&8);
        self.data[data_idx] & (1 << bit_idx) != 0
    }

    /// PANICS: Panics if `idx` out of bounds
    pub fn set(&mut self, idx: usize) {
        let (data_idx, bit_idx) = idx.div_rem(&8);
        self.data[data_idx] |= 1 << bit_idx;
    }

    /// PANICS: Panics if `idx` out of bounds
    #[allow(dead_code)]
    pub fn clear(&mut self, idx: usize) {
        let (data_idx, bit_idx) = idx.div_rem(&8);
        self.data[data_idx] &= !(1 << bit_idx);
    }

    pub fn clear_all(&mut self) {
        self.data.fill(0);
    }

    pub fn len(&self) -> usize {
        self.data.len() * 8
    }
}
