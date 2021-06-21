use num::Integer;

pub struct Bitmap<'a> {
    pub data: &'a mut [u8],
}

impl<'a> Bitmap<'a> {
    pub fn set(&mut self, idx: usize) {
        let (data_idx, bit_idx) = idx.div_rem(&8);
        self.data[data_idx] |= 1 << bit_idx;
    }

    pub fn clear(&mut self, idx: usize) {
        let (data_idx, bit_idx) = idx.div_rem(&8);
        self.data[data_idx] &= !(1 << bit_idx);
    }

    pub fn get(&self, idx: usize) -> bool {
        let (data_idx, bit_idx) = idx.div_rem(&8);
        self.data[data_idx] & (1 << bit_idx) != 0
    }

    pub fn len(&self) -> usize {
        self.data.len() * 8
    }
}
