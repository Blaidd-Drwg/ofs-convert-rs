# ofs-convert-rs [![Tests](https://github.com/Blaidd-Drwg/ofs-convert-rs/actions/workflows/ci.yaml/badge.svg?branch=master)](https://github.com/Blaidd-Drwg/ofs-convert-rs/actions/workflows/ci.yaml)
`ofs-convert-rs` is a proof-of-concept tool for converting a FAT32 filesystem into an ext4 filesystem in-place, using the free space within the filesystem as temporary storage. It is a Rust rewrite of [ofs-convert](https://github.com/Blaidd-Drwg/ofs-convert) and (for now) runs only on Linux.


### ⚠️ DISCLAIMER ⚠️
Do not use `ofs-convert-rs` on data that you haven't previously backed up: it's experimental software that has not been exhaustively tested and any bug could irreversibly corrupt the entire filesystem. This program comes with absolutely no warranty.


## How it works
The usual way of converting between two filesystems involves copying all files to a separate filesystem, formatting the source partition with the desired target filesystem, and copying all files back. `ofs-convert-rs` greatly speeds up this process by leaving the file contents in place and only converting the filesystem-specific metadata structures. Free space in the filesystem is used for temporary storage, so that no additional filesystem is needed and RAM requirements are minimized.

`ofs-convert-rs` performs the conversion with the following steps:

1. Read the source filesystem's structure and derive the target filesystem's structure from it.
2. Serialize the source filesystem's directory tree, storing it in the filesystem's free blocks.
3. While iterating through the directory tree for step 2, copy any data block that will be overwritten with ext4 metadata to a free block.
4. Perform a dry run of the serialized directory tree's conversion. If `ofs-convert-rs` fails before step 5, the source filesystem is left in a consistent state.
5. Initialize the target filesystem's metadata.
6. Perform the conversion of the serialized directory tree.


## Running
Build the executable in the directory `target/release` with:
```
$ cargo build --release
```

It is recommended to install `fsck.fat` so that `ofs-convert-rs` can check the input filesystem for consistency.

The program takes the following arguments:
```
USAGE:
    ofs-convert-rs [FLAGS] <PARTITION_PATH>

FLAGS:
    -f, --force      Skip fsck (can lead to unexpected errors and data loss if the input filesystem is inconsistent)

ARGS:
    <PARTITION_PATH>    The partition containing the FAT32 filesystem that should be converted. This will usually be
                        a block device (e.g. /dev/sda1), but it can also be a file containing a disk image. The
                        filesystem must be unmounted and must not be modified by another process during the conversion
```


## Testing
Unit tests are implemented in Rust and can be directly run through `cargo`, integration tests require running a separate Python script. Alternatively, all tests can be run with a single command inside a Docker container.

### Unit tests
Run the unit tests with:
```
$ cargo test
```

Some tests are ignored by default because they require superuser privileges. Run them with:
```
$ cargo test-sudo
```

### Integration tests
Running the integration tests requires superuser privileges in order to mount the test filesystems. It also requires Python 3.5+, `fsck.ext4`, `mkfs.fat` and `rsync` to be installed. Run the tests with:
```
# test/run.py /path/to/ofs-convert-executable test/tests
```

For more information on the integration tests, see `test/README.md`.

### Running all tests inside a Docker container
Build the image with:
```
$ test/container/build.sh
```
The image will cache the dependencies specified in the `Cargo.toml` file. The source code and tests are mounted as volumes when the tests are run, so a rebuild is only required after changing `Cargo.toml` or after modifying a file in `test/container`.

Start the container and run the tests with:
```
$ test/container/run.sh
```
