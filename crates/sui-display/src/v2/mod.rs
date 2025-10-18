// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use error::FormatError;
use futures::future::try_join_all;
use futures::join;
use indexmap::IndexMap;
use move_core_types::annotated_value::MoveTypeLayout;
use sui_types::collection_types::{Entry, VecMap};

use self::error::Error;
use self::interpreter::Interpreter;
use self::meter::{Limits, Meter};
use self::parser::{Parser, Strand};
use self::value::{Slice, Store};

pub mod error;
pub(crate) mod interpreter;
pub mod lexer;
pub mod meter;
pub(crate) mod parser;
pub(crate) mod peek;
pub mod value;
pub(crate) mod visitor;

pub(crate) mod writer;

/// Format strings extracted from a `Display` object on-chain.
pub struct Format<'s> {
    fields: Vec<Field<'s>>,
}

/// Parsed key-value pair for a single field in the format.
struct Field<'s> {
    key: Sourced<'s, Vec<Strand<'s>>>,
    val: Sourced<'s, Vec<Strand<'s>>>,
}

/// Some value associated with the source it came from.
struct Sourced<'s, T> {
    src: &'s str,
    val: Result<T, FormatError>,
}

impl<'s> Format<'s> {
    /// Convert the contents of a `Display` object into a `Format` by parsing each of its names and
    /// values as format strings.
    ///
    /// `limits` bound the dimensions (depth, number of output nodes, max number of object loads)
    /// that the parsed format can consume.
    ///
    /// This operation supports partial failures (if one of the format strings is invalid), but
    /// will fail completely if the display overall is detected to exceed the provided `limits`.
    pub fn parse(
        limits: Limits,
        display_fields: &'s VecMap<String, String>,
    ) -> Result<Self, Error> {
        let mut fields = Vec::new();
        let mut budget = limits.budget();
        let mut meter = Meter::new(limits.max_depth, &mut budget);

        let mut parse = |src: &'s str| {
            let val = match Parser::run(src, &mut meter) {
                Err(FormatError::TooBig) => return Err(Error::TooBig),
                Err(FormatError::TooManyLoads) => return Err(Error::TooManyLoads),
                Err(e) => Err(e),
                Ok(ast) => Ok(ast),
            };

            Ok(Sourced { src, val })
        };

        for Entry { key: k, value: v } in &display_fields.contents {
            let key = parse(k)?;
            let val = parse(v)?;
            fields.push(Field { key, val });
        }

        Ok(Self { fields })
    }

    /// Render the object provided as its `bytes` and `layout`, using this Display format, and with
    /// support for dynamically fetching additional objects from `store` as needed.
    ///
    /// This operation requires all field names to evaluate successfully to unique strings, and for
    /// the overall output to be bounded by `max_output_size`, but otherwise supports partial
    /// failures (if one of the field values fails to parse or evaluate).
    pub async fn display<S: Store<'s>>(
        &'s self,
        max_output_size: usize,
        bytes: &'s [u8],
        layout: &'s MoveTypeLayout,
        store: S,
    ) -> Result<IndexMap<String, Result<serde_json::Value, FormatError>>, Error> {
        // Create the interpreter and root slice
        let root = Slice { layout, bytes };
        let interpreter = Arc::new(Interpreter::new(root, store, max_output_size));
        let mut output = IndexMap::new();

        // You think you want to factor a helper out to do the evaluation and error handling, but
        // trust me, you don't.

        let names = try_join_all(self.fields.iter().map(|kvp| {
            let interpreter = interpreter.clone();
            async move {
                let strands = match kvp.key.val.as_ref() {
                    Ok(strands) => strands,
                    Err(e) => return Ok(Err(e.clone())),
                };

                match interpreter.eval(strands).await {
                    Err(FormatError::TooMuchOutput) => Err(Error::TooMuchOutput),
                    other => Ok(other),
                }
            }
        }));

        let values = try_join_all(self.fields.iter().map(|kvp| {
            let interpreter = interpreter.clone();
            async move {
                let strands = match kvp.val.val.as_ref() {
                    Ok(strands) => strands,
                    Err(e) => return Ok(Err(e.clone())),
                };

                match interpreter.eval(strands).await {
                    Err(FormatError::TooMuchOutput) => Err(Error::TooMuchOutput),
                    other => Ok(other),
                }
            }
        }));

        let (names, values) = join!(names, values);
        for ((field, name), value) in self.fields.iter().zip(names?).zip(values?) {
            use indexmap::map::Entry;
            use serde_json::Value as JSON;

            let src = field.key.src;

            let n = match name {
                Ok(JSON::String(n)) => n,
                Ok(JSON::Null) => return Err(Error::NameEmpty(src.to_owned())),
                Ok(_) => return Err(Error::NameInvalid(src.to_owned())),
                Err(e) => return Err(Error::NameError(src.to_owned(), e)),
            };

            match output.entry(n) {
                Entry::Occupied(e) => return Err(Error::NameDuplicate(e.key().to_owned())),
                Entry::Vacant(e) => {
                    e.insert(value);
                }
            }
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    use insta::assert_debug_snapshot;
    use move_core_types::{
        account_address::AccountAddress, annotated_value::MoveTypeLayout as T, u256::U256,
    };
    use sui_types::{
        base_types::{move_ascii_str_layout, move_utf8_str_layout, url_layout},
        id::{ID, UID},
    };

    use crate::v2::value::tests::{MockStore, enum_, struct_, vector_};
    use crate::v2::{error::FormatError, value::tests::optional_};

    const ONE_MB: usize = 1024 * 1024;

    /// Helper to parse display fields and render them against the provided object.
    async fn format<'b, 'l>(
        store: &MockStore,
        limits: Limits,
        bytes: &'b [u8],
        layout: &'l MoveTypeLayout,
        max_output_size: usize,
        fields: impl IntoIterator<Item = (&str, &str)>,
    ) -> Result<IndexMap<String, Result<serde_json::Value, FormatError>>, Error> {
        let display = VecMap {
            contents: fields
                .into_iter()
                .map(|(key, value)| Entry {
                    key: key.to_owned(),
                    value: value.to_owned(),
                })
                .collect(),
        };

        Format::parse(limits, &display)?
            .display(max_output_size, bytes, layout, store)
            .await
    }

    #[tokio::test]
    async fn test_fields_and_scalars() {
        let bytes = bcs::to_bytes(&(
            AccountAddress::from_str("0x4243").unwrap(),
            AccountAddress::from_str("0x4445").unwrap(),
            AccountAddress::from_str("0x4647").unwrap(),
            true,
            48u8,
            49u16,
            50u32,
            51u64,
            52u128,
            U256::from(53u64),
            "hello",
            "world",
            "https://example.com",
        ))
        .unwrap();

        let fields = vec![
            ("addr", T::Address),
            ("id", T::Struct(Box::new(ID::layout()))),
            ("uid", T::Struct(Box::new(UID::layout()))),
            ("flag", T::Bool),
            ("n8", T::U8),
            ("n16", T::U16),
            ("n32", T::U32),
            ("n64", T::U64),
            ("n128", T::U128),
            ("n256", T::U256),
            ("ascii", T::Struct(Box::new(move_ascii_str_layout()))),
            ("utf8", T::Struct(Box::new(move_ascii_str_layout()))),
            ("url", T::Struct(Box::new(url_layout()))),
        ];

        let formats = [
            ("ser_ids", "{addr}, {id}, {uid}"),
            ("ser_bool", "{flag}"),
            ("ser_nums", "{n8}, {n16}, {n32}, {n64}, {n128}, {n256}"),
            ("ser_strs", "{ascii}, {utf8}, {url}"),
            ("ser_bytes", "{ascii.bytes}, {utf8.bytes}, {url.url.bytes}"),
            ("lit_addr", "{@0x5455}"),
            ("lit_bool", "{false}"),
            (
                "lit_nums",
                "{56u8}, {57u16}, {58u32}, {59u64}, {60u128}, {61u256}",
            ),
            ("lit_str", "{'goodbye'}"),
        ];

        let output = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &struct_("0x1::m::S", fields),
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "ser_ids": Ok(
                String("0x0000000000000000000000000000000000000000000000000000000000004243, 0x0000000000000000000000000000000000000000000000000000000000004445, 0x0000000000000000000000000000000000000000000000000000000000004647"),
            ),
            "ser_bool": Ok(
                String("true"),
            ),
            "ser_nums": Ok(
                String("48, 49, 50, 51, 52, 53"),
            ),
            "ser_strs": Ok(
                String("hello, world, https://example.com"),
            ),
            "ser_bytes": Ok(
                String("hello, world, https://example.com"),
            ),
            "lit_addr": Ok(
                String("0x0000000000000000000000000000000000000000000000000000000000005455"),
            ),
            "lit_bool": Ok(
                String("false"),
            ),
            "lit_nums": Ok(
                String("56, 57, 58, 59, 60, 61"),
            ),
            "lit_str": Ok(
                String("goodbye"),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_vector_access() {
        let bytes =
            bcs::to_bytes(&(vec![2u64, 1u64, 0u64], vec!["first", "second", "third"])).unwrap();

        let fields = vec![
            ("ns", vector_(T::U64)),
            ("ss", vector_(T::Struct(Box::new(move_ascii_str_layout())))),
        ];

        let formats = [
            ("ns", "{{{ns[0u8]}, {ns[1u16]}, {ns[2u32]}}}"),
            ("ss", "{{{ss[0u64]}, {ss[1u128]}, {ss[2u256]}}}"),
            ("xs", "{{{ss[ns[0u64]]}, {ss[ns[1u64]]}, {ss[ns[2u64]]}}}"),
        ];

        let output = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &struct_("0x1::m::S", fields),
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "ns": Ok(
                String("{2, 1, 0}"),
            ),
            "ss": Ok(
                String("{first, second, third}"),
            ),
            "xs": Ok(
                String("{third, second, first}"),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_enums() {
        #[derive(serde::Serialize)]
        enum Status<'s> {
            Pending(&'s str),
            Active(u32),
            Done(u128, u64),
        }

        let layout = enum_(
            "0x1::m::Status",
            vec![
                (
                    "Pending",
                    vec![("message", T::Struct(Box::new(move_ascii_str_layout())))],
                ),
                ("Active", vec![("progress", T::U32)]),
                ("Done", vec![("count", T::U128), ("timestamp", T::U64)]),
            ],
        );

        let formats = [
            ("pending", "message = {message}"),
            ("active", "progress = {progress}"),
            ("complete", "count = {count}, timestamp = {timestamp}"),
        ];

        let mut outputs = vec![];

        let pending = bcs::to_bytes(&Status::Pending("waiting")).unwrap();
        outputs.push(
            format(
                &MockStore::default(),
                Limits::default(),
                &pending,
                &layout,
                ONE_MB,
                formats,
            )
            .await
            .unwrap(),
        );

        let active = bcs::to_bytes(&Status::Active(42)).unwrap();
        outputs.push(
            format(
                &MockStore::default(),
                Limits::default(),
                &active,
                &layout,
                ONE_MB,
                formats,
            )
            .await
            .unwrap(),
        );

        let complete = bcs::to_bytes(&Status::Done(100, 999)).unwrap();
        outputs.push(
            format(
                &MockStore::default(),
                Limits::default(),
                &complete,
                &layout,
                ONE_MB,
                formats,
            )
            .await
            .unwrap(),
        );

        assert_debug_snapshot!(outputs, @r###"
        [
            {
                "pending": Ok(
                    String("message = waiting"),
                ),
                "active": Ok(
                    Null,
                ),
                "complete": Ok(
                    Null,
                ),
            },
            {
                "pending": Ok(
                    Null,
                ),
                "active": Ok(
                    String("progress = 42"),
                ),
                "complete": Ok(
                    Null,
                ),
            },
            {
                "pending": Ok(
                    Null,
                ),
                "active": Ok(
                    Null,
                ),
                "complete": Ok(
                    String("count = 100, timestamp = 999"),
                ),
            },
        ]
        "###);
    }

    #[tokio::test]
    async fn test_nested_access() {
        let bytes = bcs::to_bytes(&(
            (42u64, "nested"),
            vec![(1u32, "first"), (2u32, "second")],
            vec![Some((100u64, 200u64, 300u64))],
        ))
        .unwrap();

        let inner = struct_(
            "0x1::m::Inner",
            vec![
                ("value", T::U64),
                ("label", T::Struct(Box::new(move_ascii_str_layout()))),
            ],
        );

        let item = struct_(
            "0x1::m::Item",
            vec![
                ("id", T::U32),
                ("name", T::Struct(Box::new(move_ascii_str_layout()))),
            ],
        );

        let tuple = struct_(
            "0x1::m::Tuple",
            vec![("pos0", T::U64), ("pos1", T::U64), ("pos2", T::U64)],
        );

        let option = enum_(
            "0x1::option::Option",
            vec![("None", vec![]), ("Some", vec![("pos0", tuple)])],
        );

        let fields = vec![
            ("inner", inner),
            ("is", vector_(item)),
            ("ts", vector_(option)),
        ];

        let formats = [
            ("inner", "{inner.value}/{inner.label}"),
            ("items", "{is[0u64].name}, {is[1u64].id}"),
            ("tuples", "({ts[0u64].0.0}, {ts[0u64].0.1}, {ts[0u64].0.2})"),
            ("litpos", "{0x2::m::S(is[1u64]).0.name}"),
            ("litnamed", "{0x2::m::T { id: is[0u64].id }.id}"),
        ];

        let output = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &struct_("0x1::m::S", fields),
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "inner": Ok(
                String("42/nested"),
            ),
            "items": Ok(
                String("first, 2"),
            ),
            "tuples": Ok(
                String("(100, 200, 300)"),
            ),
            "litpos": Ok(
                String("second"),
            ),
            "litnamed": Ok(
                String("1"),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_string_bytes() {
        let bytes = bcs::to_bytes("ABC").unwrap();
        let layout = T::Struct(Box::new(move_ascii_str_layout()));

        let formats = vec![
            ("serialized", "{bytes[0u64]}"),
            ("string_lit", "{'ABC'.bytes[1u64]}"),
            ("bytes_lit", "{b'ABC'[2u64]}"),
        ];

        let output = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &layout,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "serialized": Ok(
                String("65"),
            ),
            "string_lit": Ok(
                String("66"),
            ),
            "bytes_lit": Ok(
                String("67"),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_missing_fields() {
        let bytes = bcs::to_bytes(&(42u64, vec![10u64, 20u64, 30u64])).unwrap();
        let fields = vec![("num", T::U64), ("nums", vector_(T::U64))];

        let formats = [
            // Scalars produce empty responses on any field access
            ("scalar_ok", "{num}"),
            ("scalar_fail", "{num.field}"),
            // Structs produce empty responses on missing field access
            ("field_fail", "{missing}"),
            // Vectors produce empty responses on out-of-bounds access
            ("index_ok", "{nums[1u64]}"),
            ("index_fail", "{numbers[10u64]}"),
            // When accessing multiple fields, all of them must succeed
            ("combined_ok", "{num}, {nums[0u64]}"),
            // If any one fails, the whole field's value is null
            ("combined_fail", "{num}, {missing}, {nums[0u64]}"),
        ];

        let output = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &struct_("0x1::m::S", fields),
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "scalar_ok": Ok(
                String("42"),
            ),
            "scalar_fail": Ok(
                Null,
            ),
            "field_fail": Ok(
                Null,
            ),
            "index_ok": Ok(
                String("20"),
            ),
            "index_fail": Ok(
                Null,
            ),
            "combined_ok": Ok(
                String("42, 10"),
            ),
            "combined_fail": Ok(
                Null,
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_alternates() {
        let bytes = bcs::to_bytes(&42u64).unwrap();
        let layout = struct_("0x1::m::S", vec![("bar", T::U64)]);

        let formats = [
            ("succeeds", "{bar | baz}"),
            ("eventually", "{foo | bar | baz}"),
            ("never", "{foo | baz | qux}"),
            ("fallback", "{foo | 'default'}"),
        ];

        let output = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &layout,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "succeeds": Ok(
                String("42"),
            ),
            "eventually": Ok(
                String("42"),
            ),
            "never": Ok(
                Null,
            ),
            "fallback": Ok(
                String("default"),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_alternate_optional() {
        let bytes = bcs::to_bytes(&(Some(100u64), None::<u64>)).unwrap();
        let layout = struct_(
            "0x1::m::S",
            vec![("a", optional_(T::U64)), ("b", optional_(T::U64))],
        );

        let formats = [("some", "{a | 42u64}"), ("none", "{b | 43u64}")];

        let output = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &layout,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "some": Ok(
                String("100"),
            ),
            "none": Ok(
                String("43"),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_dynamic_fields() {
        let parent = AccountAddress::from_str("0x1000").unwrap();
        let bytes = bcs::to_bytes(&parent).unwrap();
        let layout = struct_(
            "0x1::m::Root",
            vec![(
                "parent",
                struct_(
                    "0x1::m::Parent",
                    vec![("id", T::Struct(Box::new(UID::layout())))],
                ),
            )],
        );

        // Add a dynamic field to the store: parent.df["key"] = 42u64
        let store = MockStore::default().with_dynamic_field(
            parent,
            "key",
            T::Struct(Box::new(move_utf8_str_layout())),
            (42u64, 43u64),
            struct_("0x1::m::Inner", vec![("x", T::U64), ("y", T::U64)]),
        );

        let formats = [
            ("via_obj", "{parent->['key'].x}"),
            ("via_uid", "{parent.id->['key'].y}"),
            ("via_id", "{parent.id.id->['key'].x}"),
            ("via_addr", "{parent.id.id.bytes->['key'].y}"),
            ("via_lit", "{@0x1000->['key'].x}"),
            ("missing", "{parent.id->['missing']}"),
        ];

        let output = format(&store, Limits::default(), &bytes, &layout, ONE_MB, formats)
            .await
            .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "via_obj": Ok(
                String("42"),
            ),
            "via_uid": Ok(
                String("43"),
            ),
            "via_id": Ok(
                String("42"),
            ),
            "via_addr": Ok(
                String("43"),
            ),
            "via_lit": Ok(
                String("42"),
            ),
            "missing": Ok(
                Null,
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_dynamic_object_fields() {
        let parent = AccountAddress::from_str("0x2000").unwrap();
        let child = AccountAddress::from_str("0x2001").unwrap();
        let bytes = bcs::to_bytes(&parent).unwrap();
        let layout = struct_(
            "0x1::m::Root",
            vec![(
                "parent",
                struct_(
                    "0x1::m::Parent",
                    vec![("id", T::Struct(Box::new(UID::layout())))],
                ),
            )],
        );

        let store = MockStore::default().with_dynamic_object_field(
            parent,
            "key",
            T::Struct(Box::new(move_utf8_str_layout())),
            (child, 100u64, 200u64),
            struct_(
                "0x1::m::Child",
                vec![
                    ("id", T::Struct(Box::new(UID::layout()))),
                    ("x", T::U64),
                    ("y", T::U64),
                ],
            ),
        );

        let formats = [
            ("via_obj", "{parent=>['key'].x}"),
            ("via_uid", "{parent.id=>['key'].y}"),
            ("via_id", "{parent.id.id=>['key'].x}"),
            ("via_addr", "{parent.id.id=>['key'].y}"),
            ("via_lit", "{@0x2000=>['key'].x}"),
            ("missing", "{parent.id=>['missing']}"),
        ];

        let limits = Limits {
            max_loads: 20, // Each DOF access counts as 2 loads
            ..Limits::default()
        };

        let output = format(&store, limits, &bytes, &layout, ONE_MB, formats)
            .await
            .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "via_obj": Ok(
                String("100"),
            ),
            "via_uid": Ok(
                String("200"),
            ),
            "via_id": Ok(
                String("100"),
            ),
            "via_addr": Ok(
                String("200"),
            ),
            "via_lit": Ok(
                String("100"),
            ),
            "missing": Ok(
                Null,
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_nested_dynamic_fields() {
        let parent = AccountAddress::from_str("0x3000").unwrap();
        let child = AccountAddress::from_str("0x3001").unwrap();
        let bytes = bcs::to_bytes(&parent).unwrap();
        let layout = struct_(
            "0x1::m::Root",
            vec![(
                "parent",
                struct_(
                    "0x1::m::Parent",
                    vec![("id", T::Struct(Box::new(UID::layout())))],
                ),
            )],
        );

        let store = MockStore::default()
            .with_dynamic_object_field(
                parent,
                "L1",
                T::Struct(Box::new(move_utf8_str_layout())),
                (child, 100u64),
                struct_(
                    "0x1::m::Child",
                    vec![("id", T::Struct(Box::new(UID::layout()))), ("data", T::U64)],
                ),
            )
            .with_dynamic_field(
                child,
                "L2",
                T::Struct(Box::new(move_utf8_str_layout())),
                (10u64, 20u64),
                struct_("0x1::m::Inner", vec![("x", T::U64), ("y", T::U64)]),
            );

        let formats = [
            ("1_data", "{parent=>['L1'].data}"),
            ("1_2_x", "{parent=>['L1']->['L2'].x}"),
            ("1_2_y", "{parent=>['L1']->['L2'].y}"),
        ];

        let limits = Limits {
            max_loads: 20,
            ..Limits::default()
        };

        let output = format(&store, limits, &bytes, &layout, ONE_MB, formats)
            .await
            .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "1_data": Ok(
                String("100"),
            ),
            "1_2_x": Ok(
                String("10"),
            ),
            "1_2_y": Ok(
                String("20"),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_vec_map() {
        let key = struct_(
            "0x42::m::Key",
            vec![
                ("id", T::U64),
                ("name", T::Struct(Box::new(move_ascii_str_layout()))),
            ],
        );

        let val = struct_("0x42::m::Value", vec![("data", T::U32)]);

        let map = struct_(
            "0x2::vec_map::VecMap<0x42::m::Key, 0x42::m::Value>",
            vec![(
                "contents",
                vector_(struct_(
                    "0x2::vec_map::Entry<0x42::m::Key, 0x42::m::Value>",
                    vec![("key", key), ("value", val)],
                )),
            )],
        );

        // Create test data: VecMap with 3 entries
        let bytes = bcs::to_bytes(&VecMap {
            contents: vec![
                Entry {
                    key: (1u64, "first"),
                    value: 100u32,
                },
                Entry {
                    key: (2u64, "second"),
                    value: 200u32,
                },
                Entry {
                    key: (3u64, "third"),
                    value: 300u32,
                },
            ],
        })
        .unwrap();

        let layout = struct_("0x1::m::Root", vec![("map", map)]);

        let formats = [
            ("1st", "{map[0x42::m::Key(1u64, 'first')].data}"),
            ("2nd", "{map[0x42::m::Key(2u64, 'second')].data}"),
            ("3rd", "{map[0x42::m::Key(3u64, 'third')].data}"),
            // Doesn't exist
            ("4th", "{map[0x42::m::Key(4u64, 'fourth')].data}"),
            // Indexing a struct that is not a VecMap
            ("err", "{map[0x42::m::Key(1u64, 'first')].data['empty']}"),
        ];

        let output = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &layout,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "1st": Ok(
                String("100"),
            ),
            "2nd": Ok(
                String("200"),
            ),
            "3rd": Ok(
                String("300"),
            ),
            "4th": Ok(
                Null,
            ),
            "err": Ok(
                Null,
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_timestamp() {
        let bytes = bcs::to_bytes(&1681318800000u64).unwrap();
        let layout = struct_("0x1::m::S", vec![("timestamp", T::U64)]);

        let formats = [
            ("epoch", "{0u64:ts}"),
            ("field", "{timestamp:ts}"),
            ("lit64", "{1683730800000u64:ts}"),
            ("lit128", "{1681318800000u128:ts}"),
            ("toobig", "{1681318800000000000u128:ts}"),
        ];

        let output = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &layout,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "epoch": Ok(
                String("1970-01-01T00:00:00Z"),
            ),
            "field": Ok(
                String("2023-04-12T17:00:00Z"),
            ),
            "lit64": Ok(
                String("2023-05-10T15:00:00Z"),
            ),
            "lit128": Ok(
                String("2023-04-12T17:00:00Z"),
            ),
            "toobig": Err(
                TransformInvalid(
                    "expected unix timestamp in milliseconds",
                ),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_field_errors() {
        let bytes = bcs::to_bytes(&0u8).unwrap();
        let layout = struct_("0x1::m::S", vec![("byte", T::U8)]);

        let formats = [
            ("parsing_error", "{42"),
            ("bad_transform", "{byte:invalid}"),
            ("too_deep", "{a[b[c[d[e[f]]]]]}"),
        ];

        let limits = Limits {
            max_depth: 5,
            ..Limits::default()
        };

        let output = format(
            &MockStore::default(),
            limits,
            &bytes,
            &layout,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "parsing_error": Err(
                UnexpectedEos {
                    expect: ExpectedSet {
                        prev: [],
                        tried: [
                            Literal(
                                "u8",
                            ),
                            Literal(
                                "u16",
                            ),
                            Literal(
                                "u32",
                            ),
                            Literal(
                                "u64",
                            ),
                            Literal(
                                "u128",
                            ),
                            Literal(
                                "u256",
                            ),
                        ],
                    },
                },
            ),
            "bad_transform": Err(
                UnexpectedToken {
                    actual: OwnedLexeme(
                        false,
                        Ident,
                        6,
                        "invalid",
                    ),
                    expect: ExpectedSet {
                        prev: [],
                        tried: [
                            Literal(
                                "str",
                            ),
                            Literal(
                                "ts",
                            ),
                        ],
                    },
                },
            ),
            "too_deep": Err(
                TooDeep,
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_output_node_limits() {
        let bytes = bcs::to_bytes(&42u64).unwrap();

        let limits = Limits {
            max_nodes: 10,
            ..Limits::default()
        };

        // The output node limit is enforced across all fields.
        let big_field = [("f", "{a | b | c | d | e | f | g | h | i | j}")];
        let two_fields = [("f", "{a | b | c | d | e}"), ("g", "{f | g | h | i | j}")];

        let res = format(
            &MockStore::default(),
            limits.clone(),
            &bytes,
            &T::U64,
            ONE_MB,
            big_field,
        )
        .await;
        assert!(matches!(res, Err(Error::TooBig)));

        let res = format(
            &MockStore::default(),
            limits,
            &bytes,
            &T::U64,
            ONE_MB,
            two_fields,
        )
        .await;
        assert!(matches!(res, Err(Error::TooBig)));
    }

    #[tokio::test]
    async fn test_output_size_limits() {
        let bytes = bcs::to_bytes(&42u64).unwrap();
        let formats = [("x", "012345"), ("y", "67890"), ("z", "ABCDE")];

        let res = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &T::U64,
            10,
            formats,
        )
        .await;
        assert!(matches!(res, Err(Error::TooMuchOutput)));
    }

    #[tokio::test]
    async fn test_too_many_loads() {
        let bytes = bcs::to_bytes(&42u64).unwrap();

        let limits = Limits {
            max_loads: 3,
            ..Limits::default()
        };

        // Dynamic field accesses (->[...]) count as 1 load
        // Dynamic object field accesses (=>[...]) count as 2 loads
        let big_field = [("f", "{a->[b]->[c]->[d]->[e]}")];
        let two_fields = [("f1", "{a->[b]}"), ("f2", "{c->[d]}"), ("f3", "{e=>[f]}")];

        let res = format(
            &MockStore::default(),
            limits.clone(),
            &bytes,
            &T::U64,
            ONE_MB,
            big_field,
        )
        .await;
        assert!(matches!(res, Err(Error::TooManyLoads)));

        let res = format(
            &MockStore::default(),
            limits,
            &bytes,
            &T::U64,
            ONE_MB,
            two_fields,
        )
        .await;
        assert!(matches!(res, Err(Error::TooManyLoads)));
    }

    #[tokio::test]
    async fn test_name_empty() {
        let bytes = bcs::to_bytes(&42u64).unwrap();

        // Name evaluates to null when the field doesn't exist
        let formats = [("name {missing}", "value")];
        let res = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &T::U64,
            ONE_MB,
            formats,
        )
        .await;
        assert!(matches!(res, Err(Error::NameEmpty(_))), "{res:?}");
    }

    #[tokio::test]
    async fn test_duplicate_name() {
        let layout = struct_("0x1::m::S", vec![("a", T::U64), ("b", T::U64)]);

        // Static duplicate: same literal name
        let formats = [("field", "value1"), ("field", "value2")];
        let bytes = bcs::to_bytes(&(42u64, 43u64)).unwrap();
        let res = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &layout,
            ONE_MB,
            formats,
        )
        .await;
        assert!(matches!(res, Err(Error::NameDuplicate(_))));

        // Dynamic duplicate: both names evaluate to the same value
        let formats = [("{a}", "value1"), ("{b}", "value2")];
        let bytes = bcs::to_bytes(&(42u64, 42u64)).unwrap();
        let res = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &layout,
            ONE_MB,
            formats,
        )
        .await;
        assert!(matches!(res, Err(Error::NameDuplicate(_))));

        // Mixed case: a dynamic name collides with a static one
        let formats = [("f42", "value1"), ("f{a}", "value2")];
        let bytes = bcs::to_bytes(&(42u64, 43u64)).unwrap();
        let res = format(
            &MockStore::default(),
            Limits::default(),
            &bytes,
            &layout,
            ONE_MB,
            formats,
        )
        .await;
        assert!(matches!(res, Err(Error::NameDuplicate(_))));
    }
}
