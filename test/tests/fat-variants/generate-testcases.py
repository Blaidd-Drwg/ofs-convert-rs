#!/usr/bin/env python3
import pathlib
import shutil
import stat


SECTOR_SIZES = [512, 1024, 2048, 4096]
SECTORS_PER_CLUSTERS = [1, 2, 4, 8, 16, 32, 64, 128]
ALL_EXECUTE = stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH

# Maximum number of 1k blocks in generated image (set to 1GB to keep testing
# time somewhat low)
MAX_IMAGE_SIZE = 1024 ** 2

# Should always be enough for a valid FAT32
CLUSTER_COUNT = 100000

GENERATE_SH_TEMPLATE = '''#!/usr/bin/env bash
mkdir -p "$1/dir/dir2"
dd if=/dev/zero bs={half_block_size} count=1 > "$1/small_file"
dd if=/dev/zero bs={half_block_size} count=3 > "$1/dir/file"
dd if=/dev/zero bs={block_size} count={bpg_plus_one} > "$1/dir/dir2/large_file"
'''


def generate_testcase(sector_size, sectors_per_cluster):
    block_size = sectors_per_cluster * sector_size
    one_k_blocks = CLUSTER_COUNT * block_size // 1024
    if block_size < 1024 or one_k_blocks > MAX_IMAGE_SIZE:
        return

    dir_name = '{}-{}.test'.format(sector_size, sectors_per_cluster)
    dir_path = pathlib.Path.cwd() / dir_name
    if dir_path.exists():
        print('Overwriting "{}"'.format(dir_name))
        shutil.rmtree(str(dir_path))
    dir_path.mkdir()

    blocks_per_group = block_size * 8
    args = '-C -F 32 -s {} -S {} {}'.format(sectors_per_cluster, sector_size,
                                            one_k_blocks)
    gen_sh = GENERATE_SH_TEMPLATE.format(block_size=block_size,
                                         half_block_size=block_size // 2,
                                         bpg_plus_one=blocks_per_group + 1)

    (dir_path / 'mkfs.args').write_text(args)
    gen_sh_path = dir_path / 'generate.sh'
    gen_sh_path.write_text(gen_sh)
    gen_sh_path.chmod(gen_sh_path.stat().st_mode | ALL_EXECUTE)


def main():
    for sector_size in SECTOR_SIZES:
        for sectors_per_cluster in SECTORS_PER_CLUSTERS:
            generate_testcase(sector_size, sectors_per_cluster)


if __name__ == '__main__':
    main()
