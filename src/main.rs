#![allow(dead_code)]

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
use crate::stream_archiver::StreamArchiver;
use crate::fat::FsTreeSerializer;
use crate::allocator::Allocator;

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

// TODO handle root
// TODO create allocator, forbid bghs
fn ofs_convert(partition_path: &str) -> io::Result<()> {
    let mut partition = Partition::open(partition_path)?;
    unsafe {
        let fat_partition = fat::FatPartition::new(partition.as_mut_slice());
        let boot_sector = *fat_partition.boot_sector();
        let superblock = ext4::SuperBlock::new(&boot_sector)?;
        let fat_partition = fat::FatPartition::new(partition.as_slice());
        let mut allocator = Allocator::new(&partition, &fat_partition);
        let stream_archiver = StreamArchiver::new(&mut allocator, superblock.block_size() as usize);
        let mut serializer = FsTreeSerializer::new(stream_archiver);
        serializer.serialize_directory_tree(&fat_partition);
        // c_convert(&mut partition, &mut read_stream);
    }
    // traverse, save metadata, move conflicting data
    // write block group headers (breaks FAT)
    // convert file metadata (makes ext4)
    Ok(())
}
