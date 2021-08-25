mod deserializer;
mod dry_run_deserializer;
mod ext4_deserializer;
mod fat_serializer;
mod stream_archiver;

pub use self::deserializer::*;
pub use self::dry_run_deserializer::*;
pub use self::ext4_deserializer::*;
pub use self::fat_serializer::*;
pub use self::stream_archiver::*;

#[derive(Clone, Copy)]
pub enum FileType {
    Directory(u32), // contains child count
    RegularFile,
}
