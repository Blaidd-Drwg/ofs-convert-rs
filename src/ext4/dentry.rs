use std::convert::TryFrom;
use std::mem::size_of;

use anyhow::{bail, Result};

use crate::ext4::InodeNo;

const EXT4_NAME_MAX_LEN: usize = 255;
const ALIGNMENT: usize = 4;

pub struct Ext4Dentry {
    pub inner: Ext4DentrySized,
    pub name: String,
}

#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct Ext4DentrySized {
    inode_no: InodeNo,
    /// Always a multiple of 4 to ensure alignment
    dentry_len: u16,
    name_len: u16,
}

impl Ext4Dentry {
    pub const MAX_LEN: usize = aligned_length(EXT4_NAME_MAX_LEN + size_of::<Ext4DentrySized>(), ALIGNMENT);

    pub fn new(inode_no: InodeNo, name: String) -> Result<Self> {
        // FAT32 allows names up to 255 UCS-2 characters, which may be longer than 255 bytes
        if name.len() > EXT4_NAME_MAX_LEN {
            bail!("Length of file name '{}' exceeds 255 bytes", name);
        }
        let dentry_len = u16::try_from(aligned_length(name.len() + size_of::<Ext4DentrySized>(), ALIGNMENT)).unwrap();
        let inner = Ext4DentrySized {
            inode_no,
            dentry_len,
            name_len: u16::try_from(name.len()).unwrap(),
        };
        Ok(Self { inner, name })
    }

    pub fn serialize_name(&self) -> Vec<u8> {
        let mut bytes = self.name.as_bytes().to_vec();
        let new_len = aligned_length(bytes.len(), ALIGNMENT);
        bytes.resize(new_len, 0);
        bytes
    }

    pub fn dentry_len(&self) -> u16 {
        self.inner.dentry_len
    }
}

impl Ext4DentrySized {
    /// PANICS: Panics if `dentry
    pub fn increment_dentry_len(&mut self, num: u16) {
        assert!(usize::from(num) % ALIGNMENT == 0);
        self.dentry_len += num;
    }
}

const fn aligned_length(n: usize, alignment: usize) -> usize {
    n.next_multiple_of(alignment)
}
