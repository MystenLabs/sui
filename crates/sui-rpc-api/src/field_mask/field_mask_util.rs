// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::FieldMaskTree;
use super::FIELD_PATH_SEPARATOR;

use prost_types::FieldMask;

pub struct FieldMaskUtil;

impl FieldMaskUtil {
    pub fn normalize(field_mask: FieldMask) -> FieldMask {
        FieldMaskTree::from(field_mask).to_field_mask()
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> FieldMask {
        Self::from_paths(s.split(FIELD_PATH_SEPARATOR))
    }

    pub fn from_paths<I: AsRef<str>, T: IntoIterator<Item = I>>(paths: T) -> FieldMask {
        FieldMask {
            paths: paths
                .into_iter()
                .filter_map(|path| {
                    let path = path.as_ref();
                    if path.is_empty() {
                        None
                    } else {
                        Some(path.to_owned())
                    }
                })
                .collect(),
        }
    }

    pub fn display(field_mask: &FieldMask) -> impl std::fmt::Display + '_ {
        FieldMaskDisplay(field_mask)
    }
}

struct FieldMaskDisplay<'a>(&'a FieldMask);

impl std::fmt::Display for FieldMaskDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write;

        let mut first = true;

        for path in &self.0.paths {
            // Ignore empty paths
            if path.is_empty() {
                continue;
            }

            // If this isn't the first path we've printed,
            // we need to print a FIELD_PATH_SEPARATOR character
            if first {
                first = false;
            } else {
                f.write_char(FIELD_PATH_SEPARATOR)?;
            }
            f.write_str(path)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_string() {
        assert!(FieldMaskUtil::display(&FieldMask::default())
            .to_string()
            .is_empty());

        let mask = FieldMaskUtil::from_paths(["foo"]);
        assert_eq!(FieldMaskUtil::display(&mask).to_string(), "foo");
        let mask = FieldMaskUtil::from_paths(["foo", "bar"]);
        assert_eq!(FieldMaskUtil::display(&mask).to_string(), "foo,bar");

        // empty paths are ignored
        let mask = FieldMaskUtil::from_paths(["", "foo", "", "bar", ""]);
        assert_eq!(FieldMaskUtil::display(&mask).to_string(), "foo,bar");
    }

    #[test]
    fn test_from_str() {
        let mask = FieldMaskUtil::from_str("");
        assert!(mask.paths.is_empty());

        let mask = FieldMaskUtil::from_str("foo");
        assert_eq!(mask.paths.len(), 1);
        assert_eq!(mask.paths[0], "foo");

        let mask = FieldMaskUtil::from_str("foo,bar.baz");
        assert_eq!(mask.paths.len(), 2);
        assert_eq!(mask.paths[0], "foo");
        assert_eq!(mask.paths[1], "bar.baz");

        // empty field paths are ignored
        let mask = FieldMaskUtil::from_str(",foo,,bar,");
        assert_eq!(mask.paths.len(), 2);
        assert_eq!(mask.paths[0], "foo");
        assert_eq!(mask.paths[1], "bar");
    }
}
