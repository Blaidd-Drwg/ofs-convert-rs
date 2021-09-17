use anyhow::{bail, Result};
use num::Integer;

const EXT4_NAME_MAX_LEN: usize = 255;
const ALIGNMENT: usize = 4;

pub struct Ext4Dentry {
    pub inner: Ext4DentrySized,
    pub name: String,
}

#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct Ext4DentrySized {
    inode_no: u32,
    /// Always a multiple of 4 to ensure alignment
    dentry_len: u16,
    name_len: u16,
}

impl Ext4Dentry {
    pub fn new(inode_no: u32, name: String) -> Result<Self> {
        // FAT32 allows names up to 255 UCS-2 characters, which may be longer than 255 bytes
        if name.len() > EXT4_NAME_MAX_LEN {
            bail!("Length of file name '{}' exceeds 255 bytes", name);
        }
        let dentry_len = aligned_length(name.len() + 8, ALIGNMENT) as u16;
        let inner = Ext4DentrySized { inode_no, dentry_len, name_len: name.len() as u16 };
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
    pub fn increment_dentry_len(&mut self, dentry_len: u16) {
        assert!(dentry_len % 4 == 0);
        self.dentry_len += dentry_len;
    }
}

fn aligned_length(n: usize, alignment: usize) -> usize {
    n.div_ceil(&alignment) * alignment
}
