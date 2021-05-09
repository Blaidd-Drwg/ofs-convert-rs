use crate::lohi::LoHi32;
use ucs2;

#[repr(C)]
pub union FatPseudoDentry {
    dentry: FatDentry,
    long_file_name: LongFileName,
}

impl FatPseudoDentry {
    pub fn as_dentry(&self) -> Option<&FatDentry> {
        if self.is_long_file_name() {
            None
        } else {
            unsafe {
                Some(&self.dentry)
            }
        }
    }

    pub fn as_long_file_name(&self) -> Option<&LongFileName> {
        if self.is_long_file_name() {
            unsafe {
                Some(&self.long_file_name)
            }
        } else {
            None
        }
    }

    pub fn is_long_file_name(&self) -> bool {
        // SAFETY: attrs field is in the same position for both FatDentry and LongFileName,
        // so this is safe
        unsafe {
            self.long_file_name.attrs & 0x0F != 0
        }
    }

    pub fn is_invalid(&self) -> bool {
        // SAFETY: the first byte is used to mark invalid entries both in FatDentry and
        // LongFileName, so this is safe
        unsafe {
            self.long_file_name.sequence_no == 0xE5
        }
    }

    /// True iff self is invalid and the directory contains no more valid dentries
    pub fn is_dir_table_end(&self) -> bool {
        unsafe {
            self.dentry.short_name[0] == 0x00
        }
    }

    pub fn should_be_ignored(&self) -> bool {
        let mut should_be_ignored = self.is_invalid();
        if let Some(dentry) = self.as_dentry() {
            should_be_ignored |= dentry.is_dot_dir();
        }
        should_be_ignored
    }
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct FatDentry {
    pub short_name: [u8; 8],
    pub short_extension: [u8; 3],
    pub attrs: u8,
    pub short_name_case: u8,
    pub create_time_10_ms: u8,
    pub create_time: u16,
    pub create_date: u16,
    pub access_date: u16,
    pub first_cluster_high: u16,
    pub mod_time: u16,
    pub mod_date: u16,
    pub first_cluster_low: u16,
    pub file_size: u32,
}

impl FatDentry {
    // TODO refactor to not need mut
    // TODO refactor to need an unsafe function to create
    pub fn first_cluster_idx(&mut self) -> u32 {
        unsafe { // Ok since both fields are 2-byte-aligned
            LoHi32 { lo: &mut self.first_cluster_low, hi: &mut self.first_cluster_high }.get()
        }
    }

    /// True iff self is invalid but the directory might more valid dentries
    pub fn is_invalid(&self) -> bool {
        self.short_name[0] != 0xE5
    }

    /// True iff the dentry represents either the current directory `.` or the parent directory `..`
    pub fn is_dot_dir(&self) -> bool {
        self.short_name[0] == b'.'
    }

    /// True iff TODO
    pub fn has_file_extension(&self) -> bool {
        self.short_extension[0] != b' '
    }

    pub fn has_lowercase_name(&self) -> bool {
        self.short_name_case & 0x8 != 0
    }

    pub fn has_lowercase_extension(&self) -> bool {
        self.short_name_case & 0x10 != 0
    }

    // TODO what encoding do FAT short names use?
    // Assume ascii for now
    pub fn read_short_file_name(&self) -> String {
        let name_ascii_bytes: Vec<_> = self.short_name.iter().copied().collect();
        let mut name_string = String::from_utf8(name_ascii_bytes).unwrap();
        if self.has_lowercase_name() {
            name_string.make_ascii_lowercase();
        }

        if self.has_file_extension() {
            let extension_ascii_bytes: Vec<_> = self.short_extension.iter().copied().collect();
            let mut extension_string = String::from_utf8(extension_ascii_bytes).unwrap();
            if self.has_lowercase_extension() {
                extension_string.make_ascii_lowercase();
            }
            name_string = format!("{}.{}", name_string, extension_string);
        }
        name_string
    }
}


#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct LongFileName {
    sequence_no: u8,
    name_1: [u16; 5],
    attrs: u8,
    lfn_type: u8,
    checksum: u8,
    name_2: [u16; 6],
    first_cluster: u16,
    name_3: [u16; 2],
}

impl LongFileName {
    /// The position of this LFN entry in the complete name, 1-based. On disk, the last LFN entry appears first.
    pub fn sequence_no(&self) -> u8 {
        // in a valid LFN entry, bits 0-4 represent the sequence number
        self.sequence_no & 0b00011111
    }

    pub fn to_utf8_string(&self) -> String {
        Self::ucs2_to_string(&self.to_ucs2_string())
    }

    fn to_ucs2_string(&self) -> Vec<u16> {
        // copy since name_1 and name_2 are unaligned, so borrowing them is undefined behavior
        let name_1 = self.name_1;
        let name_2 = self.name_2;
        let name_3 = self.name_3;

        let mut ucs_string = Vec::new();
        ucs_string.extend_from_slice(&name_1);
        ucs_string.extend_from_slice(&name_2);
        ucs_string.extend_from_slice(&name_3);

        ucs_string.into_iter().take_while(|&character| character != 0x0000).collect()
    }

    fn ucs2_to_string(ucs2_string: &[u16]) -> String {
        let mut utf8_bytes = Vec::new();
        ucs2::decode_with(&ucs2_string, |char_bytes| {
            utf8_bytes.extend_from_slice(char_bytes);
            Ok(())
        }).unwrap(); // TODO
        String::from_utf8(utf8_bytes).unwrap() // TODO
    }
}
