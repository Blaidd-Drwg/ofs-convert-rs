use crate::lohi::LoHi;
use crate::fat::FatTableIndex;

#[repr(C)]
pub union FatPseudoDentry {
    dentry: FatDentry,
    long_file_name: LongFileName,
}

impl FatPseudoDentry {
    pub fn as_dentry(&self) -> Option<&FatDentry> {
        // SAFETY: this is safe, since we only access the union if the check succeeds
        unsafe { (!self.is_long_file_name()).then(|| &self.dentry) }
    }

    pub fn as_long_file_name(&self) -> Option<&LongFileName> {
        // SAFETY: this is safe, since we only access the union if the check succeeds
        unsafe { self.is_long_file_name().then(|| &self.long_file_name) }
    }

    pub fn is_long_file_name(&self) -> bool {
        // SAFETY: attrs field is in the same position for both FatDentry and LongFileName,
        // so this is safe
        unsafe {
            self.long_file_name.attrs & 0x0F != 0
        }
    }

    /// True iff self is invalid but the directory might more valid dentries
    pub fn is_invalid(&self) -> bool {
        // SAFETY: the first byte is used to mark invalid entries both for dentries and
        // LFN entries, so this is safe
        unsafe {
            self.long_file_name.sequence_no == 0xE5
        }
    }

    /// True iff self is invalid and the directory contains no more valid dentries
    pub fn is_dir_table_end(&self) -> bool {
        // SAFETY: we misuse `sequence_no` to check the first byte, regardless of
        // whether it's a dentry or LFN entry
        unsafe {
            self.long_file_name.sequence_no == 0x00
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

#[repr(C)] // technically packed but it already has no padding by default
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct FatDentry {
    pub short_name: [u8; 8],
    pub short_extension: [u8; 3],
    pub attrs: u8,
    pub short_name_case: u8,
    pub create_time_10_ms: u8,
    pub create_time: u16,
    pub create_date: u16,
    pub access_date: u16,
    pub first_fat_index_hi: u16,
    pub mod_time: u16,
    pub mod_date: u16,
    pub first_fat_index_lo: u16,
    pub file_size: u32,
}

impl FatDentry {
    const DIR_FLAG: u8 = 0x10;

    pub fn first_fat_index(&self) -> FatTableIndex {
        let idx = LoHi::new(&self.first_fat_index_lo, &self.first_fat_index_hi).get();
        FatTableIndex::new(idx)
    }

    pub fn is_dir(&self) -> bool {
        self.attrs & Self::DIR_FLAG != 0
    }

    /// True iff the dentry represents either the current directory `.` or the parent directory `..`
    pub fn is_dot_dir(&self) -> bool {
        self.short_name[0] == b'.'
    }

    /// True iff the file name has an extension
    pub fn has_file_extension(&self) -> bool {
        self.short_extension[0] != b' '
    }

    pub fn has_lowercase_name(&self) -> bool {
        self.short_name_case & 0x8 != 0
    }

    pub fn has_lowercase_extension(&self) -> bool {
        self.short_name_case & 0x10 != 0
    }

    // TODO what encoding do FAT short names use?  Assume ascii for now
    pub fn read_short_file_name(&self) -> String {
        let name_ascii_bytes: Vec<_> = self.short_name.iter().copied().collect();
        let mut name_string = String::from_utf8(name_ascii_bytes).unwrap().trim_end().to_string();
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
    /// The position of this LFN entry in the complete name, 1-based. On disk, LFN entries appear
    /// in reverse order, so the first entry's `sequence_no` equals the number of entries.
    pub fn sequence_no(&self) -> u8 {
        // in a valid LFN entry, bits 0-4 represent the sequence number
        self.sequence_no & 0b00011111
    }

    // TODO handle Errors
    pub fn to_utf8_string(self) -> String {
        std::char::decode_utf16(self.to_utf16_string()).map(|utf16_char| utf16_char.unwrap()).collect()
    }

    // By the standard, long file names are encoded in UCS-2. However, the Linux implementation
    // actually uses UTF-16. UTF-16 is backwards compatible with UCS-2 and can encode a superset
    // of the characters encodable with UCS-2, so to support files written by Linux that contain
    // these characters, we treat the file names as UTF-16.
    pub fn to_utf16_string(self) -> Vec<u16> {
        let mut ucs_string = Vec::new();
        ucs_string.extend(self.name_1);
        ucs_string.extend(self.name_2);
        ucs_string.extend(self.name_3);

        ucs_string.into_iter().take_while(|&character| character != 0x0000).collect()
    }
}
