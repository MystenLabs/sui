// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::FieldMaskTree;
use super::FIELD_PATH_SEPARATOR;

use prost_types::FieldMask;

pub trait FieldMaskUtil: sealed::Sealed {
    fn normalize(self) -> FieldMask;

    fn from_str(s: &str) -> FieldMask;

    fn from_paths<I: AsRef<str>, T: IntoIterator<Item = I>>(paths: T) -> FieldMask;

    fn display(&self) -> impl std::fmt::Display + '_;
}

impl FieldMaskUtil for FieldMask {
    fn normalize(self) -> FieldMask {
        FieldMaskTree::from(self).to_field_mask()
    }

    fn from_str(s: &str) -> FieldMask {
        Self::from_paths(s.split(FIELD_PATH_SEPARATOR))
    }

    fn from_paths<I: AsRef<str>, T: IntoIterator<Item = I>>(paths: T) -> FieldMask {
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

    fn display(&self) -> impl std::fmt::Display + '_ {
        FieldMaskDisplay(self)
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

mod sealed {
    pub trait Sealed {}

    impl Sealed for prost_types::FieldMask {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_string() {
        assert!(FieldMask::display(&FieldMask::default())
            .to_string()
            .is_empty());

        let mask = FieldMask::from_paths(["foo"]);
        assert_eq!(FieldMask::display(&mask).to_string(), "foo");
        assert_eq!(mask.display().to_string(), "foo");
        let mask = FieldMask::from_paths(["foo", "bar"]);
        assert_eq!(FieldMask::display(&mask).to_string(), "foo,bar");

        // empty paths are ignored
        let mask = FieldMask::from_paths(["", "foo", "", "bar", ""]);
        assert_eq!(FieldMask::display(&mask).to_string(), "foo,bar");
    }

    #[test]
    fn test_from_str() {
        let mask = FieldMask::from_str("");
        assert!(mask.paths.is_empty());

        let mask = FieldMask::from_str("foo");
        assert_eq!(mask.paths.len(), 1);
        assert_eq!(mask.paths[0], "foo");

        let mask = FieldMask::from_str("foo,bar.baz");
        assert_eq!(mask.paths.len(), 2);
        assert_eq!(mask.paths[0], "foo");
        assert_eq!(mask.paths[1], "bar.baz");

        // empty field paths are ignored
        let mask = FieldMask::from_str(",foo,,bar,");
        assert_eq!(mask.paths.len(), 2);
        assert_eq!(mask.paths[0], "foo");
        assert_eq!(mask.paths[1], "bar");
    }
}
