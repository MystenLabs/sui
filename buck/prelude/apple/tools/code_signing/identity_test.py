# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import unittest

from .identity import CodeSigningIdentity


class TestParse(unittest.TestCase):
    def test_multiple_certificates_are_parsed(self):
        text = r"""
  1) 5C5E14F66E6B3C2697107764C9D728EE5AB393B9 "Apple Development: Johnny Appleseed (B4H6M5LP3J)"
  2) 3348E051F7E7E1ED509D2D620567BAF796210C36 "iPhone Developer: Johnny Appleseed (B4H6M5LP3J)"
     2 valid identities found
"""
        expected = [
            CodeSigningIdentity(
                "5C5E14F66E6B3C2697107764C9D728EE5AB393B9",
                "Apple Development: Johnny Appleseed (B4H6M5LP3J)",
            ),
            CodeSigningIdentity(
                "3348E051F7E7E1ED509D2D620567BAF796210C36",
                "iPhone Developer: Johnny Appleseed (B4H6M5LP3J)",
            ),
        ]
        result = CodeSigningIdentity.parse_security_stdout(text)
        self.assertEqual(expected, result)

    def test_expired_certificates_are_ignored(self):
        text = r"""
  1) 5C5E14F66E6B3C2697107764C9D728EE5AB393B9 "Apple Development: Johnny Appleseed (B4H6M5LP3J)"
  2) 3348E051F7E7E1ED509D2D620567BAF796210C36 "iPhone Developer: Johnny Appleseed (B4H6M5LP3J)" (CSSMERR_TP_CERT_EXPIRED)
     2 valid identities found
"""
        expected = [
            CodeSigningIdentity(
                "5C5E14F66E6B3C2697107764C9D728EE5AB393B9",
                "Apple Development: Johnny Appleseed (B4H6M5LP3J)",
            ),
        ]
        result = CodeSigningIdentity.parse_security_stdout(text)
        self.assertEqual(expected, result)
