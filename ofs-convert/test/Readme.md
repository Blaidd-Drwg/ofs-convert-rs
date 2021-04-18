# Testing

Testing is done by converting FAT images and checking them with `fsck.ext4`, as well as comparing contents using `rsync`.

## Usage

Run with `./run.py path/to/ofs-convert tests_dir`.

`tests_dir` is a directory which contains `*.test` directories (either directly or in subdirectories).
Each `*.test` directory is a single test case, which contains either:
 * a `fat.img` image to test.
   Testing will be done on temporary copies.
 * a `generate.sh` script, with an accompanying `mkfs.args` file.
   The arguments from `mkfs.args` will be used to create a FAT image, which is then mounted.
   `generate.sh` will be called with the mount point as an argument and should fill the image with test data.
   
   `mkfs.args` should contain all arguments to `mkfs.fat`, excluding the path to the image file.
   It should always contain:
     - `-F 32`, to select FAT32 mode
     - `-s ...`, to select sectors per cluster (creating cluster 1k or larger)
     - `-S ...`, to select a sector size
     - `-C`, to create a new file
     - and, as the last argument, the number of 1k blocks in the created image file.
       The minimum number of blocks is 66055 + 1 for 1k clusters, 132110 + 1 for 2k clusters, etc.

When a test case fails, the output (stdout, stderr) of tools will be placed in files in the test cases directory.
No file will be created if there is no output.

`run.py` can also be used as a module for a `unittest` test runner.
In this case, the arguments must be specified using the `OFS_CONVERT` and `OFS_CONVERT_TESTS_DIR` environment variables.

All shell commands are run with a timeout, defaulting to 10 seconds.
This can be adjusted using the environment variable `OFS_CONVERT_TOOL_TIMEOUT` (in seconds).

## Requirements

 * Python 3.5+
 * `fsck.ext4`
 * `mkfs.fat`
 * `rsync`
 * support and permission for mounting `vfat` and `ext4` partitions using `mount` (on Linux)
 * `ext4fuse` (on macOS)
