#![allow(dead_code)]
#![feature(step_trait)]

mod stream_archiver;
mod allocator;
mod ranges;
mod partition;
mod fat;
mod ext4;
mod lohi;
mod util;
mod c_wrapper;
// mod allocator;

use crate::partition::Partition;
use crate::fat::FsTreeSerializer;
use crate::ext4::SuperBlock;
use crate::ranges::Ranges;
use crate::c_wrapper::{c_initialize, c_start_writing, c_end_writing};

use std::env::args;
use std::io;
use static_assertions::const_assert;

// u32 must fit into usize
const_assert!(std::mem::size_of::<usize>() >= std::mem::size_of::<u32>());

fn main() {
    if args().len() != 2 {
        print_help();
        std::process::exit(1);
    }

    match args().nth(1).unwrap().as_str() {
        "-h" | "--help" => print_help(),
        partition_path => {
            let result = ofs_convert(partition_path);
            if let Err(reason) = result {
                eprintln!("Error: {}", reason);
                std::process::exit(1);
            }
        }
    }
}

fn print_help() {
    println!("Usage: ofs-convert-rs path/to/fat-partition");
}

fn ofs_convert(partition_path: &str) -> io::Result<()> {
    let mut partition = Partition::open(partition_path)?;
    unsafe {
        let (fat_partition, mut allocator) = fat::FatPartition::new_with_allocator(partition.as_mut_slice());
        let boot_sector = *fat_partition.boot_sector();
        let superblock = SuperBlock::from(&boot_sector)?;
        let forbidden_ranges = Ranges::from(superblock.block_group_overhead_ranges());
        for range in &forbidden_ranges {
            allocator.forbid(range.clone());
        }

        let mut serializer = FsTreeSerializer::new(allocator, fat_partition.cluster_size() as usize, forbidden_ranges);
        serializer.serialize_directory_tree(&fat_partition);

        let mut deserializer = serializer.into_deserializer();
        let ext4_partition = fat_partition.into_ext4();
        c_initialize(ext4_partition.as_ptr() as *mut u8, superblock, boot_sector);
        let mut dentry_write_position = c_start_writing(&mut || u32::from(deserializer.allocator.allocate_one()));
        deserializer.deserialize_directory_tree(&mut dentry_write_position);
        c_end_writing(dentry_write_position, &mut || u32::from(deserializer.allocator.allocate_one()));
    }
    // TODO write block group headers (breaks FAT)
    // TODO convert file metadata (makes ext4)
    Ok(())
}
