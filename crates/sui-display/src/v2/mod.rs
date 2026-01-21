// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use futures::future::try_join_all;
use futures::join;
use indexmap::IndexMap;

use crate::v2::error::Error;
use crate::v2::meter::Meter;
use crate::v2::parser::Chain;
use crate::v2::parser::Literal;
use crate::v2::parser::Parser;
use crate::v2::parser::Strand;
use crate::v2::writer::Writer;

mod error;
mod interpreter;
mod lexer;
mod meter;
mod parser;
mod peek;
mod value;
mod visitor;
mod writer;

pub use crate::v2::error::FormatError;
pub use crate::v2::interpreter::Interpreter;
pub use crate::v2::meter::Limits;
pub use crate::v2::value::OwnedSlice;
pub use crate::v2::value::Store;
pub use crate::v2::value::Value;

/// A path into a Move value, to extract a sub-slice.
pub struct Extract<'s>(Chain<'s>);

/// A literal value, representing a dynamic field name.
pub struct Name<'s>(Literal<'s>);

/// A parsed format string.
pub struct Format<'s>(Vec<Strand<'s>>);

/// A collection of format strings that are evaluated to a string-to-string mapping.
pub struct Display<'s> {
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

impl<'s> Extract<'s> {
    /// Parse a string as a sequence of nested accessors.
    ///
    /// `limits` bounds the dimensions (depth, number of output nodes, max number of object loads)
    /// that the parsed accessor can consume.
    pub fn parse(limits: Limits, src: &'s str) -> Result<Self, FormatError> {
        let mut budget = limits.budget();
        let mut meter = Meter::new(limits.max_depth, &mut budget);
        let chain = Parser::chain(src, &mut meter)?;

        Ok(Self(chain))
    }

    /// Pull the value located at this extractor's path out of the object provided as its `bytes`
    /// and `layout`, and with support for dynamically fetching additional objects from `store` as
    /// needed.
    ///
    /// It is only valid to extract slices from other slices (not literal values).
    pub async fn extract<S: Store>(
        &'s self,
        interpreter: &'s Interpreter<S>,
    ) -> Result<Option<Value<'s>>, FormatError> {
        interpreter.eval_chain(&self.0).await
    }
}

impl<'s> Name<'s> {
    /// Parse a string as a literal value.
    ///
    /// `limits` bounds the dimensions (depth, number of output nodes, max number of object loads)
    /// that the parsed literal can consume.
    pub fn parse(limits: Limits, src: &'s str) -> Result<Self, FormatError> {
        let mut budget = limits.budget();
        let mut meter = Meter::new(limits.max_depth, &mut budget);
        let literal = Parser::literal(src, &mut meter)?;

        Ok(Self(literal))
    }

    /// Evaluate the literal representing the dynamic field name, returning a `Value` which can be
    /// used to derive a dynamic field or dynamic object field ID.
    pub async fn eval<S: Store>(
        &'s self,
        interpreter: &'s Interpreter<S>,
    ) -> Result<Option<Value<'s>>, FormatError> {
        interpreter.eval_literal(&self.0).await
    }
}

impl<'s> Format<'s> {
    /// Parse a string as a format.
    ///
    /// `limits` bounds the dimensions (depth, number of output nodes, max number of object loads)
    /// that the parsed format string can consume.
    pub fn parse(limits: Limits, src: &'s str) -> Result<Self, FormatError> {
        let mut budget = limits.budget();
        let mut meter = Meter::new(limits.max_depth, &mut budget);
        let format = Parser::format(src, &mut meter)?;

        Ok(Self(format))
    }

    /// Evaluate the format string returning a formatted JSON value.
    pub async fn format<S: Store>(
        &'s self,
        interpreter: &'s Interpreter<S>,
        max_depth: usize,
        max_output_size: usize,
    ) -> Result<serde_json::Value, FormatError> {
        let writer = Writer::new(max_depth, max_output_size);
        let Some(value) = interpreter.eval_strands(&self.0).await? else {
            return Ok(serde_json::Value::Null);
        };

        writer.write(value)
    }
}

impl<'s> Display<'s> {
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
        display_fields: impl IntoIterator<Item = (&'s str, &'s str)>,
    ) -> Result<Self, Error> {
        let mut fields = Vec::new();
        let mut budget = limits.budget();
        let mut meter = Meter::new(limits.max_depth, &mut budget);

        let mut parse = |src: &'s str| {
            let val = match Parser::format(src, &mut meter) {
                Err(FormatError::TooBig) => return Err(Error::TooBig),
                Err(FormatError::TooManyLoads) => return Err(Error::TooManyLoads),
                Err(e) => Err(e),
                Ok(ast) => Ok(ast),
            };

            Ok(Sourced { src, val })
        };

        for (k, v) in display_fields.into_iter() {
            let key = parse(k)?;
            let val = parse(v)?;
            fields.push(Field { key, val });
        }

        Ok(Self { fields })
    }

    /// Render the format with the provided `interpreter`
    ///
    /// This operation requires all field names to evaluate successfully to unique strings, and for
    /// the overall output to be bounded by `max_depth` and `max_output_size`, but otherwise
    /// supports partial failures (if one of the field values fails to parse or evaluate).
    pub async fn display<S: Store>(
        &'s self,
        interpreter: &'s Interpreter<S>,
        max_depth: usize,
        max_output_size: usize,
    ) -> Result<IndexMap<String, Result<serde_json::Value, FormatError>>, Error> {
        let writer = Arc::new(Writer::new(max_depth, max_output_size));
        let mut output = IndexMap::new();

        // You think you want to factor a helper out to do the evaluation and error handling, but
        // trust me, you don't.

        let names = try_join_all(self.fields.iter().map(|kvp| {
            let writer = writer.clone();
            async move {
                let strands = match kvp.key.val.as_ref() {
                    Ok(strands) => strands,
                    Err(e) => return Ok(Err(e.clone())),
                };

                let evaluated = match interpreter.eval_strands(strands).await {
                    Ok(Some(v)) => v,
                    Ok(None) => return Ok(Ok(serde_json::Value::Null)),
                    Err(e) => return Ok(Err(e)),
                };

                match writer.write(evaluated) {
                    Err(FormatError::TooMuchOutput) => Err(Error::TooMuchOutput),
                    other => Ok(other),
                }
            }
        }));

        let values = try_join_all(self.fields.iter().map(|kvp| {
            let writer = writer.clone();
            async move {
                let strands = match kvp.val.val.as_ref() {
                    Ok(strands) => strands,
                    Err(e) => return Ok(Err(e.clone())),
                };

                let evaluated = match interpreter.eval_strands(strands).await {
                    Ok(Some(v)) => v,
                    Ok(None) => return Ok(Ok(serde_json::Value::Null)),
                    Err(e) => return Ok(Err(e)),
                };

                match writer.write(evaluated) {
                    Err(FormatError::TooMuchOutput) => Err(Error::TooMuchOutput),
                    other => Ok(other),
                }
            }
        }));

        let (names, values) = join!(names, values);

        let names = names?;
        debug_assert_eq!(self.fields.len(), names.len());

        let values = values?;
        debug_assert_eq!(self.fields.len(), values.len());

        for ((field, name), value) in self.fields.iter().zip(names).zip(values) {
            use indexmap::map::Entry;
            use serde_json::Value as JSON;

            let src = field.key.src;

            let n = match name {
                Ok(JSON::String(n)) => n,
                Ok(JSON::Null) => return Err(Error::NameEmpty(src.to_owned())),
                Ok(_) => return Err(Error::NameInvalid(src.to_owned())),
                Err(e) => return Err(Error::NameEvaluation(src.to_owned(), e)),
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
    use std::sync::atomic::AtomicUsize;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use insta::assert_debug_snapshot;
    use insta::assert_json_snapshot;
    use move_core_types::account_address::AccountAddress;
    use move_core_types::annotated_value::MoveTypeLayout;
    use move_core_types::annotated_value::MoveTypeLayout as L;
    use move_core_types::language_storage::TypeTag;
    use move_core_types::u256::U256;
    use serde::Serialize;
    use sui_types::base_types::move_ascii_str_layout;
    use sui_types::base_types::move_utf8_str_layout;
    use sui_types::base_types::url_layout;
    use sui_types::dynamic_field::DynamicFieldInfo;
    use sui_types::dynamic_field::derive_dynamic_field_id;
    use sui_types::id::ID;
    use sui_types::id::UID;

    use crate::v2::value::tests::MockStore;
    use crate::v2::value::tests::enum_;
    use crate::v2::value::tests::optional_;
    use crate::v2::value::tests::struct_;
    use crate::v2::value::tests::vec_map;
    use crate::v2::value::tests::vector_;
    use crate::v2::writer::JsonWriter;

    use super::*;

    const ONE_MB: usize = 1024 * 1024;

    /// Helper to parse a path and extract it from the provided object.
    async fn extract(
        store: MockStore,
        bytes: Vec<u8>,
        layout: MoveTypeLayout,
        path: &str,
    ) -> Result<Option<serde_json::Value>, FormatError> {
        let interpreter = Interpreter::new(OwnedSlice { bytes, layout }, store);
        let used = AtomicUsize::new(0);

        let chain = Extract::parse(Limits::default(), path)?;
        let Some(value) = chain.extract(&interpreter).await? else {
            return Ok(None);
        };

        let writer = JsonWriter::new(&used, usize::MAX, usize::MAX);
        Ok(Some(value.format_json(writer)?))
    }

    async fn dynamic_field_id(
        store: MockStore,
        bytes: Vec<u8>,
        layout: MoveTypeLayout,
        parent: AccountAddress,
        literal: &str,
    ) -> Result<Option<AccountAddress>, FormatError> {
        let interpreter = Interpreter::new(OwnedSlice { bytes, layout }, store);

        let name = Name::parse(Limits::default(), literal)?;
        let Some(value) = name.eval(&interpreter).await? else {
            return Ok(None);
        };

        Ok(Some(value.derive_dynamic_field_id(parent)?.into()))
    }

    async fn dynamic_object_field_id(
        store: MockStore,
        bytes: Vec<u8>,
        layout: MoveTypeLayout,
        parent: AccountAddress,
        literal: &str,
    ) -> Result<Option<AccountAddress>, FormatError> {
        let interpreter = Interpreter::new(OwnedSlice { bytes, layout }, store);

        let name = Name::parse(Limits::default(), literal)?;
        let Some(value) = name.eval(&interpreter).await? else {
            return Ok(None);
        };

        Ok(Some(value.derive_dynamic_object_field_id(parent)?.into()))
    }

    /// Helper to parse display fields and render them against the provided object.
    async fn format<'s>(
        store: MockStore,
        limits: Limits,
        bytes: Vec<u8>,
        layout: MoveTypeLayout,
        max_depth: usize,
        max_output_size: usize,
        fields: impl IntoIterator<Item = (&'s str, &'s str)>,
    ) -> Result<IndexMap<String, Result<serde_json::Value, FormatError>>, Error> {
        let interpreter = Interpreter::new(OwnedSlice { bytes, layout }, store);
        Display::parse(limits, fields)?
            .display(&interpreter, max_depth, max_output_size)
            .await
    }

    #[tokio::test]
    async fn test_extract_simple() {
        let bytes = bcs::to_bytes(&(
            AccountAddress::from_str("0x1234").unwrap(),
            None::<bool>,
            Some(true),
            48u8,
            vec![1u64, 2u64, 3u64],
            vec![(4u32, 5u32), (6u32, 7u32), (8u32, 9u32)],
        ))
        .unwrap();

        let layout = struct_(
            "0x1::m::S",
            vec![
                ("addr", L::Address),
                ("none", optional_(L::Bool)),
                ("some", optional_(L::Bool)),
                ("posn", struct_("0x1::m::P", vec![("pos0", L::U8)])),
                ("nums", vector_(L::U64)),
                ("kvps", vec_map(L::U32, L::U32)),
            ],
        );

        let fields = [
            "addr",
            "none",
            "some",
            "posn.0",
            "nums[1u64]",
            "kvps[6u32]",
            "i.dont.exist",
        ];

        let mut outputs = Vec::with_capacity(fields.len());
        for field in fields {
            outputs.push(
                extract(MockStore::default(), bytes.clone(), layout.clone(), field)
                    .await
                    .unwrap(),
            );
        }

        assert_json_snapshot!(outputs, @r###"
        [
          "0x0000000000000000000000000000000000000000000000000000000000001234",
          null,
          true,
          48,
          "2",
          7,
          null
        ]
        "###);
    }

    #[tokio::test]
    async fn test_extract_with_dynamic_loads() {
        let parent = AccountAddress::from_str("0x5000").unwrap();
        let child = AccountAddress::from_str("0x5001").unwrap();
        let bytes = bcs::to_bytes(&parent).unwrap();

        let layout = struct_(
            "0x1::m::Root",
            vec![(
                "parent",
                struct_(
                    "0x1::m::Parent",
                    vec![("id", L::Struct(Box::new(UID::layout())))],
                ),
            )],
        );

        // Add a dynamic field: parent->['df_key'] = (10, 20)
        // Add a dynamic object field: parent=>['dof_key'] = Child { id, x: 100, y: 200 }
        let store = MockStore::default()
            .with_dynamic_field(
                parent,
                "df_key",
                L::Struct(Box::new(move_utf8_str_layout())),
                (10u64, 20u64),
                struct_("0x1::m::Inner", vec![("x", L::U64), ("y", L::U64)]),
            )
            .with_dynamic_object_field(
                parent,
                "dof_key",
                L::Struct(Box::new(move_utf8_str_layout())),
                (child, 100u64, 200u64),
                struct_(
                    "0x1::m::Child",
                    vec![
                        ("id", L::Struct(Box::new(UID::layout()))),
                        ("x", L::U64),
                        ("y", L::U64),
                    ],
                ),
            );

        let fields = [
            // Dynamic field access
            "parent->['df_key'].x",
            "parent->['df_key'].y",
            "parent.id->['df_key'].x",
            // Dynamic object field access
            "parent=>['dof_key'].x",
            "parent=>['dof_key'].y",
            "parent.id=>['dof_key'].id",
            // Missing dynamic field
            "parent->['missing']",
            "parent=>['missing']",
        ];

        let mut outputs = Vec::with_capacity(fields.len());
        for field in fields {
            outputs.push(
                extract(store.clone(), bytes.clone(), layout.clone(), field)
                    .await
                    .unwrap(),
            );
        }

        assert_json_snapshot!(outputs, @r###"
        [
          "10",
          "20",
          "10",
          "100",
          "200",
          "0x0000000000000000000000000000000000000000000000000000000000005001",
          null,
          null
        ]
        "###);
    }

    #[tokio::test]
    async fn test_dynamic_field_names() {
        let parent = AccountAddress::from_str("0x4242").unwrap();

        // Dummy object to interpret against (not used for literal evaluation)
        let obj_bytes = bcs::to_bytes(&0u8).unwrap();
        let obj_layout = L::U8;

        // Test cases: (literal, expected_type_tag, expected_bcs_bytes)
        let cases: Vec<(&str, &str, Vec<u8>)> = vec![
            (
                "'hello'",
                "0x1::string::String",
                bcs::to_bytes(&"hello").unwrap(),
            ),
            ("42u64", "u64", bcs::to_bytes(&42u64).unwrap()),
            ("123u128", "u128", bcs::to_bytes(&123u128).unwrap()),
            (
                "@0xabc",
                "address",
                bcs::to_bytes(&AccountAddress::from_str("0xabc").unwrap()).unwrap(),
            ),
            (
                "0x1::m::Key(99u32, 'test')",
                "0x1::m::Key",
                bcs::to_bytes(&(99u32, "test")).unwrap(),
            ),
            (
                "0x1::m::Key<u32, 0x1::string::String>(99u32, 'test')",
                "0x1::m::Key<u32, 0x1::string::String>",
                bcs::to_bytes(&(99u32, "test")).unwrap(),
            ),
            (
                "vector[1u8, 2u8, 3u8]",
                "vector<u8>",
                bcs::to_bytes(&vec![1u8, 2u8, 3u8]).unwrap(),
            ),
        ];

        for (literal, type_, bytes) in cases {
            let id = dynamic_field_id(
                MockStore::default(),
                obj_bytes.clone(),
                obj_layout.clone(),
                parent,
                literal,
            )
            .await
            .unwrap()
            .unwrap();

            let type_: TypeTag = type_.parse().unwrap();
            let expected = derive_dynamic_field_id(parent, &type_, &bytes).unwrap();
            assert_eq!(id, expected.into(), "mismatch for literal: {literal}");
        }
    }

    #[tokio::test]
    async fn test_dynamic_object_field_names() {
        let parent = AccountAddress::from_str("0x4242").unwrap();

        // Dummy object to interpret against (not used for literal evaluation)
        let obj_bytes = bcs::to_bytes(&0u8).unwrap();
        let obj_layout = L::U8;

        // Test cases: (literal, expected_type_tag, expected_bcs_bytes)
        let cases: Vec<(&str, &str, Vec<u8>)> = vec![
            (
                "'hello'",
                "0x1::string::String",
                bcs::to_bytes(&"hello").unwrap(),
            ),
            ("42u64", "u64", bcs::to_bytes(&42u64).unwrap()),
            ("123u128", "u128", bcs::to_bytes(&123u128).unwrap()),
            (
                "@0xabc",
                "address",
                bcs::to_bytes(&AccountAddress::from_str("0xabc").unwrap()).unwrap(),
            ),
            (
                "0x1::m::Key(99u32, 'test')",
                "0x1::m::Key",
                bcs::to_bytes(&(99u32, "test")).unwrap(),
            ),
            (
                "0x1::m::Key<u32, 0x1::string::String>(99u32, 'test')",
                "0x1::m::Key<u32, 0x1::string::String>",
                bcs::to_bytes(&(99u32, "test")).unwrap(),
            ),
            (
                "vector[1u8, 2u8, 3u8]",
                "vector<u8>",
                bcs::to_bytes(&vec![1u8, 2u8, 3u8]).unwrap(),
            ),
        ];

        for (literal, type_, bytes) in cases {
            let id = dynamic_object_field_id(
                MockStore::default(),
                obj_bytes.clone(),
                obj_layout.clone(),
                parent,
                literal,
            )
            .await
            .unwrap()
            .unwrap();

            let type_: TypeTag = type_.parse().unwrap();
            let wrapper_type = DynamicFieldInfo::dynamic_object_field_wrapper(type_);
            let expected = derive_dynamic_field_id(parent, &wrapper_type.into(), &bytes).unwrap();
            assert_eq!(id, expected.into(), "mismatch for literal: {literal}");
        }
    }

    #[test]
    fn test_dynamic_field_name_parse_errors() {
        let cases = [
            // Empty input
            "",
            // Field access (not a literal)
            "foo",
            "foo.bar",
            // Missing type suffix
            "42",
            // Unclosed string
            "'hello",
            // Unclosed struct
            "0x1::m::S(",
            "0x1::m::S(42u64",
            // Unclosed vector
            "vector[1u8, 2u8",
            // Invalid address
            "@0xGGG",
        ];

        for literal in cases {
            assert!(
                Name::parse(Limits::default(), literal).is_err(),
                "expected error for: {literal:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_format_fields_and_scalars() {
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
            ("addr", L::Address),
            ("id", L::Struct(Box::new(ID::layout()))),
            ("uid", L::Struct(Box::new(UID::layout()))),
            ("flag", L::Bool),
            ("n8", L::U8),
            ("n16", L::U16),
            ("n32", L::U32),
            ("n64", L::U64),
            ("n128", L::U128),
            ("n256", L::U256),
            ("ascii", L::Struct(Box::new(move_ascii_str_layout()))),
            ("utf8", L::Struct(Box::new(move_ascii_str_layout()))),
            ("url", L::Struct(Box::new(url_layout()))),
        ];

        let formats = [
            "{addr}, {id}, {uid}",
            "{flag}",
            "{n8}, {n16}, {n32}, {n64}, {n128}, {n256}",
            "{ascii}, {utf8}, {url}",
            "{ascii.bytes}, {utf8.bytes}, {url.url.bytes}",
            "{@0x5455}",
            "{false}",
            "{56u8}, {57u16}, {58u32}, {59u64}, {60u128}, {61u256}",
            "{'goodbye'}",
        ];

        let store = MockStore::default();
        let root = OwnedSlice {
            layout: struct_("0x1::m::S", fields),
            bytes,
        };

        let mut output = Vec::with_capacity(formats.len());
        let interpreter = Interpreter::new(root, store);
        for s in formats {
            let format = Format::parse(Limits::default(), s).unwrap();
            output.push(
                format
                    .format(&interpreter, usize::MAX, usize::MAX)
                    .await
                    .unwrap(),
            );
        }

        assert_json_snapshot!(output, @r###"
        [
          "0x0000000000000000000000000000000000000000000000000000000000004243, 0x0000000000000000000000000000000000000000000000000000000000004445, 0x0000000000000000000000000000000000000000000000000000000000004647",
          "true",
          "48, 49, 50, 51, 52, 53",
          "hello, world, https://example.com",
          "hello, world, https://example.com",
          "0x0000000000000000000000000000000000000000000000000000000000005455",
          "false",
          "56, 57, 58, 59, 60, 61",
          "goodbye"
        ]
        "###);
    }

    #[tokio::test]
    async fn test_display_fields_and_scalars() {
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
            ("addr", L::Address),
            ("id", L::Struct(Box::new(ID::layout()))),
            ("uid", L::Struct(Box::new(UID::layout()))),
            ("flag", L::Bool),
            ("n8", L::U8),
            ("n16", L::U16),
            ("n32", L::U32),
            ("n64", L::U64),
            ("n128", L::U128),
            ("n256", L::U256),
            ("ascii", L::Struct(Box::new(move_ascii_str_layout()))),
            ("utf8", L::Struct(Box::new(move_ascii_str_layout()))),
            ("url", L::Struct(Box::new(url_layout()))),
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
            MockStore::default(),
            Limits::default(),
            bytes,
            struct_("0x1::m::S", fields),
            usize::MAX,
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
    async fn test_display_vector_access() {
        let bytes =
            bcs::to_bytes(&(vec![2u64, 1u64, 0u64], vec!["first", "second", "third"])).unwrap();

        let fields = vec![
            ("ns", vector_(L::U64)),
            ("ss", vector_(L::Struct(Box::new(move_ascii_str_layout())))),
        ];

        let formats = [
            ("ns", "{{{ns[0u8]}, {ns[1u16]}, {ns[2u32]}}}"),
            ("ss", "{{{ss[0u64]}, {ss[1u128]}, {ss[2u256]}}}"),
            ("xs", "{{{ss[ns[0u64]]}, {ss[ns[1u64]]}, {ss[ns[2u64]]}}}"),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            struct_("0x1::m::S", fields),
            usize::MAX,
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
    async fn test_display_enums() {
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
                    vec![("message", L::Struct(Box::new(move_ascii_str_layout())))],
                ),
                ("Active", vec![("progress", L::U32)]),
                ("Done", vec![("count", L::U128), ("timestamp", L::U64)]),
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
                MockStore::default(),
                Limits::default(),
                pending,
                layout.clone(),
                usize::MAX,
                ONE_MB,
                formats,
            )
            .await
            .unwrap(),
        );

        let active = bcs::to_bytes(&Status::Active(42)).unwrap();
        outputs.push(
            format(
                MockStore::default(),
                Limits::default(),
                active,
                layout.clone(),
                usize::MAX,
                ONE_MB,
                formats,
            )
            .await
            .unwrap(),
        );

        let complete = bcs::to_bytes(&Status::Done(100, 999)).unwrap();
        outputs.push(
            format(
                MockStore::default(),
                Limits::default(),
                complete,
                layout,
                usize::MAX,
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
    async fn test_display_nested_access() {
        let bytes = bcs::to_bytes(&(
            (42u64, "nested"),
            vec![(1u32, "first"), (2u32, "second")],
            vec![Some((100u64, 200u64, 300u64))],
        ))
        .unwrap();

        let inner = struct_(
            "0x1::m::Inner",
            vec![
                ("value", L::U64),
                ("label", L::Struct(Box::new(move_ascii_str_layout()))),
            ],
        );

        let item = struct_(
            "0x1::m::Item",
            vec![
                ("id", L::U32),
                ("name", L::Struct(Box::new(move_ascii_str_layout()))),
            ],
        );

        let tuple = struct_(
            "0x1::m::Tuple",
            vec![("pos0", L::U64), ("pos1", L::U64), ("pos2", L::U64)],
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
            MockStore::default(),
            Limits::default(),
            bytes,
            struct_("0x1::m::S", fields),
            usize::MAX,
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
    async fn test_display_string_bytes() {
        let bytes = bcs::to_bytes("ABC").unwrap();
        let layout = L::Struct(Box::new(move_ascii_str_layout()));

        let formats = vec![
            ("serialized", "{bytes[0u64]}"),
            ("string_lit", "{'ABC'.bytes[1u64]}"),
            ("bytes_lit", "{b'ABC'[2u64]}"),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
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
    async fn test_display_missing_fields() {
        let bytes = bcs::to_bytes(&(42u64, vec![10u64, 20u64, 30u64])).unwrap();
        let fields = vec![("num", L::U64), ("nums", vector_(L::U64))];

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
            MockStore::default(),
            Limits::default(),
            bytes,
            struct_("0x1::m::S", fields),
            usize::MAX,
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
    async fn test_display_alternates() {
        let bytes = bcs::to_bytes(&42u64).unwrap();
        let layout = struct_("0x1::m::S", vec![("bar", L::U64)]);

        let formats = [
            ("succeeds", "{bar | baz}"),
            ("eventually", "{foo | bar | baz}"),
            ("never", "{foo | baz | qux}"),
            ("fallback", "{foo | 'default'}"),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
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
    async fn test_display_alternate_optional() {
        let bytes = bcs::to_bytes(&(Some(100u64), None::<u64>)).unwrap();
        let layout = struct_(
            "0x1::m::S",
            vec![("a", optional_(L::U64)), ("b", optional_(L::U64))],
        );

        let formats = [("some", "{a | 42u64}"), ("none", "{b | 43u64}")];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
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
    async fn test_display_optional_auto_dereference() {
        let inner = struct_(
            "0x1::m::Inner",
            vec![("data", L::U64), ("optional_data", optional_(L::U64))],
        );

        let layout = struct_(
            "0x1::m::Test",
            vec![
                ("some_inner", optional_(inner.clone())),
                ("none_inner", optional_(inner.clone())),
                ("partial_inner", optional_(inner)),
                ("some_value", optional_(L::U64)),
                ("none_value", optional_(L::U64)),
            ],
        );

        let bytes = bcs::to_bytes(&(
            Some((100u64, Some(200u64))), // some_inner
            None::<(u64, Option<u64>)>,   // none_inner
            Some((300u64, None::<u64>)),  // partial_inner
            Some(42u64),                  // some_value
            None::<u64>,                  // none_value
        ))
        .unwrap();

        let formats = [
            // Accessing through Some option to nested field
            ("some_inner_data", "{some_inner.data}"),
            ("some_inner_optional", "{some_inner.optional_data}"),
            // Accessing through None option should return null
            ("none_inner_data", "{none_inner.data}"),
            ("none_inner_optional", "{none_inner.optional_data}"),
            // Accessing through Some option to None nested optional
            ("partial_inner_data", "{partial_inner.data}"),
            ("partial_inner_optional", "{partial_inner.optional_data}"),
            // Direct optional access
            ("some_value", "{some_value}"),
            ("none_value", "{none_value}"),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "some_inner_data": Ok(
                String("100"),
            ),
            "some_inner_optional": Ok(
                String("200"),
            ),
            "none_inner_data": Ok(
                Null,
            ),
            "none_inner_optional": Ok(
                Null,
            ),
            "partial_inner_data": Ok(
                String("300"),
            ),
            "partial_inner_optional": Ok(
                Null,
            ),
            "some_value": Ok(
                String("42"),
            ),
            "none_value": Ok(
                Null,
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_display_dynamic_fields() {
        let parent = AccountAddress::from_str("0x1000").unwrap();
        let bytes = bcs::to_bytes(&parent).unwrap();
        let layout = struct_(
            "0x1::m::Root",
            vec![(
                "parent",
                struct_(
                    "0x1::m::Parent",
                    vec![("id", L::Struct(Box::new(UID::layout())))],
                ),
            )],
        );

        // Add a dynamic field to the store: parent.df["key"] = 42u64
        let store = MockStore::default().with_dynamic_field(
            parent,
            "key",
            L::Struct(Box::new(move_utf8_str_layout())),
            (42u64, 43u64),
            struct_("0x1::m::Inner", vec![("x", L::U64), ("y", L::U64)]),
        );

        let formats = [
            ("via_obj", "{parent->['key'].x}"),
            ("via_uid", "{parent.id->['key'].y}"),
            ("via_id", "{parent.id.id->['key'].x}"),
            ("via_addr", "{parent.id.id.bytes->['key'].y}"),
            ("via_lit", "{@0x1000->['key'].x}"),
            ("missing", "{parent.id->['missing']}"),
        ];

        let output = format(
            store,
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
            ONE_MB,
            formats,
        )
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
    async fn test_display_dynamic_object_fields() {
        let parent = AccountAddress::from_str("0x2000").unwrap();
        let child = AccountAddress::from_str("0x2001").unwrap();
        let bytes = bcs::to_bytes(&parent).unwrap();
        let layout = struct_(
            "0x1::m::Root",
            vec![(
                "parent",
                struct_(
                    "0x1::m::Parent",
                    vec![("id", L::Struct(Box::new(UID::layout())))],
                ),
            )],
        );

        let store = MockStore::default().with_dynamic_object_field(
            parent,
            "key",
            L::Struct(Box::new(move_utf8_str_layout())),
            (child, 100u64, 200u64),
            struct_(
                "0x1::m::Child",
                vec![
                    ("id", L::Struct(Box::new(UID::layout()))),
                    ("x", L::U64),
                    ("y", L::U64),
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

        let output = format(store, limits, bytes, layout, usize::MAX, ONE_MB, formats)
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
    async fn test_display_nested_dynamic_fields() {
        let parent = AccountAddress::from_str("0x3000").unwrap();
        let child = AccountAddress::from_str("0x3001").unwrap();
        let bytes = bcs::to_bytes(&parent).unwrap();
        let layout = struct_(
            "0x1::m::Root",
            vec![(
                "parent",
                struct_(
                    "0x1::m::Parent",
                    vec![("id", L::Struct(Box::new(UID::layout())))],
                ),
            )],
        );

        let store = MockStore::default()
            .with_dynamic_object_field(
                parent,
                "L1",
                L::Struct(Box::new(move_utf8_str_layout())),
                (child, 100u64),
                struct_(
                    "0x1::m::Child",
                    vec![("id", L::Struct(Box::new(UID::layout()))), ("data", L::U64)],
                ),
            )
            .with_dynamic_field(
                child,
                "L2",
                L::Struct(Box::new(move_utf8_str_layout())),
                (10u64, 20u64),
                struct_("0x1::m::Inner", vec![("x", L::U64), ("y", L::U64)]),
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

        let output = format(store, limits, bytes, layout, usize::MAX, ONE_MB, formats)
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
    async fn test_display_vec_map() {
        let key = struct_(
            "0x42::m::Key",
            vec![
                ("id", L::U64),
                ("name", L::Struct(Box::new(move_ascii_str_layout()))),
            ],
        );

        let val = struct_("0x42::m::Value", vec![("data", L::U32)]);

        // Create test data: VecMap with 3 entries
        let bytes = bcs::to_bytes(&vec![
            (1u64, "first", 100u32),
            (2u64, "second", 200u32),
            (3u64, "third", 300u32),
        ])
        .unwrap();

        let layout = struct_("0x1::m::Root", vec![("map", vec_map(key, val))]);

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
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
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
    async fn test_display_timestamp() {
        let bytes = bcs::to_bytes(&1681318800000u64).unwrap();
        let layout = struct_("0x1::m::S", vec![("timestamp", L::U64)]);

        let formats = [
            ("epoch", "{0u64:ts}"),
            ("field", "{timestamp:ts}"),
            ("lit64", "{1683730800000u64:ts}"),
            ("lit128", "{1681318800000u128:ts}"),
            ("toobig", "{1681318800000000000u128:ts}"),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
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
                TransformInvalid_ {
                    offset: 0,
                    reason: "expected unix timestamp in milliseconds",
                },
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_display_hex() {
        let bytes = bcs::to_bytes(&(
            0x42u8,
            0x4243u16,
            0x42434445u32,
            0x4243444546474849u64,
            0x42434445464748494a4b4c4d4e4f5051u128,
            U256::from_str_radix(
                "42434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f6061",
                16,
            )
            .unwrap(),
            AccountAddress::from_str(
                "0x41403f3e3d3c3b3a393837363534333231300f0e0d0c0b0a0908070605040302",
            )
            .unwrap(),
            vec![0x41u8, 0x40, 0x3a],
            "ABC",
        ))
        .unwrap();

        let layout = struct_(
            "0x1::m::S",
            vec![
                ("n8", L::U8),
                ("n16", L::U16),
                ("n32", L::U32),
                ("n64", L::U64),
                ("n128", L::U128),
                ("n256", L::U256),
                ("addr", L::Address),
                ("bytes", vector_(L::U8)),
                ("str", L::Struct(Box::new(move_ascii_str_layout()))),
            ],
        );

        let formats = [
            ("n8", "{n8:hex}"),
            ("n16", "{n16:hex}"),
            ("n32", "{n32:hex}"),
            ("n64", "{n64:hex}"),
            ("n128", "{n128:hex}"),
            ("n256", "{n256:hex}"),
            ("addr", "{addr:hex}"),
            ("bytes", "{bytes:hex}"),
            ("str", "{str:hex}"),
            ("str_bytes", "{str.bytes:hex}"),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "n8": Ok(
                String("42"),
            ),
            "n16": Ok(
                String("4243"),
            ),
            "n32": Ok(
                String("42434445"),
            ),
            "n64": Ok(
                String("4243444546474849"),
            ),
            "n128": Ok(
                String("42434445464748494a4b4c4d4e4f5051"),
            ),
            "n256": Ok(
                String("42434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f6061"),
            ),
            "addr": Ok(
                String("41403f3e3d3c3b3a393837363534333231300f0e0d0c0b0a0908070605040302"),
            ),
            "bytes": Ok(
                String("41403a"),
            ),
            "str": Ok(
                String("414243"),
            ),
            "str_bytes": Ok(
                String("414243"),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_display_url() {
        let bytes = bcs::to_bytes(&(
            1234u32,
            "hello/goodbye world",
            "🔥",
            vec![0x3eu8, 0x3f, 0x40, 0x41, 0x42, 0x43],
        ))
        .unwrap();

        let layout = struct_(
            "0x1::m::S",
            vec![
                ("num", L::U32),
                ("str", L::Struct(Box::new(move_ascii_str_layout()))),
                ("emoji", L::Struct(Box::new(move_utf8_str_layout()))),
                ("bytes", L::Struct(Box::new(url_layout()))),
            ],
        );

        let formats = [(
            "url",
            "https://example.com/?num={num:url}&str={str:url}&emoji={emoji:url}&data={bytes:url}",
        )];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "url": Ok(
                String("https://example.com/?num=1234&str=hello%2Fgoodbye%20world&emoji=%F0%9F%94%A5&data=%3E%3F%40ABC"),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_display_base64() {
        let bytes = bcs::to_bytes(&00u8).unwrap();
        let layout = struct_("0x1::m::S", vec![("dummy_field", L::Bool)]);

        let formats = [
            ("byte", "{0u8:base64}"),
            ("byte_nopad", "{0u8:base64(nopad)}"),
            ("byte_url", "{0u8:base64(url)}"),
            ("byte_url_nopad", "{0u8:base64(url, nopad)}"),
            ("long", "{0xf8fbu64:base64}"),
            ("long_nopad", "{0xf8fbu64:base64(nopad)}"),
            ("long_url", "{0xf8fbu64:base64(url)}"),
            ("long_url_nopad", "{0xf8fbu64:base64(nopad, url)}"),
            ("str", "{'hello':base64}"),
            ("str_nopad", "{'hello':base64(nopad)}"),
            ("str_url", "{'hello':base64(url)}"),
            ("str_url_nopad", "{'hello':base64(url, nopad)}"),
            (
                "flatland",
                "{43920588204278303214855528440570972873796977361529388163322669436471087583698u256:base64(url)}",
            ),
            (
                "flatland_nopad",
                "{43920588204278303214855528440570972873796977361529388163322669436471087583698u256:base64(nopad)}",
            ),
            (
                "flatland_url",
                "{43920588204278303214855528440570972873796977361529388163322669436471087583698u256:base64(url)}",
            ),
            (
                "flatland_url_nopad",
                "{43920588204278303214855528440570972873796977361529388163322669436471087583698u256:base64(url, nopad)}",
            ),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "byte": Ok(
                String("AA=="),
            ),
            "byte_nopad": Ok(
                String("AA"),
            ),
            "byte_url": Ok(
                String("AA=="),
            ),
            "byte_url_nopad": Ok(
                String("AA"),
            ),
            "long": Ok(
                String("+/gAAAAAAAA="),
            ),
            "long_nopad": Ok(
                String("+/gAAAAAAAA"),
            ),
            "long_url": Ok(
                String("-_gAAAAAAAA="),
            ),
            "long_url_nopad": Ok(
                String("-_gAAAAAAAA"),
            ),
            "str": Ok(
                String("aGVsbG8="),
            ),
            "str_nopad": Ok(
                String("aGVsbG8"),
            ),
            "str_url": Ok(
                String("aGVsbG8="),
            ),
            "str_url_nopad": Ok(
                String("aGVsbG8"),
            ),
            "flatland": Ok(
                String("0tGFaqPKhfWCrycZHVcT6lgF7C-YIrMMzORXFwcsGmE="),
            ),
            "flatland_nopad": Ok(
                String("0tGFaqPKhfWCrycZHVcT6lgF7C+YIrMMzORXFwcsGmE"),
            ),
            "flatland_url": Ok(
                String("0tGFaqPKhfWCrycZHVcT6lgF7C-YIrMMzORXFwcsGmE="),
            ),
            "flatland_url_nopad": Ok(
                String("0tGFaqPKhfWCrycZHVcT6lgF7C-YIrMMzORXFwcsGmE"),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_display_bcs() {
        let bytes = bcs::to_bytes(&(
            0x42u8,
            0x1234u16,
            0x12345678u32,
            0x123456789abcdef0u64,
            "hello",
            vec![1u8, 2, 3],
        ))
        .unwrap();

        let layout = struct_(
            "0x1::m::S",
            vec![
                ("n8", L::U8),
                ("n16", L::U16),
                ("n32", L::U32),
                ("n64", L::U64),
                ("str", L::Struct(Box::new(move_utf8_str_layout()))),
                ("bytes", vector_(L::U8)),
            ],
        );

        let formats = [
            ("s8", "{n8:bcs}"),
            ("l8", "{0x43u8:bcs}"),
            ("s16", "{n16:bcs}"),
            ("l16", "{0x1235u16:bcs}"),
            ("s32", "{n32:bcs}"),
            ("l32", "{0x12345679u32:bcs}"),
            ("s64", "{n64:bcs}"),
            ("l64", "{0x123456789abcdef1u64:bcs}"),
            ("sstr", "{str:bcs}"),
            ("lstr", "{'goodbye':bcs}"),
            ("sbytes", "{bytes:bcs}"),
            ("lbytes", "{x'010204':bcs}"),
            ("hbytes", "{vector[0x41u8, n8, 0x43u8]:bcs}"),
            ("lstruct", "{0x1::m::S(n8, n16):bcs}"),
            ("lempty", "{0x1::m::Empty():bcs}"),
            ("lnone", "{0x1::option::Option<u8>::None#0():bcs}"),
            ("lsome", "{0x1::option::Option<u8>::Some#1(0x44u8):bcs}"),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        let actual = |f: &str| output.get(f).unwrap().as_ref().unwrap().as_str().unwrap();
        fn expect(x: impl Serialize) -> String {
            STANDARD.encode(bcs::to_bytes(&x).unwrap())
        }

        assert_eq!(actual("s8"), expect(0x42u8));
        assert_eq!(actual("l8"), expect(0x43u8));
        assert_eq!(actual("s16"), expect(0x1234u16));
        assert_eq!(actual("l16"), expect(0x1235u16));
        assert_eq!(actual("s32"), expect(0x12345678u32));
        assert_eq!(actual("l32"), expect(0x12345679u32));
        assert_eq!(actual("s64"), expect(0x123456789abcdef0u64));
        assert_eq!(actual("l64"), expect(0x123456789abcdef1u64));
        assert_eq!(actual("sstr"), expect("hello"));
        assert_eq!(actual("lstr"), expect("goodbye"));
        assert_eq!(actual("sbytes"), expect(vec![1u8, 2, 3]));
        assert_eq!(actual("lbytes"), expect(vec![1u8, 2, 4]));
        assert_eq!(actual("hbytes"), expect(vec![0x41u8, 0x42, 0x43]));
        assert_eq!(actual("lstruct"), expect((0x42u8, 0x1234u16)));
        assert_eq!(actual("lempty"), expect(false));
        assert_eq!(actual("lnone"), expect(None::<u8>));
        assert_eq!(actual("lsome"), expect(Some(0x44u8)));
    }

    #[tokio::test]
    async fn test_display_bcs_modifiers() {
        let bytes = bcs::to_bytes(&00u8).unwrap();
        let layout = struct_("0x1::m::S", vec![("dummy_field", L::Bool)]);

        let formats = [
            ("byte", "{0u8:bcs}"),
            ("byte_nopad", "{0u8:bcs(nopad)}"),
            ("byte_url", "{0u8:bcs(url)}"),
            ("byte_url_nopad", "{0u8:bcs(url, nopad)}"),
            ("long", "{0xf8fbu64:bcs}"),
            ("long_nopad", "{0xf8fbu64:bcs(nopad)}"),
            ("long_url", "{0xf8fbu64:bcs(url)}"),
            ("long_url_nopad", "{0xf8fbu64:bcs(nopad, url)}"),
            ("str", "{'hello':bcs}"),
            ("str_nopad", "{'hello':bcs(nopad)}"),
            ("str_url", "{'hello':bcs(url)}"),
            ("str_url_nopad", "{'hello':bcs(url, nopad)}"),
            (
                "flatland",
                "{43920588204278303214855528440570972873796977361529388163322669436471087583698u256:bcs(url)}",
            ),
            (
                "flatland_nopad",
                "{43920588204278303214855528440570972873796977361529388163322669436471087583698u256:bcs(nopad)}",
            ),
            (
                "flatland_url",
                "{43920588204278303214855528440570972873796977361529388163322669436471087583698u256:bcs(url)}",
            ),
            (
                "flatland_url_nopad",
                "{43920588204278303214855528440570972873796977361529388163322669436471087583698u256:bcs(url, nopad)}",
            ),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "byte": Ok(
                String("AA=="),
            ),
            "byte_nopad": Ok(
                String("AA"),
            ),
            "byte_url": Ok(
                String("AA=="),
            ),
            "byte_url_nopad": Ok(
                String("AA"),
            ),
            "long": Ok(
                String("+/gAAAAAAAA="),
            ),
            "long_nopad": Ok(
                String("+/gAAAAAAAA"),
            ),
            "long_url": Ok(
                String("-_gAAAAAAAA="),
            ),
            "long_url_nopad": Ok(
                String("-_gAAAAAAAA"),
            ),
            "str": Ok(
                String("BWhlbGxv"),
            ),
            "str_nopad": Ok(
                String("BWhlbGxv"),
            ),
            "str_url": Ok(
                String("BWhlbGxv"),
            ),
            "str_url_nopad": Ok(
                String("BWhlbGxv"),
            ),
            "flatland": Ok(
                String("0tGFaqPKhfWCrycZHVcT6lgF7C-YIrMMzORXFwcsGmE="),
            ),
            "flatland_nopad": Ok(
                String("0tGFaqPKhfWCrycZHVcT6lgF7C+YIrMMzORXFwcsGmE"),
            ),
            "flatland_url": Ok(
                String("0tGFaqPKhfWCrycZHVcT6lgF7C-YIrMMzORXFwcsGmE="),
            ),
            "flatland_url_nopad": Ok(
                String("0tGFaqPKhfWCrycZHVcT6lgF7C-YIrMMzORXFwcsGmE"),
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_display_json() {
        let bytes = bcs::to_bytes(&(
            12u8,
            1234u16,
            12345678u32,
            123456781234567890u64,
            "hello",
            vec![1u8, 2, 3],
            None::<u8>,
            Some(vec![4u32, 5u32, 6u32]),
            (1u8, 5678u16),
            (9u32, 10u64),
        ))
        .unwrap();

        let layout = struct_(
            "0x1::m::S",
            vec![
                ("n8", L::U8),
                ("n16", L::U16),
                ("n32", L::U32),
                ("n64", L::U64),
                ("str", L::Struct(Box::new(move_utf8_str_layout()))),
                ("bytes", vector_(L::U8)),
                (
                    "none",
                    struct_("0x1::option::Option<u8>", vec![("vec", vector_(L::U8))]),
                ),
                (
                    "some",
                    struct_(
                        "0x1::option::Option<vector<u32>>",
                        vec![("vec", vector_(vector_(L::U32)))],
                    ),
                ),
                (
                    "variant",
                    enum_(
                        "0x1::m::E",
                        vec![("A", vec![("x", L::U8)]), ("B", vec![("y", L::U16)])],
                    ),
                ),
                (
                    "nested",
                    struct_("0x1::m::N", vec![("a", L::U32), ("b", L::U64)]),
                ),
            ],
        );

        let formats = [
            ("s8", "{n8:json}"),
            ("l8", "{34u8:json}"),
            ("s16", "{n16:json}"),
            ("l16", "{5678u16:json}"),
            ("s32", "{n32:json}"),
            ("l32", "{87654321u32:json}"),
            ("s64", "{n64:json}"),
            ("l64", "{9876543210987654321u64:json}"),
            ("sstr", "{str:json}"),
            ("lstr", "{'goodbye':json}"),
            ("sbytes", "{bytes:json}"),
            ("lbytes", "{x'040506':json}"),
            ("vbytes", "{vector[0x01u8, 0x02u8, 0x03u8]:json}"),
            ("snone", "{none:json}"),
            ("ssome", "{some:json}"),
            ("lvec", "{vector[7u64, 8u64, 9u64]:json}"),
            ("svariant", "{variant:json}"),
            ("lvariant", "{0x1::m::E::A#0(90u8):json}"),
            ("snested", "{nested:json}"),
            ("lstruct", "{0x1::m::S { c: n8, d: n16 }:json}"),
            ("lempty", "{0x1::m::Empty():json}"),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "s8": Ok(
                Number(12),
            ),
            "l8": Ok(
                Number(34),
            ),
            "s16": Ok(
                Number(1234),
            ),
            "l16": Ok(
                Number(5678),
            ),
            "s32": Ok(
                Number(12345678),
            ),
            "l32": Ok(
                Number(87654321),
            ),
            "s64": Ok(
                String("123456781234567890"),
            ),
            "l64": Ok(
                String("9876543210987654321"),
            ),
            "sstr": Ok(
                String("hello"),
            ),
            "lstr": Ok(
                String("goodbye"),
            ),
            "sbytes": Ok(
                String("AQID"),
            ),
            "lbytes": Ok(
                String("BAUG"),
            ),
            "vbytes": Ok(
                Array [
                    Number(1),
                    Number(2),
                    Number(3),
                ],
            ),
            "snone": Ok(
                Null,
            ),
            "ssome": Ok(
                Array [
                    Number(4),
                    Number(5),
                    Number(6),
                ],
            ),
            "lvec": Ok(
                Array [
                    String("7"),
                    String("8"),
                    String("9"),
                ],
            ),
            "svariant": Ok(
                Object {
                    "@variant": String("B"),
                    "y": Number(5678),
                },
            ),
            "lvariant": Ok(
                Object {
                    "@variant": String("A"),
                    "pos0": Number(90),
                },
            ),
            "snested": Ok(
                Object {
                    "a": Number(9),
                    "b": String("10"),
                },
            ),
            "lstruct": Ok(
                Object {
                    "c": Number(12),
                    "d": Number(1234),
                },
            ),
            "lempty": Ok(
                Object {},
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_display_string_hardening() {
        let bytes = bcs::to_bytes(&("ascii", "🔥", vec![0xC3u8])).unwrap();
        let layout = struct_(
            "0x1::m::S",
            vec![
                ("ascii", L::Struct(Box::new(move_utf8_str_layout()))),
                ("utf8", L::Struct(Box::new(move_utf8_str_layout()))),
                ("invalid", L::Struct(Box::new(move_utf8_str_layout()))),
            ],
        );

        let formats = [
            ("ascii", "{ascii}"),
            ("utf8", "{utf8}"),
            ("invalid", "{invalid}"),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "ascii": Ok(
                String("ascii"),
            ),
            "utf8": Ok(
                String("🔥"),
            ),
            "invalid": Err(
                TransformInvalid_ {
                    offset: 0,
                    reason: "expected utf8 bytes",
                },
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_display_field_errors() {
        let bytes = bcs::to_bytes(&0u8).unwrap();
        let layout = struct_("0x1::m::S", vec![("byte", L::U8)]);

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
            MockStore::default(),
            limits,
            bytes,
            layout,
            usize::MAX,
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
                                "base64",
                            ),
                            Literal(
                                "bcs",
                            ),
                            Literal(
                                "hex",
                            ),
                            Literal(
                                "json",
                            ),
                            Literal(
                                "str",
                            ),
                            Literal(
                                "ts",
                            ),
                            Literal(
                                "url",
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
    async fn test_display_vector_literal_type_mismatch() {
        let bytes = bcs::to_bytes(&0u8).unwrap();
        let layout = struct_("0x1::m::S", vec![("byte", L::U8)]);

        let formats = [
            ("between_literals", "{vector[42u8, 42u64]:bcs}"),
            ("between_field_and_literal", "{vector[42u64, byte]:bcs}"),
            ("between_annotation_and_element", "{vector<u64>[byte]:bcs}"),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout,
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "between_literals": Err(
                VectorTypeMismatch {
                    offset: 1,
                    this: U8,
                    that: U64,
                },
            ),
            "between_field_and_literal": Err(
                VectorTypeMismatch {
                    offset: 1,
                    this: U64,
                    that: U8,
                },
            ),
            "between_annotation_and_element": Err(
                VectorTypeMismatch {
                    offset: 1,
                    this: U64,
                    that: U8,
                },
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_display_output_node_limits() {
        let bytes = bcs::to_bytes(&42u64).unwrap();

        let limits = Limits {
            max_nodes: 10,
            ..Limits::default()
        };

        // The output node limit is enforced across all fields.
        let big_field = [("f", "{a | b | c | d | e | f | g | h | i | j}")];
        let two_fields = [("f", "{a | b | c | d | e}"), ("g", "{f | g | h | i | j}")];

        let res = format(
            MockStore::default(),
            limits.clone(),
            bytes.clone(),
            L::U64,
            usize::MAX,
            ONE_MB,
            big_field,
        )
        .await;
        assert!(matches!(res, Err(Error::TooBig)));

        let res = format(
            MockStore::default(),
            limits,
            bytes,
            L::U64,
            usize::MAX,
            ONE_MB,
            two_fields,
        )
        .await;
        assert!(matches!(res, Err(Error::TooBig)));
    }

    #[tokio::test]
    async fn test_display_output_size_limits() {
        let bytes = bcs::to_bytes(&42u64).unwrap();
        let formats = [("x", "012345"), ("y", "67890"), ("z", "ABCDE")];

        let res = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            L::U64,
            usize::MAX,
            10,
            formats,
        )
        .await;
        assert!(matches!(res, Err(Error::TooMuchOutput)));
    }

    #[tokio::test]
    async fn test_display_move_value_depth_limit() {
        let bytes = bcs::to_bytes(&42u64).unwrap();

        let formats = [
            ("leaf", "{42u64:json}"),
            ("shallow", "{0x1::m::S(43u128):json}"),
            (
                "deep",
                "{0x1::m::S(vector[vector[0x1::m::T(44u256)]]):json}",
            ),
        ];

        let output = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            L::U64,
            3,
            ONE_MB,
            formats,
        )
        .await
        .unwrap();

        assert_debug_snapshot!(output, @r###"
        {
            "leaf": Ok(
                String("42"),
            ),
            "shallow": Ok(
                Object {
                    "pos0": String("43"),
                },
            ),
            "deep": Err(
                TooDeep,
            ),
        }
        "###);
    }

    #[tokio::test]
    async fn test_display_too_many_loads() {
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
            MockStore::default(),
            limits.clone(),
            bytes.clone(),
            L::U64,
            usize::MAX,
            ONE_MB,
            big_field,
        )
        .await;
        assert!(matches!(res, Err(Error::TooManyLoads)));

        let res = format(
            MockStore::default(),
            limits,
            bytes,
            L::U64,
            usize::MAX,
            ONE_MB,
            two_fields,
        )
        .await;
        assert!(matches!(res, Err(Error::TooManyLoads)));
    }

    #[tokio::test]
    async fn test_display_name_empty() {
        let bytes = bcs::to_bytes(&42u64).unwrap();

        // Name evaluates to null when the field doesn't exist
        let formats = [("name {missing}", "value")];
        let res = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            L::U64,
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await;
        assert!(matches!(res, Err(Error::NameEmpty(_))), "{res:?}");
    }

    #[tokio::test]
    async fn test_display_duplicate_name() {
        let layout = struct_("0x1::m::S", vec![("a", L::U64), ("b", L::U64)]);

        // Static duplicate: same literal name
        let formats = [("field", "value1"), ("field", "value2")];
        let bytes = bcs::to_bytes(&(42u64, 43u64)).unwrap();
        let res = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout.clone(),
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await;
        assert!(matches!(res, Err(Error::NameDuplicate(_))));

        // Dynamic duplicate: both names evaluate to the same value
        let formats = [("{a}", "value1"), ("{b}", "value2")];
        let bytes = bcs::to_bytes(&(42u64, 42u64)).unwrap();
        let res = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout.clone(),
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await;
        assert!(matches!(res, Err(Error::NameDuplicate(_))));

        // Mixed case: a dynamic name collides with a static one
        let formats = [("f42", "value1"), ("f{a}", "value2")];
        let bytes = bcs::to_bytes(&(42u64, 43u64)).unwrap();
        let res = format(
            MockStore::default(),
            Limits::default(),
            bytes,
            layout.clone(),
            usize::MAX,
            ONE_MB,
            formats,
        )
        .await;
        assert!(matches!(res, Err(Error::NameDuplicate(_))));
    }
}
