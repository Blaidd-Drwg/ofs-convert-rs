use crate::fat::{BootSector, FatFile, FatDentry};
use crate::ext4::SuperBlock;
use crate::partition::Partition;
use crate::fat::Extent;
use std::os::raw::c_int;
use std::convert::{TryInto, TryFrom};

extern "C" {
    #[link_name = "\u{1}_Z10initialize9Partition16ext4_super_block11boot_sector"]
    pub fn initialize(
        partition: CPartition,
        _sb: SuperBlock,
        _boot_sector: BootSector,
    ) -> StreamArchiver;

    #[link_name = "\u{1}_Z7convert9PartitionP14StreamArchiver"]
    pub fn convert(partition: CPartition, read_stream: *mut StreamArchiver);

    #[link_name = "\u{1}_Z16add_regular_fileP14StreamArchiver10fat_dentryPKPKtmPK10fat_extentm"]
    pub fn add_regular_file(
        write_stream: *mut StreamArchiver,
        dentry: FatDentry,
        lfn_entries: *const *const u16,
        lfn_entry_count: usize,
        extents: *const CExtent,
        extent_count: usize,
    );

    #[link_name = "\u{1}_Z7add_dirP14StreamArchiver10fat_dentryPKPKtmPK10fat_extentm"]
    pub fn add_dir(
        write_stream: *mut StreamArchiver,
        dentry: FatDentry,
        lfn_entries: *const *const u16,
        lfn_entry_count: usize,
        extents: *const CExtent,
        extent_count: usize,
    ) -> *mut u32;
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CPartition {
    pub size: usize,
    pub ptr: *mut u8,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CExtent {
    pub logical_start: u32,  // First file cluster number that this extent covers
    pub length: u16,  // Number of clusters covered by extent
    pub physical_start: u32,  // Physical cluster number to which this extent points
}

/// We only temporarily store this as the result of a C function to then pass it to another C function.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct StreamArchiver {
    page_placeholder: *mut u8,
    offset_in_page: u64,
    element_index: u64,
    header_placeholder: *mut u8,
}

pub unsafe fn c_initialize(partition: &mut Partition, superblock: SuperBlock, boot_sector: BootSector) -> StreamArchiver {
    initialize(CPartition{size: partition.size(), ptr: partition.as_mut_ptr()}, superblock, boot_sector)
}

pub unsafe fn c_convert(partition: &mut Partition, stream_archiver: *mut StreamArchiver) {
    convert(CPartition{size: partition.size(), ptr: partition.as_mut_ptr()}, stream_archiver);
}

pub fn c_serialize_file(file: &FatFile, stream_archiver: *mut StreamArchiver) {
    let c_extents = to_c_extents(&file.data_ranges);
    unsafe {
        add_regular_file(
            stream_archiver,
            file.dentry,
            file.lfn_entries.iter().map(|entry| entry.as_ptr()).collect::<Vec<_>>().as_ptr(),
            file.lfn_entries.len(),
            c_extents.as_ptr(),
            c_extents.len()
        );
    }
}

pub fn c_serialize_directory(file: &FatFile, stream_archiver: *mut StreamArchiver) -> *mut u32 {
    let c_extents = to_c_extents(&file.data_ranges);
    unsafe {
        add_dir(
            stream_archiver,
            file.dentry,
            file.lfn_entries.iter().map(|entry| entry.as_ptr()).collect::<Vec<_>>().as_ptr(),
            file.lfn_entries.len(),
            c_extents.as_ptr(),
            c_extents.len()
        )
    }
}

// TODO type conversions
fn to_c_extents(data_ranges: &[Extent]) -> Vec<CExtent> {
    let mut extent_start = 0;
    data_ranges.iter()
        .map(|range| {
            let len = range.end.get() - range.start.get();
            let c_extent = CExtent {
                logical_start: extent_start,
                length: len.try_into().unwrap(),
                physical_start: range.start.get(),
            };
            extent_start += u32::try_from(len).unwrap();
            c_extent
        })
        .collect()
}
