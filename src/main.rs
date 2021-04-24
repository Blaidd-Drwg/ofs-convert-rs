mod partition;
mod fat;
mod ext4;
mod lohi;

use partition::Partition;

use std::env::args;
use std::os::raw::c_int;
use std::io;


extern "C" {
    #[link_name = "\u{1}_Z6c_main9Partition"]
    pub fn c_main(partition: CPartition) -> c_int;
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CPartition {
    pub size: usize,
    pub ptr: *mut u8,
}

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
    let fat_partition = fat::FatPartition::new(partition.as_mut_ptr());
    let superblock = ext4::SuperBlock::new(fat_partition.boot_sector());
    // determine ext4 structure (via boot sector)
    // traverse, save metadata, move conflicting data
    // write block group headers (breaks FAT)
    // convert file metadata (makes ext4)
    Ok(())
}
