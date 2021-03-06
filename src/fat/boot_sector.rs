use std::ops::Range;

use anyhow::{bail, Result};

use crate::fat::{ClusterIdx, FatDentry};
use crate::util::FromU32;

const FS_TYPE_FAT32: [u8; 8] = *b"FAT32   ";
const EXT_BOOT_SIGNATURE_FAT32: u8 = 0x29;

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct BootSector {
    pub jump_instruction: [u8; 3],
    pub oem_name: [u8; 8],
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub sectors_before_fat: u16,
    pub fat_count: u8,
    pub dir_entries: u16,
    pub sector_count_1: u16,
    pub media_descriptor: u8,
    pub unused2: u16,
    pub sectors_per_disk_track: u16,
    pub disk_heads: u16,
    pub hidden_sectors_before_partition: u32,
    pub sector_count_2: u32,
    pub sectors_per_fat: u32,
    pub drive_description_flags: u16,
    pub version: u16,
    pub root_cluster_no: u32,
    pub fs_info_sector_no: u16,
    pub backup_boot_sector_no: u16,
    pub reserved: [u8; 12],
    pub physical_drive_no: u8,
    pub reserved2: u8,
    pub ext_boot_signature: u8,
    pub volume_id: u32,
    pub volume_label: [u8; 11],
    pub fs_type: [u8; 8],
}

impl BootSector {
    /// Performs a sanity check to see if this is indeed a FAT32 boot sector. A return value of `true` does not
    /// guarantee that `self` is consistent with the partition it belongs to, only that this data was meant to be a boot
    /// sector.
    pub fn validate(&self) -> Result<&Self> {
        if self.ext_boot_signature != EXT_BOOT_SIGNATURE_FAT32 {
            bail!(
                "Unexpected extended boot signature: {} instead of {}",
                self.ext_boot_signature,
                EXT_BOOT_SIGNATURE_FAT32
            );
        }
        if self.fs_type != FS_TYPE_FAT32 {
            bail!(
                "Unexpected file system type: {} instead of {}",
                std::str::from_utf8(&self.fs_type).unwrap_or("(non-printable)"),
                std::str::from_utf8(&FS_TYPE_FAT32).unwrap_or("(non-printable)")
            );
        }
        Ok(self)
    }

    /// Returns the range in bytes of the first FAT table, relative to the filesystem start
    pub fn get_fat_table_range(&self) -> Range<usize> {
        let fat_table_start_byte = usize::from(self.sectors_before_fat) * usize::from(self.bytes_per_sector);
        let fat_table_len = usize::fromx(self.sectors_per_fat) * usize::from(self.bytes_per_sector);
        fat_table_start_byte..fat_table_start_byte + fat_table_len
    }

    /// Returns the range in bytes of the data region, relative to the filesystem start
    pub fn get_data_range(&self) -> Range<usize> {
        let first_data_byte = usize::fromx(self.first_data_sector()) * usize::from(self.bytes_per_sector);
        first_data_byte..self.fs_size()
    }

    fn first_data_sector(&self) -> u32 {
        u32::from(self.sectors_before_fat) + (self.sectors_per_fat * u32::from(self.fat_count))
    }

    pub fn first_data_cluster(&self) -> ClusterIdx {
        self.first_data_sector() / ClusterIdx::from(self.sectors_per_cluster)
    }

    pub fn sector_count(&self) -> u32 {
        if self.sector_count_1 == 0 {
            self.sector_count_2
        } else {
            u32::from(self.sector_count_1)
        }
    }

    pub fn cluster_count(&self) -> u32 {
        self.sector_count() / u32::from(self.sectors_per_cluster)
    }

    /// in bytes
    pub fn fs_size(&self) -> usize {
        usize::from(self.bytes_per_sector) * usize::fromx(self.sector_count())
    }

    /// in bytes
    pub fn cluster_size(&self) -> u32 {
        u32::from(self.sectors_per_cluster) * u32::from(self.bytes_per_sector)
    }

    pub fn dentries_per_cluster(&self) -> usize {
        usize::fromx(self.cluster_size()) / std::mem::size_of::<FatDentry>()
    }

    pub fn volume_label(&self) -> &[u8] {
        if self.ext_boot_signature == 0x28 {
            &[]
        } else {
            let last_character_idx = self
                .volume_label
                .iter()
                .enumerate()
                .rev()
                .filter(|(_idx, &character)| character != b' ')
                .map(|(idx, _character)| idx)
                .next()
                .unwrap_or(0);

            &self.volume_label[0..last_character_idx]
        }
    }
}
