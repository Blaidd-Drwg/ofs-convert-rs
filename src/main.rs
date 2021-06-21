#![allow(dead_code)]

mod allocator;
mod bitmap;
mod ext4;
mod fat;
mod lohi;
mod partition;
mod ranges;
mod serialization;
mod util;

use std::env::args;
use std::io;
use std::mem::size_of;

use static_assertions::const_assert;

use crate::ext4::SuperBlock;
use crate::partition::Partition;
use crate::serialization::FatTreeSerializer;

// u32 must fit into usize
const_assert!(size_of::<usize>() >= size_of::<u32>());

fn main() {
    if args().len() != 2 {
        print_help();
        std::process::exit(1);
    }

    match args().nth(1).unwrap().as_str() {
        "-h" | "--help" => print_help(),
        partition_path => {
            let result = unsafe { ofs_convert(partition_path) };
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

/// SAFETY: `partition_path` must be a path to a valid FAT partition. TODO update when all the C parts are ported
unsafe fn ofs_convert(partition_path: &str) -> io::Result<()> {
    let mut partition = Partition::open(partition_path)?;
    let (fat_partition, mut allocator) =
        fat::FatPartition::new_with_allocator(partition.as_mut_ptr(), partition.len(), partition.lifetime);
    let boot_sector = *fat_partition.boot_sector();
    let superblock = SuperBlock::from(&boot_sector)?;
    let forbidden_ranges = superblock.block_group_overhead_ranges();
    for range in &forbidden_ranges {
        allocator.forbid(range.clone());
    }

    let mut serializer = FatTreeSerializer::new(allocator, fat_partition.cluster_size() as usize, forbidden_ranges);
    serializer.serialize_directory_tree(&fat_partition);

    let mut deserializer = serializer.into_deserializer();
    let mut ext4_partition = fat_partition.into_ext4();
    deserializer.deserialize_directory_tree(&mut ext4_partition);

    Ok(())
}
