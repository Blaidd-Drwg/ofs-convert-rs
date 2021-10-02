use anyhow::Result;

use crate::fat::FatDentry;
type Timestamp = u32;

/// A slimmed down representation of the relevant components of a FAT dentry for serialization
/// This excludes the file name and the file's data ranges: since they have variable length,
/// they are treated separately.
#[derive(Clone, Copy)]
pub struct DentryRepresentation {
    pub access_time: Timestamp,
    pub create_time: Timestamp,
    pub mod_time: Timestamp,
    pub file_size: u32,
    pub is_dir: bool,
}

impl DentryRepresentation {
    pub fn from(dentry: FatDentry) -> Result<Self> {
        Ok(Self {
            access_time: dentry.access_time_as_unix()?,
            create_time: dentry.create_time_as_unix()?,
            mod_time: dentry.modify_time_as_unix()?,
            file_size: dentry.file_size,
            is_dir: dentry.is_dir(),
        })
    }
}
