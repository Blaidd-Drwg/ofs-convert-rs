use std::any::Any;
use std::marker::PhantomData;
use std::ops::Range;

use anyhow::Result;

use crate::fat::ClusterIdx;
use crate::serialization::{DentryRepresentation, FileType};


pub trait DirectoryWriter {}


pub struct Deserializer<'a, I: DeserializerInternals<'a>> {
    pub internals: I,
    pub _lifetime: PhantomData<&'a ()>,
}

impl<'a, I: DeserializerInternals<'a>> Deserializer<'a, I> {
    pub fn deserialize_directory_tree(&mut self) -> Result<()> {
        let mut root_directory_writer = self.internals.build_root()?;

        for _ in 0..self.internals.read_root_child_count() {
            self.internals.deserialize_file(&mut root_directory_writer)?;
        }
        Ok(())
    }
}

pub trait DeserializerInternals<'a> {
    type D: DirectoryWriter;

    fn build_root(&mut self) -> Result<Self::D>;

    fn deserialize_directory(
        &mut self,
        dentry: DentryRepresentation,
        name: String,
        parent_directory_writer: &mut Self::D,
    ) -> Result<Self::D>;

    fn deserialize_regular_file(
        &mut self,
        dentry: DentryRepresentation,
        name: String,
        data_ranges: Vec<Range<ClusterIdx>>,
        parent_directory_writer: &mut Self::D,
    ) -> Result<()>;

    fn read_next<T: Any>(&mut self) -> Vec<T>;


    fn deserialize_file(&mut self, parent_directory_writer: &mut Self::D) -> Result<()> {
        let file_type = self.read_next::<FileType>()[0];
        let dentry = self.read_next::<DentryRepresentation>()[0];
        let name = String::from_utf8(self.read_next::<u8>())
            .expect("File name is no longer a valid String after deserialization");

        match file_type {
            FileType::Directory(child_count) => {
                let mut directory_writer = self.deserialize_directory(dentry, name, parent_directory_writer)?;
                for _ in 0..child_count {
                    self.deserialize_file(&mut directory_writer)?;
                }
            }
            FileType::RegularFile => {
                let data_ranges = self.read_next::<Range<ClusterIdx>>();
                self.deserialize_regular_file(dentry, name, data_ranges, parent_directory_writer)?;
            }
        }
        Ok(())
    }

    fn read_root_child_count(&mut self) -> u32 {
        if let FileType::Directory(child_count) = self.read_next::<FileType>()[0] {
            child_count
        } else {
            panic!("First StreamArchiver entry is not root directory child count");
        }
    }
}
