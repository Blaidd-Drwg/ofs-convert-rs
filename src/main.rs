#![feature(step_trait)]
#![feature(iter_advance_by)]

mod allocator;
mod bitmap;
mod ext4;
mod fat;
mod lohi;
mod partition;
mod ranges;
mod serialization;
mod util;

use std::io::{self, Write};
use std::mem::size_of;
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{App, Arg};
use static_assertions::const_assert;
use text_io::try_read;

use crate::ext4::SuperBlock;
use crate::fat::FatFs;
use crate::partition::Partition;
use crate::serialization::FatTreeSerializer;

const_assert!(size_of::<usize>() >= size_of::<u32>());

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
    // SAFETY: TODO
    let (fat_fs, mut allocator) =
        FatFs::new_with_allocator(partition.as_mut_ptr(), partition.len(), partition.lifetime)?;
    let boot_sector = fat_fs.boot_sector();
    let forbidden_ranges = SuperBlock::from(boot_sector)?.block_group_overhead_ranges();
    for range in &forbidden_ranges {
        allocator.forbid(range.clone());
    }

    let mut serializer = FatTreeSerializer::new(allocator, fat_fs, forbidden_ranges);
    serializer.serialize_directory_tree().context("Serialization failed")?;
    // TODO differentiate FAT consistent/inconsistent errors

    let mut deserializer = serializer.into_deserializer().context("A dry run of the conversion failed")?;

    // This step makes the FAT filesystem inconsistent
    deserializer.deserialize_directory_tree().context("Conversion failed")?;

    Ok(())
}
