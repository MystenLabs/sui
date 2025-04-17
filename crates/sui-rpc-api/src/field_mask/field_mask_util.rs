// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::message::MessageField;
use crate::message::MessageFields;

use super::FieldMaskTree;
use super::FIELD_PATH_SEPARATOR;
use super::FIELD_SEPARATOR;

use prost_types::FieldMask;

pub trait FieldMaskUtil: sealed::Sealed {
    fn normalize(self) -> FieldMask;

    fn from_str(s: &str) -> FieldMask;

    fn from_paths<I: AsRef<str>, T: IntoIterator<Item = I>>(paths: T) -> FieldMask;

    fn display(&self) -> impl std::fmt::Display + '_;

    fn validate<M: MessageFields>(&self) -> Result<(), &str>;
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

    fn validate<M: MessageFields>(&self) -> Result<(), &str> {
        // Determine if the provided path matches one of the provided fields. If a path matches a
        // field and that field is a message type (which can have its own set of fields), attempt
        // to match the remainder of the path against a field in the sub_message.
        fn is_valid_path(mut fields: &[&MessageField], mut path: &str) -> bool {
            loop {
                let (field_name, remainder) = path
                    .split_once(FIELD_SEPARATOR)
                    .map(|(field, remainder)| (field, (!remainder.is_empty()).then_some(remainder)))
                    .unwrap_or((path, None));

                if let Some(field) = fields.iter().find(|field| field.name == field_name) {
                    match (field.message_fields, remainder) {
                        (None, None) | (Some(_), None) => return true,
                        (None, Some(_)) => return false,
                        (Some(sub_message_fields), Some(remainder)) => {
                            fields = sub_message_fields;
                            path = remainder;
                        }
                    }
                } else {
                    return false;
                }
            }
        }

        for path in &self.paths {
            if !is_valid_path(M::FIELDS, path) {
                return Err(path);
            }
        }

        Ok(())
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

    #[test]
    fn test_validate() {
        struct Foo;
        impl MessageFields for Foo {
            const FIELDS: &'static [&'static MessageField] = &[
                &MessageField::new("bar").with_message_fields(Bar::FIELDS),
                &MessageField::new("baz"),
            ];
        }
        struct Bar;

        impl MessageFields for Bar {
            const FIELDS: &'static [&'static MessageField] = &[
                &MessageField {
                    name: "a",
                    message_fields: None,
                },
                &MessageField {
                    name: "b",
                    message_fields: None,
                },
            ];
        }

        let mask = FieldMask::from_str("");
        assert_eq!(mask.validate::<Foo>(), Ok(()));
        let mask = FieldMask::from_str("bar");
        assert_eq!(mask.validate::<Foo>(), Ok(()));
        let mask = FieldMask::from_str("bar.a");
        assert_eq!(mask.validate::<Foo>(), Ok(()));
        let mask = FieldMask::from_str("bar.a,bar.b");
        assert_eq!(mask.validate::<Foo>(), Ok(()));
        let mask = FieldMask::from_str("bar.a,bar.b,bar.c");
        assert_eq!(mask.validate::<Foo>(), Err("bar.c"));
        let mask = FieldMask::from_str("baz");
        assert_eq!(mask.validate::<Foo>(), Ok(()));
        let mask = FieldMask::from_str("baz.a");
        assert_eq!(mask.validate::<Foo>(), Err("baz.a"));
        let mask = FieldMask::from_str("foobar");
        assert_eq!(mask.validate::<Foo>(), Err("foobar"));
    }
}
