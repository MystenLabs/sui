# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import os
import platform
import stat


def make_path_user_writable(path: str):
    # On Linux, `os.chmod()` does not support setting the permissions on a symlink.
    # `chmod` manpage says:
    #   > AT_SYMLINK_NOFOLLOW     If pathname is a symbolic link, do not
    #   >     dereference it: instead operate on the link itself.
    #   >     This flag is not currently implemented.
    #
    # In Python, an exception will be thrown:
    # > NotImplementedError: chmod: follow_symlinks unavailable on this platform
    #
    # Darwin supports permission setting on symlinks.
    follow_symlinks = platform.system() != "Darwin"
    st = os.stat(path)
    os.chmod(path, st.st_mode | stat.S_IWUSR, follow_symlinks=follow_symlinks)


def make_dir_recursively_writable(dir: str):
    for dirpath, _, filenames in os.walk(dir):
        make_path_user_writable(dirpath)
        for filename in filenames:
            make_path_user_writable(os.path.join(dirpath, filename))
