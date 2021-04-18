#!/usr/bin/env python3
import os
import pathlib
import shutil
import sys
import tempfile
import unittest

from utils import FsType, ImageMounter, ToolRunner


NOT_ENOUGH_CLUSTERS_MSG = 'WARNING: Not enough clusters for a 32 bit FAT!'


class OfsConvertTest(unittest.TestCase):
    _OFS_CONVERT = None

    @classmethod
    def setup_test_methods(cls, ofs_convert_path, tests_dir):
        cls._OFS_CONVERT = ofs_convert_path
        tool_timeout = os.environ.get('OFS_CONVERT_TOOL_TIMEOUT')
        tool_timeout = int(tool_timeout) if tool_timeout else None
        tests_dir = pathlib.Path(tests_dir)
        input_dirs = (e for e in tests_dir.glob('**/*.test') if e.is_dir())
        for input_dir in input_dirs:
            cls._add_test_method(tests_dir, input_dir, tool_timeout)

    @classmethod
    def _add_test_method(cls, tests_dir, input_dir, tool_timeout):
        fat_image_path = input_dir / 'fat.img'
        if fat_image_path.exists():
            def create_fat_image(*_args):
                return fat_image_path
        else:
            def create_fat_image(self, *args):
                return self._create_fat_image_from_gen_script(input_dir, *args)

        def test(self):
            self._run_test(input_dir, create_fat_image, tool_timeout)

        rel_path = input_dir.relative_to(tests_dir)
        parts = list(rel_path.parent.parts) + [rel_path.stem]
        meth_name = 'test_' + '__'.join(p.replace('-', '_') for p in parts)
        setattr(cls, meth_name, test)

    def _run_test(self, input_dir, create_fat_image, tool_timeout):
        tool_runner = ToolRunner(self, input_dir, tool_timeout)
        tool_runner.clean()
        with tempfile.TemporaryDirectory() as temp_dir_name:
            temp_dir = pathlib.Path(temp_dir_name)
            image_mounter = ImageMounter(tool_runner, temp_dir)
            try:
                fat_image_path = create_fat_image(self, temp_dir, tool_runner,
                                                  image_mounter)
                ext4_image_path = temp_dir / 'ext4.img'
                shutil.copyfile(str(fat_image_path), str(ext4_image_path))
                self._convert_to_ext4(tool_runner, ext4_image_path)
                self._run_fsck_ext4(tool_runner, ext4_image_path)
                self._check_contents(tool_runner, image_mounter, fat_image_path,
                                     ext4_image_path)
            except Exception:
                tool_runner.write_output()
                raise

    def _convert_to_ext4(self, tool_runner, fat_image_path):
        tool_runner.run([self._OFS_CONVERT, str(fat_image_path)], 'ofs-convert')

    def _handle_fsck_ext4_error(self, exc):
        if exc.returncode & ~12 == 0:
            self.fail('fsck.ext4 reported errors in converted image')
        return True

    def _run_fsck_ext4(self, tool_runner, ext4_image_path):
        tool_runner.run(['fsck.ext4', '-n', '-f', str(ext4_image_path)],
                        'fsck.ext4',
                        custom_error_handler=self._handle_fsck_ext4_error)

    def _check_rsync_output(self, proc):
        # Expect report about differing root directory times. These are
        # dependent on the fat driver, because fat doesn't actually store that
        # information (no dentry for the root directory)
        self.assertRegex(
            proc.stdout.decode('utf-8'), r'^\.d\.\.t\.+\ \./' + '\n$',
            'rsync reported differences between fat and ext4 images')

    def _check_contents(self, tool_runner, image_mounter, fat_image_path,
                        ext4_image_path):
        with image_mounter.mount(fat_image_path,
                                 FsType.VFAT, True) as fat_mount:
            with image_mounter.mount(ext4_image_path,
                                     FsType.EXT4, True) as ext_mount:
                # if the fat path doesn't end in a slash, rsync wants to copy
                # the directory and not its contents
                formatted_path_path = str(fat_mount)
                if not formatted_path_path.endswith('/'):
                    formatted_path_path += '/'
                args = ['rsync', '--dry-run', '--itemize-changes', '--archive',
                        '--checksum', '--no-perms', '--no-owner', '--no-group',
                        '--delete', '--exclude=/lost+found', formatted_path_path,
                        str(ext_mount)]
                tool_runner.run(args, 'rsync',
                                custom_output_checker=self._check_rsync_output)

    @staticmethod
    def _check_mkfs_fat_output(proc):
        stderr = proc.stderr.decode('utf-8')
        if NOT_ENOUGH_CLUSTERS_MSG in stderr:
            raise Exception('Too few clusters for FAT32 specified in test case')

    def _create_fat_image_from_gen_script(self, input_dir, temp_dir,
                                          tool_runner, image_mounter):
        image_file_path = temp_dir / 'fat.img'
        args_file = input_dir / 'mkfs.args'
        mkfs_call = 'mkfs.fat {} {}'.format(image_file_path,
                                            args_file.read_text().rstrip('\n'))
        tool_runner.run(mkfs_call, 'mkfs.fat', shell=True,
                        custom_output_checker=self._check_mkfs_fat_output)
        with image_mounter.mount(image_file_path,
                                 FsType.VFAT, False) as mount_point:
            tool_runner.run([str(input_dir / 'generate.sh'), str(mount_point)],
                            'gen script')
        return image_file_path


if __name__ == '__main__':
    # Executed as a script, load arguments from argv
    if len(sys.argv) != 3:
        print('Usage: {} ofs-convert tests_dir'.format(sys.argv[0]))
        exit(1)

    OfsConvertTest.setup_test_methods(sys.argv[1], sys.argv[2])
    unittest.main(argv=sys.argv[:1])
else:
    # Imported as a unittest module, load from env variables
    OfsConvertTest.setup_test_methods(os.environ['OFS_CONVERT'],
                                      os.environ['OFS_CONVERT_TESTS_DIR'])
