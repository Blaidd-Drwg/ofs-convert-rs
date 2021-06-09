use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::process::Command;
use std::slice;

use fs2::FileExt;
use memmap::{MmapMut, MmapOptions};
use nix::ioctl_read;

// TODO check whether file is a FAT partition? although this will be hard to check if we can't rely on fsck.fat
// TODO macos support
pub struct Partition {
    mmap: MmapMut,
}

impl Partition {
    pub fn open<P: AsRef<Path>>(partition_path: P) -> io::Result<Self> {
        let partition_path = partition_path.as_ref().canonicalize()?;
        if Self::is_mounted(partition_path.as_path())? {
            return Err(io::Error::new(io::ErrorKind::AddrInUse, "Partition is already mounted"));
        }
        let file = OpenOptions::new().read(true).write(true).create(false).open(partition_path)?;
        // the lock is only advisory, other processes may still access the file
        // the lock is automatically released after both file and mmap are dropped
        file.try_lock_exclusive()?;

        let size = Self::get_file_size(&file)?;
        let mmap = unsafe { MmapOptions::new().len(size).map_mut(&file)? };
        Ok(Self { mmap })
    }

    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.mmap.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.mmap.as_mut_ptr()
    }

    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: TODO
        unsafe { slice::from_raw_parts(self.mmap.as_ptr(), self.mmap.len()) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: no aliasing because we borrow self as mut; valid length because we get
        // it from `self.mmap`; trivially aligned because it's u8
        unsafe { slice::from_raw_parts_mut(self.mmap.as_mut_ptr(), self.mmap.len()) }
    }

    fn get_file_size(file: &File) -> io::Result<usize> {
        let metadata = file.metadata()?;
        let filetype = metadata.file_type();
        if filetype.is_file() {
            return Ok(metadata.len() as usize);
        } else if filetype.is_block_device() {
            return Ok(Self::get_block_device_size as usize);
        }

        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Expected path to a file or a block device",
        ))
    }

    // error_chain?
    /// partition_path must be absolute
    fn is_mounted(partition_path: &Path) -> io::Result<bool> {
        let path_str = partition_path.to_str().expect("Partition path is not valid UTF-8");
        let output =
            String::from_utf8(Command::new("mount").output()?.stdout).expect("mount output is not valid UTF-8");
        Ok(output.lines().any(|line| line.starts_with(path_str)))
    }


    // declared in linux/fs.h
    #[cfg(target_os = "linux")]
    ioctl_read!(block_device_size, 0x12, 114, usize);

    #[cfg(target_os = "linux")]
    fn get_block_device_size(file: &File) -> io::Result<usize> {
        debug_assert!(file.metadata().unwrap().file_type().is_block_device());
        let mut size = 0;
        unsafe {
            match Self::block_device_size(file.as_raw_fd(), &mut size) {
                Err(e) => Err(Self::nix_error_to_io_error(e)),
                Ok(_) => Ok(size),
            }
        }
    }

    fn nix_error_to_io_error(err: nix::Error) -> io::Error {
        io::Error::new(io::ErrorKind::Other, err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn opens_file() {
        const FILE_SIZE: usize = 6427;
        let mut tmp_file = NamedTempFile::new().unwrap();
        tmp_file.as_file_mut().write(&[0; FILE_SIZE]).unwrap();

        let partition = Partition::open(tmp_file.path()).unwrap();
        assert_eq!(partition.len(), FILE_SIZE);
    }

    #[test]
    #[ignore] // requires sudo
    fn opens_block_device() {
        const BLOCK_DEVICE: &str = "/dev/sda"; // should use a loop device
        assert!(Partition::open(BLOCK_DEVICE).is_ok());
    }

    #[test]
    fn opens_symlink() {
        unimplemented!()
    }

    #[test]
    fn returns_err_if_file_does_not_exist() {
        let filename = "a_file_that_does_not_exist";
        assert!(!Path::new(filename).exists());
        let partition = Partition::open(filename);
        assert!(partition.is_err());
        assert!(partition.err().unwrap().kind() == io::ErrorKind::NotFound);
    }

    #[test]
    fn returns_err_if_file_not_writable() {
        let mut tmp_file = NamedTempFile::new().unwrap();
        let mut permissions = tmp_file.as_file_mut().metadata().unwrap().permissions();
        permissions.set_readonly(true);
        tmp_file.as_file_mut().set_permissions(permissions).unwrap();

        let partition = Partition::open(tmp_file.path());
        assert!(partition.is_err());
        assert!(partition.err().unwrap().kind() == io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn returns_err_if_not_file_or_device() {
        unimplemented!()
    }

    #[test]
    fn returns_err_if_file_locked() {
        unimplemented!()
    }

    #[test]
    fn returns_err_if_file_mounted() {
        unimplemented!()
    }

    #[test]
    fn returns_err_if_not_a_fat_partition() {
        unimplemented!()
    }

    #[test]
    fn has_correct_is_mounted() {
        unimplemented!()
    }

    #[test]
    fn has_correct_size() {
        unimplemented!()
    }

    #[test]
    fn has_working_mmap() {
        unimplemented!()
    }
}
