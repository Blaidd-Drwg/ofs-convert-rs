use std::convert::{TryFrom, TryInto};
use std::ffi::c_void;

use crate::ext4::{Inode, SuperBlock, EXT4_LOST_FOUND_INODE, EXT4_ROOT_INODE};
use crate::fat::{BootSector, Extent, FatDentry};

mod ffi {
    use crate::ext4::{Inode, SuperBlock};
    use crate::fat::{BootSector, FatDentry};

    pub type AllocatorData = *mut ::std::os::raw::c_void;
    pub type AllocatorFunc = unsafe extern "C" fn(arg1: AllocatorData) -> u32;

    extern "C" {
        #[link_name = "\u{1}_Z10initializePh16ext4_super_block11boot_sector"]
        pub fn initialize(fs_start: *mut u8, _sb: SuperBlock, _boot_sector: BootSector);

        #[link_name = "\u{1}_Z13start_writingv"]
        pub fn start_writing();

        #[link_name = "\u{1}_Z11end_writingv"]
        pub fn end_writing();

        #[link_name = "\u{1}_Z18get_existing_inodej"]
        pub fn get_existing_inode(inode_no: u32) -> *mut Inode;

        #[link_name = "\u{1}_Z11build_inodePK10fat_dentry"]
        pub fn build_inode(dentry: *const FatDentry) -> u32;

        #[link_name = "\u{1}_Z16build_root_inodev"]
        pub fn build_root_inode();

        #[link_name = "\u{1}_Z22build_lost_found_inodev"]
        pub fn build_lost_found_inode();

        #[link_name = "\u{1}_Z15register_extentPK10fat_extentjb"]
        pub fn register_extent(ext: *const Extent, inode_no: u32, add_to_extent_tree: bool);
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
        pub length: u16,         // Number of clusters covered by extent
        pub physical_start: u32, // Physical cluster number to which this extent points
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

pub unsafe fn c_start_writing() {
    ffi::start_writing()
}

pub unsafe fn c_end_writing() {
    ffi::end_writing();
}

fn wrap_allocator_callback<F>(allocator_callback: &mut F) -> (ffi::AllocatorFunc, ffi::AllocatorData)
where F: FnMut() -> u32 {
    extern "C" fn callback_wrapper<F>(closure_ptr: *mut c_void) -> u32
    where F: FnMut() -> u32 {
        let closure = closure_ptr as *mut F;
        unsafe { (*closure)() }
    }
    let allocator_data = allocator_callback as *mut _ as ffi::AllocatorData;
    (callback_wrapper::<F>, allocator_data)
}

pub fn c_add_extent(inode_no: u32, block_no: u32, logical_block: u32, len: u16) {
    let extent = ffi::Extent {
        physical_start: block_no,
        logical_start: logical_block,
        length: len,
    };
    unsafe {
        ffi::register_extent(&extent, inode_no, true);
    }
}

pub fn c_build_root_inode() -> u32 {
    unsafe {
        ffi::build_root_inode();
    }
    EXT4_ROOT_INODE
}

pub fn c_build_lost_found_inode() -> u32 {
    unsafe {
        ffi::build_lost_found_inode();
    }
    EXT4_LOST_FOUND_INODE
}

pub fn c_build_inode(f_dentry: &FatDentry) -> u32 {
    unsafe { ffi::build_inode(f_dentry as *const FatDentry) }
}

pub fn c_get_inode(inode_no: u32) -> &'static mut Inode {
    unsafe {
        ffi::get_existing_inode(inode_no)
            .as_mut()
            .expect("C returned a NULL inode pointer")
    }
}

// TODO type conversions
fn to_c_extents(data_ranges: &[Extent]) -> Vec<ffi::Extent> {
    let mut extent_start = 0;
    data_ranges
        .iter()
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
