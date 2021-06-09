use crate::fat::{BootSector, FatDentry, ClusterIdx};
use crate::ext4::SuperBlock;
use crate::fat::Extent;
use std::convert::{TryInto, TryFrom};
use std::ops::Range;
use std::ffi::c_void;

mod ffi {
    use crate::fat::{BootSector, FatDentry};
    use crate::ext4::SuperBlock;
    use crate::c_wrapper::DentryWritePosition;

    pub type AllocatorData = *mut ::std::os::raw::c_void;
    pub type AllocatorFunc = unsafe extern "C" fn(arg1: AllocatorData) -> u32;

    extern "C" {
        #[link_name = "\u{1}_Z10initializePh16ext4_super_block11boot_sector"]
        pub fn initialize(fs_start: *mut u8, _sb: SuperBlock, _boot_sector: BootSector);

        #[link_name = "\u{1}_Z13start_writingPFjPvES_"]
        pub fn start_writing(
            allocate_block_callback: AllocatorFunc,
            allocator_data: AllocatorData,
        ) -> DentryWritePosition;

        #[link_name = "\u{1}_Z11end_writing19DentryWritePositionPFjPvES0_"]
        pub fn end_writing(dentry_write_position: DentryWritePosition, allocate_block_callback: AllocatorFunc, allocator_data: AllocatorData);

        #[link_name = "\u{1}_Z18build_regular_filePK10fat_dentryPKhmR19DentryWritePositionPFjPvES6_PK10fat_extentm"]
        pub fn build_regular_file(
            f_dentry: *const FatDentry,
            name: *const u8,
            name_len: usize,
            dentry_write_position: *mut DentryWritePosition,
            allocate_block_callback: AllocatorFunc,
            allocator_data: AllocatorData,
            extents: *const Extent,
            extent_count: usize,
        );

        #[link_name = "\u{1}_Z15build_directoryPK10fat_dentryPKhmR19DentryWritePositionPFjPvES6_"]
        pub fn build_directory(
            f_dentry: *const FatDentry,
            name: *const u8,
            name_len: usize,
            parent_dentry_write_position: *mut DentryWritePosition,
            allocate_block_callback: AllocatorFunc,
            allocator_data: AllocatorData,
        ) -> DentryWritePosition;

        #[link_name = "\u{1}_Z12finalize_dirR19DentryWritePosition"]
        pub fn finalize_dir(dentry_write_position: *mut DentryWritePosition);
    }

    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct Partition {
        pub size: usize,
        pub ptr: *const u8,
    }

    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct Extent {
        pub logical_start: u32,  // First file cluster number that this extent covers
        pub length: u16,  // Number of clusters covered by extent
        pub physical_start: u32,  // Physical cluster number to which this extent points
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DentryWritePosition {
    pub inode_no: u32,
    pub block_no: u32,
    pub position_in_block: u32,
    pub block_count: u32,
    pub previous_dentry: *mut u8,
}

pub unsafe fn c_initialize(partition_ptr: *mut u8, superblock: SuperBlock, boot_sector: BootSector) {
    ffi::initialize(partition_ptr, superblock, boot_sector);
}

pub unsafe fn c_start_writing<F>(allocate_block_callback: &mut F) -> DentryWritePosition where F: FnMut() -> u32 {
    let (allocator_callback, allocator_data) = wrap_allocator_callback(allocate_block_callback);
    ffi::start_writing(allocator_callback, allocator_data)
    // DentryWritePosition { inode_no: 2, block_no: 0, position_in_block: 0, block_count: 0, previous_dentry: std::ptr::null_mut() }
}

pub unsafe fn c_end_writing<F>(dentry_write_position: DentryWritePosition, allocate_block_callback: &mut F) where F: FnMut() -> u32 {
    let (allocator_callback, allocator_data) = wrap_allocator_callback(allocate_block_callback);
    ffi::end_writing(dentry_write_position, allocator_callback, allocator_data);
}

fn wrap_allocator_callback<F>(allocator_callback: &mut F) -> (ffi::AllocatorFunc, ffi::AllocatorData) where F: FnMut() -> u32 {
    extern "C" fn callback_wrapper<F>(closure_ptr: *mut c_void) -> u32 where F: FnMut() -> u32 {
        let closure = closure_ptr as *mut F;
        unsafe {
             (*closure)()
        }
    }
    let allocator_data = allocator_callback as *mut _ as ffi::AllocatorData;
    (callback_wrapper::<F>, allocator_data)
}

pub fn c_build_regular_file<F>(
    dentry: FatDentry,
    name: String,
    extents: Vec<Range<ClusterIdx>>,
    parent_dentry_write_position: &mut DentryWritePosition,
    allocate_block_callback: &mut F
) where F: FnMut() -> u32 {
    let c_extents = to_c_extents(&extents);
    let (allocator_callback, allocator_data) = wrap_allocator_callback(allocate_block_callback);
    unsafe {
        ffi::build_regular_file(
            &dentry as *const FatDentry,
            name.as_ptr(),
            name.len(),
            parent_dentry_write_position as *mut DentryWritePosition,
            allocator_callback,
            allocator_data,
            c_extents.as_ptr(),
            c_extents.len(),
        )
    }
}

pub fn c_build_directory<F>(
    dentry: FatDentry,
    name: String,
    parent_dentry_write_position: &mut DentryWritePosition,
    allocate_block_callback: &mut F
) -> DentryWritePosition where F: FnMut() -> u32 {
    let (allocator_callback, allocator_data) = wrap_allocator_callback(allocate_block_callback);
    unsafe {
        ffi::build_directory(
            &dentry as *const FatDentry,
            name.as_ptr(),
            name.len(),
            parent_dentry_write_position as *mut DentryWritePosition,
            allocator_callback,
            allocator_data,
        )
    }
}

pub fn c_finalize_directory(dentry_write_position: &mut DentryWritePosition) {
    unsafe {
        ffi::finalize_dir(dentry_write_position)
    }
}

// TODO type conversions
fn to_c_extents(data_ranges: &[Extent]) -> Vec<ffi::Extent> {
    let mut extent_start = 0;
    data_ranges.iter()
        .map(|range| {
            let c_extent = ffi::Extent {
                logical_start: extent_start,
                length: range.len().try_into().unwrap(),
                physical_start: range.start,
            };
            extent_start += u32::try_from(range.len()).unwrap();
            c_extent
        })
        .collect()
}
