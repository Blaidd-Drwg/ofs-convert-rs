#![feature(step_trait)]
#![feature(iter_advance_by)]
#![feature(int_roundings)]
#![feature(maybe_uninit_extra)]
#![feature(maybe_uninit_slice)]
#![feature(maybe_uninit_write_slice)]

mod allocator;
mod bitmap;
mod ext4;
mod fat;
mod lohi;
mod partition;
mod ranges;
mod serialization;
mod util;

use std::convert::TryFrom;
use std::io::{self, Write};
use std::mem::size_of;
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{App, Arg};
use static_assertions::const_assert;
use text_io::try_read;

use crate::ext4::{BlockIdx, SuperBlock};
use crate::fat::{ClusterIdx, FatFs};
use crate::partition::Partition;
use crate::ranges::Ranges;
use crate::serialization::FatTreeSerializer;

const_assert!(size_of::<usize>() >= size_of::<u32>());
const_assert!(size_of::<usize>() <= size_of::<u64>());

// TODO how does Ubuntu FAT driver handle timezones?
// TODO what can overflow?
// TODO convention: expect messages
// TODO allow manually increasing number of inodes
// TODO sometimes using Result where Option would be more idiomatic
// TODO add context to Errs
fn main() -> Result<()> {
    let matches =
        App::new("ofs-convert")
            .arg(
                Arg::with_name("PARTITION_PATH")
                    .required(true)
                    .help("The partition containing the FAT32 filesystem that should be converted"),
            )
            .arg(Arg::with_name("force").long("force").short("f").help(
                "Skip fsck (can lead to unexpected errors and data loss if the input filesystem is inconsistent)",
            ))
            .get_matches();

    let partition_path = matches.value_of("PARTITION_PATH").unwrap();
    if !matches.is_present("force") {
        match fsck_fat(partition_path) {
            Ok(true) => (),
            Ok(false) => bail!(
                "fsck failed. Running ofs-convert on an inconsistent FAT32 partition can lead to unexpected errors \
                 and data loss. To force the conversion, run again with the '-f' flag."
            ),
            Err(e) => {
                eprintln!("Error: {}", e);
                eprintln!(
                    "Unable to run fsck. Running ofs-convert on an inconsistent FAT32 partition can lead to \
                     unexpected errors and data loss."
                );
                eprint!("Run anyway? [y/N] ");
                io::stderr().flush()?;
                let answer: String = try_read!("{}\n")?;
                if !is_yes(&answer) {
                    bail!("Aborted by user");
                }
            }
        }
    }

    // SAFETY: We've done our best to ensure `partition_path` contains a consistent FAT32 partition
    unsafe { ofs_convert(partition_path) }
}

/// Returns `Ok(true)` if the filesystem check is successful, `Ok(false)` if it fails, and `Err` if fsck fails to run
/// (e.g. if the command `fsck.fat` is not found).
fn fsck_fat(partition_path: &str) -> Result<bool> {
    Ok(Command::new("fsck.fat").arg("-n").arg(partition_path).status()?.success())
}

fn is_yes(s: &str) -> bool {
    ["y", "yes"].contains(&s.trim().to_lowercase().as_str())
}

/// SAFETY: `partition_path` must point to a partition containing a consistent FAT32 filesystem.
unsafe fn ofs_convert(partition_path: &str) -> Result<()> {
    let mut partition = Partition::open(partition_path)?;
    // SAFETY: Safe because `partition`'s memory is valid and contains a FAT32 filesystem.
    let (fat_fs, mut allocator) =
        FatFs::new_with_allocator(partition.as_mut_ptr(), partition.len(), partition.lifetime)?;
    let boot_sector = fat_fs.boot_sector();
    let superblock = SuperBlock::from(boot_sector)?;

    let forbidden_ranges = forbidden_ranges(&superblock, fat_fs.cluster_count());
    for range in &forbidden_ranges {
        allocator.forbid(range.clone());
    }

    let mut serializer = FatTreeSerializer::new(allocator, fat_fs, forbidden_ranges);
    serializer.serialize_directory_tree().context("Serialization failed")?;
    let mut deserializer = serializer.into_deserializer().context("A dry run of the conversion failed")?;

    deserializer
        .deserialize_directory_tree()
        .context("Conversion failed unexpectedly. The FAT partition may have been left in an inconsistent status.")?;
    Ok(())
}

/// Returns the ranges of `ClusterIdx`s in the partition described by `superblock` that may not be overwritten with ext4
/// data.
fn forbidden_ranges(superblock: &SuperBlock, cluster_count: u32) -> Ranges<ClusterIdx> {
    let forbidden_ranges = superblock.block_group_overhead_ranges();
    let mut forbidden_ranges = into_cluster_idx_ranges(forbidden_ranges);
    let last_ext_cluster_idx = ClusterIdx::try_from(superblock.block_count_with_padding())
        .expect("ext4 block count <= FAT32 cluster count, so the index fits into a ClusterIdx");
    let overhanging_block_range = last_ext_cluster_idx..cluster_count;
    forbidden_ranges.insert(overhanging_block_range);
    forbidden_ranges
}

fn into_cluster_idx_ranges(ranges: Ranges<BlockIdx>) -> Ranges<ClusterIdx> {
    ranges
        .into_iter()
        .map(|range| {
            ClusterIdx::try_from(range.start)
                .expect("ext4 blocks count <= FAT32 cluster count, so the indices fit into a ClusterIdx")
                ..ClusterIdx::try_from(range.end).unwrap()
        })
        .collect()
}
