# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import plistlib
from io import BytesIO


def _is_fmt_binary(header):
    return header[:8] == b"bplist00"


def detect_format_and_load(fp):
    header = fp.read(32)
    fp.seek(0)
    if _is_fmt_binary(header):
        fmt = plistlib.FMT_BINARY
    else:
        fmt = plistlib.FMT_XML
    return plistlib.load(fp, fmt=fmt)


def detect_format_and_loads(value):
    fp = BytesIO(value)
    return detect_format_and_load(fp)
