use crate::fat::{FatDentry, FatPartition, FatFile, ROOT_FAT_IDX, ClusterIdx};
use crate::stream_archiver::{StreamArchiver, Reader};

use chrono::prelude::*;
use std::convert::{TryFrom, TryInto};
use std::ops::Range;



type Timestamp = u32;

#[derive(Clone, Copy)]
enum FileType {
    Directory(u32), // contains child count
    RegularFile,
}

pub struct FsTreeSerializer<'a> {
    stream_archiver: StreamArchiver<'a>,
}

/// A slimmed down representation of the relevant components of a FAT dentry for serialization
/// This excludes the file name and the file's data ranges: since they have variable length,
/// they are treated separately.
struct DentryRepresentation {
    access_time: Timestamp,
    create_time: Timestamp,
    mod_time: Timestamp,
}

impl DentryRepresentation {
    pub fn from(dentry: &FatDentry) -> Self {
        Self {
            access_time: fat_time_to_unix_time(dentry.access_date, None),
            create_time: fat_time_to_unix_time(dentry.create_date, Some(dentry.create_time)),
            mod_time: fat_time_to_unix_time(dentry.mod_date, Some(dentry.mod_time)),
        }
    }
}

pub fn fat_time_to_unix_time(date: u16, time: Option<u16>) -> u32 {
    // datetm.tm_year = ((date & 0xFE00) >> 9) + 80;
    // datetm.tm_mon= ((date & 0x1E0) >> 5) - 1;
    // datetm.tm_mday = date & 0x1F;
    // datetm.tm_hour = (time & 0xF800) >> 11;
    // datetm.tm_min = (time & 0x7E0) >> 5;
    // datetm.tm_sec = (time & 0x1F) * 2;

    let year = ((date & 0xFE00) >> 9) + 1980;
    let month = (date & 0x1E0) >> 5;
    let day = date & 0x1F;
    let date = Utc.ymd(i32::from(year), u32::from(month), u32::from(day));

    let mut hour = 0;
    let mut minute = 0;
    let mut second = 0;
    if let Some(time) = time {
        hour = (time & 0xF800) >> 11;
        minute = (time & 0x7E0) >> 5;
        second = (time & 0x1F) * 2;
    }

    let datetime = date.and_hms(u32::from(hour), u32::from(minute), u32::from(second));
    u32::try_from(datetime.timestamp()).expect("Timestamp after year 2038 does not fit into 32 bits")
}

impl<'a> FsTreeSerializer<'a> {
    pub fn new(stream_archiver: StreamArchiver<'a>) -> Self {
        Self { stream_archiver }
    }

    pub fn serialize_directory_tree(&mut self, partition: &FatPartition) {
        let root_file = FatFile {
            name: "".to_string(),
            lfn_entries: Vec::new(),
            dentry: FatDentry::root_dentry(),
            data_ranges: partition.data_ranges(ROOT_FAT_IDX)
        };
        unsafe {
            self.serialize_directory(root_file, partition);
        }
    }

    unsafe fn serialize_directory(&mut self, file: FatFile, partition: &FatPartition) {
        let first_fat_idx = file.dentry.first_fat_index();
        let child_count = partition.dir_content_iter(first_fat_idx).count();
        self.archive_directory(file, child_count.try_into().unwrap());

        for file in partition.dir_content_iter(first_fat_idx) {
            if file.dentry.is_dir() {
                self.serialize_directory(file, partition);
            } else {
                self.archive_file(file);
            }
        }
    }

    pub fn archive_file(&mut self, file: FatFile) {
        self.stream_archiver.archive(vec![file.dentry]);
        self.stream_archiver.archive(file.name.into_bytes());
        self.stream_archiver.archive(vec![FileType::RegularFile]);
        self.stream_archiver.archive(file.data_ranges);
    }

    pub fn archive_directory(&mut self, file: FatFile, child_count: u32) {
        self.stream_archiver.archive(vec![file.dentry]);
        self.stream_archiver.archive(file.name.into_bytes());
        self.stream_archiver.archive(vec![FileType::Directory(child_count)]);
    }

    pub fn into_deserializer(self) -> FsTreeDeserializer<'a> {
        FsTreeDeserializer { reader: self.stream_archiver.into_reader() }
    }
}

pub struct FsTreeDeserializer<'a> {
    reader: Reader<'a>,
}

impl<'a> FsTreeDeserializer<'a> {
    pub fn deserialize_directory_tree(&mut self) {
        self.deserialize_file();
    }

    pub fn deserialize_file(&mut self) {
        unsafe {
            let dentry = self.reader.next::<FatDentry>()[0];
            let name = String::from_utf8(self.reader.next::<u8>()).unwrap();
            let file_type = self.reader.next::<FileType>()[0];
            match file_type {
                FileType::Directory(child_count) => {
                    for _ in 0..child_count {
                        self.deserialize_file();
                    }
                },
                FileType::RegularFile => { self.reader.next::<Range<ClusterIdx>>(); },
            }
            println!("{}", name);
        }
    }
}
