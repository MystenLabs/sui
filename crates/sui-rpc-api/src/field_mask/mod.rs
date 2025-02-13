// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod field_mask_tree;
mod field_mask_util;

pub use field_mask_tree::FieldMaskTree;
pub use field_mask_util::FieldMaskUtil;

/// Separator between field paths when a FieldMask is encoded as a string
const FIELD_PATH_SEPARATOR: char = ',';
/// Separator between fields in a field path
pub const FIELD_SEPARATOR: char = '.';

fn is_valid_path(path: &str) -> bool {
    if path == "*" {
        return true;
    }

    path.split(FIELD_SEPARATOR).all(is_valid_path_component)
}

// A valid path component needs to be a valid protobuf identifier which is defined by the
// following:
//
// ```
// letter        = "A" … "Z" | "a" … "z" | "_" .
// decimal_digit = "0" … "9"
// identifier = letter { letter | decimal_digit }
// ```
fn is_valid_path_component(component: &str) -> bool {
    if component.is_empty() || component == "_" {
        return false;
    }

    let component = component.as_bytes();

    if !(component[0].is_ascii_alphabetic() || component[0] == b'_') {
        return false;
    }

    for &byte in &component[1..] {
        if !(byte.is_ascii_alphabetic() || byte.is_ascii_digit() || byte == b'_') {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_path_component() {
        let cases = [
            ("foo", true),
            ("_", false),
            ("", false),
            ("_abc", true),
            ("BAR", true),
            ("foo.bar", false),
        ];

        for (case, expected) in cases {
            assert_eq!(is_valid_path_component(case), expected);
        }
    }

    #[test]
    fn test_valid_path() {
        let cases = [
            ("*", true),
            ("**", false),
            ("foo.bar", true),
            ("foo.bar.baz", true),
            ("_", false),
            (".", false),
            ("", false),
            ("_abc", true),
            ("BAR", true),
        ];

        for (case, expected) in cases {
            assert_eq!(is_valid_path(case), expected);
        }
    }
}
