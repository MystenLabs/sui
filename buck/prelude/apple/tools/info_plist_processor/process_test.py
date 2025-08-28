# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import io
import plistlib
import unittest

from .process import process


class TestProcess(unittest.TestCase):
    def test_canary(self):
        input_file = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>foo</key>
    <string>bar</string>
</dict>
</plist>
"""
        )
        output_file = io.BytesIO()
        process(input_file, output_file)
        self.assertTrue(len(output_file.getvalue()) > 0)

    def test_additional_input_given_no_keys_conflict(self):
        input_file = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>foo</key>
    <string>bar</string>
</dict>
</plist>
"""
        )
        override_input_file = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>baz</key>
    <string>qux</string>
</dict>
</plist>
"""
        )
        output_file = io.BytesIO()
        process(input_file, output_file, override_input_file)
        output_file.seek(0)
        root = plistlib.load(output_file)
        self.assertEquals(root, {"foo": "bar", "baz": "qux"})

    def test_additional_input_given_keys_conflict(self):
        input_file = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>foo</key>
    <string>bar</string>
    <key>qux</key>
    <dict>
        <key>a</key>
        <string>b</string>
        <key>b</key>
        <string>c</string>
    </dict>
    <key>foobar</key>
    <dict>
        <key>hello</key>
        <string>world</string>
    </dict>
</dict>
</plist>
"""
        )
        override_input_file = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>foo</key>
    <string>baz</string>
    <key>qux</key>
    <dict>
        <key>a</key>
        <string>z</string>
        <key>c</key>
        <string>x</string>
    </dict>
    <key>foobar</key>
    <string>zanzibar</string>
</dict>
</plist>
"""
        )
        output_file = io.BytesIO()
        process(input_file, output_file, override_input_file)
        output_file.seek(0)
        root = plistlib.load(output_file)
        self.assertEquals(
            root,
            {"foo": "baz", "qux": {"a": "z", "b": "c", "c": "x"}, "foobar": "zanzibar"},
        )

    def test_additional_keys(self):
        input_file = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>foo</key>
    <string>bar</string>
</dict>
</plist>
"""
        )
        additional_keys = {"baz": "qux"}
        output_file = io.BytesIO()
        process(input_file, output_file, additional_keys=additional_keys)
        output_file.seek(0)
        root = plistlib.load(output_file)
        self.assertEquals(root, {"foo": "bar", "baz": "qux"})

    def test_additional_keys_do_not_override(self):
        input_file = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>foo</key>
    <string>bar</string>
</dict>
</plist>
"""
        )
        additional_keys = {"foo": "baz"}
        output_file = io.BytesIO()
        process(input_file, output_file, additional_keys=additional_keys)
        output_file.seek(0)
        root = plistlib.load(output_file)
        self.assertEquals(root, {"foo": "bar"})

    def test_additional_keys_from_file(self):
        input_file = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>foo</key>
    <string>bar</string>
</dict>
</plist>
"""
        )
        additional_keys_file = io.BytesIO(b"""{"baz": "qux"}""")
        output_file = io.BytesIO()
        process(input_file, output_file, additional_keys_file=additional_keys_file)
        output_file.seek(0)
        root = plistlib.load(output_file)
        self.assertEquals(root, {"foo": "bar", "baz": "qux"})

    def test_override_keys_from_file(self):
        input_file = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>foo</key>
    <string>bar</string>
</dict>
</plist>
"""
        )
        override_keys_file = io.BytesIO(b"""{"foo": "baz"}""")
        output_file = io.BytesIO()
        process(input_file, output_file, override_keys_file=override_keys_file)
        output_file.seek(0)
        root = plistlib.load(output_file)
        self.assertEquals(root, {"foo": "baz"})
