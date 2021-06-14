#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Extent {
    pub ee_block: u32,
    pub ee_len: u16,
    pub ee_start_hi: u16,
    pub ee_start_lo: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ExtentHeader {
    pub eh_magic: u16,
    pub eh_entries: u16,
    pub eh_max: u16,
    pub eh_depth: u16,
    pub eh_generation: u32,
}
