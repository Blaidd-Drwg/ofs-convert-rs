use num::Integer;

const EXT4_NAME_MAX_LEN: usize = 255;

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
    pub fn new(inode_no: u32, name: String) -> Self {
        let dentry_len = next_multiple_of_four(name.len() + 8) as u16;
        let inner = Ext4DentrySized { inode_no, dentry_len, name_len: name.len() as u16 };
        Self { inner, name }
    }

    pub fn serialize_name(&self) -> Vec<u8> {
        let mut bytes = self.name.as_bytes().to_vec();
        bytes.push(0);
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

fn next_multiple_of_four(n: usize) -> usize {
    n.div_ceil(&4) * 4
}
