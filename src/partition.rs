use std::convert::TryInto;
use std::fs::{File, OpenOptions};
use std::marker::PhantomData;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use fs2::FileExt;
use memmap::{MmapMut, MmapOptions};
use nix::ioctl_read;

pub struct Partition<'a> {
    mmap: MmapMut,
    pub lifetime: PhantomData<&'a ()>,
}

impl<'a> Partition<'a> {
    pub fn open<P: AsRef<Path>>(partition_path: P) -> Result<Self> {
        let partition_path = partition_path.as_ref().canonicalize()?;
        if Self::is_mounted(partition_path.as_path())? {
            bail!("Partition is already mounted");
        }
        let file = OpenOptions::new().read(true).write(true).create(false).open(partition_path)?;
        // the lock is only advisory, other processes may still access the file
        // the lock is automatically released after both file and mmap are dropped
        file.try_lock_exclusive()?;

        let size = Self::get_file_size(&file)?;
        // SAFETY: We assume that no other process is modifying the partition
        let mmap = unsafe { MmapOptions::new().len(size).map_mut(&file)? };
        Ok(Self { mmap, lifetime: PhantomData })
    }

    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.mmap.as_mut_ptr()
    }

    fn get_file_size(file: &File) -> Result<usize> {
        let metadata = file.metadata()?;
        let filetype = metadata.file_type();
        let len = if filetype.is_file() {
            metadata.len()
        } else if filetype.is_block_device() {
            Self::get_block_device_size(file)?
        } else {
            bail!("Expected path to a file or a block device")
        };

        len.try_into()
            .with_context(|| format!("File size {} does not fit into a usize", len))
    }

    fn is_mounted(partition_path: &Path) -> Result<bool> {
        let absolute_path = partition_path.canonicalize()?;
        let path_str = absolute_path.to_str().context("Partition path is not valid UTF-8")?;
        let command_output = Command::new("mount").output()?;
        command_output.status.exit_ok()?;
        let output = String::from_utf8(command_output.stdout).expect("mount output is not valid UTF-8");
        Ok(output.lines().any(|line| line.starts_with(path_str)))
    }

    // declared in linux/fs.h
    // The type is declared as size_t due to a bug that cannot be fixed due to backwards compatibility. If I understand
    // correctly, passing u64 instead of usize should work even on 32bit systems, I haven't had a chance to test it
    // though. cfr. https://lists.debian.org/debian-glibc/2005/12/msg00069.html
    #[cfg(target_os = "linux")]
    ioctl_read!(block_device_size, 0x12, 114, u64);

    /// PANICS: Panics if `file` is not a block device.
    #[cfg(target_os = "linux")]
    fn get_block_device_size(file: &File) -> Result<u64> {
        assert!(file.metadata()?.file_type().is_block_device());
        let mut size = 0;
        // SAFETY: the nix crate provides no safety documentation, so we must just assume that this is safe.
        unsafe {
            Self::block_device_size(file.as_raw_fd(), &mut size)?;
        }
        Ok(size)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};
    use std::os::unix::fs::symlink;

    use itertools::Itertools;
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;
    use rand::distributions::Standard;
    use rand::{self, Rng};
    use tempfile::{tempdir, NamedTempFile};

    use super::*;

    #[test]
    fn opens_file() {
        const FILE_SIZE: usize = 6427;
        let content = rand::thread_rng().sample_iter(&Standard).take(FILE_SIZE).collect_vec();
        let mut tmp_file = NamedTempFile::new().unwrap();
        tmp_file.as_file_mut().write_all(&content).unwrap();

        let mut partition = Partition::open(tmp_file.path()).unwrap();
        assert_eq!(partition.len(), FILE_SIZE);
        let part_content = unsafe { std::slice::from_raw_parts(partition.as_mut_ptr(), FILE_SIZE) };
        assert_eq!(part_content, content);
    }

    #[test]
    #[ignore] // requires sudo or group membership in "disk"
    fn opens_block_device() {
        const FILE_SIZE: usize = 2560; // must be multiple of 512
        let content = rand::thread_rng().sample_iter(&Standard).take(FILE_SIZE).collect_vec();
        let mut tmp_file = NamedTempFile::new().unwrap();
        tmp_file.as_file_mut().write_all(&content).unwrap();

        let path_str = tmp_file.path().to_str().unwrap();
        let loop_cmd = Command::new("losetup").args(["-f", "--show", path_str]).output();
        let loop_device = String::from_utf8(loop_cmd.unwrap().stdout).unwrap().trim().to_string();
        let mut partition = Partition::open(loop_device).unwrap();

        assert_eq!(partition.len(), FILE_SIZE);
        let part_content = unsafe { std::slice::from_raw_parts(partition.as_mut_ptr(), FILE_SIZE) };
        assert_eq!(part_content, content);
    }

    #[test]
    fn opens_symlink() {
        const FILE_SIZE: usize = 6427;
        const FILE_NAME: &str = "file";
        const LINK_NAME: &str = "link";
        let content = rand::thread_rng().sample_iter(&Standard).take(FILE_SIZE).collect_vec();
        let tmp_dir = tempdir().unwrap();
        let file_path = tmp_dir.path().join(FILE_NAME);
        let link_path = tmp_dir.path().join(LINK_NAME);
        let mut tmp_file = File::create(&file_path).unwrap();
        tmp_file.write_all(&content).unwrap();
        symlink(&file_path, &link_path).unwrap();

        let mut partition = Partition::open(link_path).unwrap();
        assert_eq!(partition.len(), FILE_SIZE);
        let part_content = unsafe { std::slice::from_raw_parts(partition.as_mut_ptr(), FILE_SIZE) };
        assert_eq!(part_content, content);
    }

    #[test]
    fn returns_err_if_file_does_not_exist() {
        let filename = "a_file_that_does_not_exist";
        assert!(!Path::new(filename).exists());
        let partition = Partition::open(filename);
        assert!(partition.is_err());
        assert_eq!(io_error_kind(partition.err().unwrap()), io::ErrorKind::NotFound);
    }

    #[test]
    fn returns_err_if_not_file_or_device() {
        const FILE_NAME: &str = "file";
        let tmp_dir = tempdir().unwrap();
        let fifo_path = tmp_dir.path().join(FILE_NAME);
        mkfifo(&fifo_path, Mode::S_IRWXU).unwrap();
        assert!(Partition::open(tmp_dir.path()).is_err());
    }

    #[test]
    fn returns_err_if_file_locked() {
        let mut tmp_file = NamedTempFile::new().unwrap();
        tmp_file.write_all(&[0; 512]).unwrap();
        tmp_file.as_file().try_lock_exclusive().unwrap();
        assert!(Partition::open(tmp_file.path()).is_err());
    }

    #[test]
    #[ignore] // requires sudo
    fn returns_err_if_file_mounted() {
        let path = Path::new("test/example_fat.img");
        assert!(!Partition::is_mounted(path).unwrap());
        let mount_dir = tempdir().unwrap();
        let _umount_on_drop = Mount::new(path, mount_dir.path()).unwrap();
        assert!(Partition::open(path).is_err());
    }

    #[test]
    #[ignore] // requires sudo
    fn has_correct_is_mounted() {
        let path = Path::new("test/example_fat.img");
        assert!(!Partition::is_mounted(path).unwrap());
        let mount_dir = tempdir().unwrap();
        let _umount_on_drop = Mount::new(path, mount_dir.path()).unwrap();
        assert!(Partition::is_mounted(path).unwrap());
    }

    fn io_error_kind(err: anyhow::Error) -> io::ErrorKind {
        err.chain().next().unwrap().downcast_ref::<io::Error>().unwrap().kind()
    }

    struct Mount {
        target: String,
    }

    impl Mount {
        fn new(source: impl AsRef<Path>, target: impl AsRef<Path>) -> Result<Self> {
            let source_str = source.as_ref().to_str().unwrap();
            let target_str = target.as_ref().to_str().unwrap();
            let mount_output = Command::new("mount").args([source_str, target_str]).output()?;
            mount_output.status.exit_ok()?;
            Ok(Self { target: target_str.to_string() })
        }
    }

    impl Drop for Mount {
        fn drop(&mut self) {
            let _ = Command::new("umount").arg(&self.target).status();
        }
    }
}
