# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import io
import unittest

from .preprocess import preprocess


class TestPreprocess(unittest.TestCase):
    def test_default_substitution_values(self):
        input_file = io.StringIO(
            r"""
<string>${PRODUCT_NAME}</string>
<string>${EXECUTABLE_NAME}</string>
"""
        )
        expected = r"""
<string>my_product_name</string>
<string>my_product_name</string>
"""
        substitutions_file = io.StringIO(r"{}")
        output_file = io.StringIO("")
        preprocess(input_file, output_file, substitutions_file, "my_product_name")
        self.assertEqual(output_file.getvalue(), expected)

    def test_json_substitution_values_precede_default_ones(self):
        input_file = io.StringIO(
            r"""
<string>${PRODUCT_NAME}</string>
<string>${EXECUTABLE_NAME}</string>
"""
        )
        expected = r"""
<string>foo</string>
<string>bar</string>
"""
        substitutions_file = io.StringIO(
            r"""
{
    "PRODUCT_NAME": "foo",
    "EXECUTABLE_NAME": "bar"
}
"""
        )
        output_file = io.StringIO("")
        preprocess(input_file, output_file, substitutions_file, "my_product_name")
        self.assertEqual(output_file.getvalue(), expected)

    def test_chained_substitutions(self):
        input_file = io.StringIO(r"<string>${foo}</string>")
        expected = r"<string>baz</string>"
        substitutions_file = io.StringIO(
            r"""
{
    "foo": "${bar}",
    "bar": "baz"
}
"""
        )
        output_file = io.StringIO("")
        preprocess(input_file, output_file, substitutions_file, "my_product_name")
        self.assertEqual(output_file.getvalue(), expected)

    def test_recursive_substitutions_throws(self):
        input_file = io.StringIO(r"<string>${foo}</string>")
        substitutions_file = io.StringIO(
            r"""
{
    "foo": "${bar}",
    "bar": "${foo}"
}
"""
        )
        output_file = io.StringIO("")
        with self.assertRaises(Exception) as context:
            preprocess(input_file, output_file, substitutions_file, "my_product_name")
        self.assertTrue("Recursive" in str(context.exception))

    def test_variable_with_modifier(self):
        input_file = io.StringIO(r"<string>${foo:modifier}</string>")
        expected = r"<string>bar</string>"
        substitutions_file = io.StringIO(r'{"foo":"bar"}')
        output_file = io.StringIO("")
        preprocess(input_file, output_file, substitutions_file, "my_product_name")
        self.assertEqual(output_file.getvalue(), expected)
