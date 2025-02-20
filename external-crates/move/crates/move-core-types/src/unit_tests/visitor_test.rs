// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fmt::Write, str::FromStr};

use crate::{
    account_address::AccountAddress,
    annotated_value::{
        MoveEnumLayout, MoveFieldLayout, MoveStruct, MoveStructLayout, MoveTypeLayout, MoveValue,
        MoveVariant,
    },
    annotated_visitor::{
        self, NullTraversal, StructDriver, Traversal, ValueDriver, VariantDriver, VecDriver,
        Visitor,
    },
    identifier::Identifier,
    language_storage::StructTag,
    u256::U256,
    VARIANT_COUNT_MAX,
};

#[derive(Default)]
pub(crate) struct CountingTraversal(usize);

#[derive(Default)]
pub(crate) struct PrintVisitor {
    depth: usize,
    pub output: String,
}

impl<'b, 'l> Traversal<'b, 'l> for CountingTraversal {
    type Error = annotated_visitor::Error;

    fn traverse_u8(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: u8,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        Ok(())
    }

    fn traverse_u16(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: u16,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        Ok(())
    }

    fn traverse_u32(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: u32,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        Ok(())
    }

    fn traverse_u64(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: u64,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        Ok(())
    }

    fn traverse_u128(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: u128,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        Ok(())
    }

    fn traverse_u256(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: U256,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        Ok(())
    }

    fn traverse_bool(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: bool,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        Ok(())
    }

    fn traverse_address(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: AccountAddress,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        Ok(())
    }

    fn traverse_signer(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: AccountAddress,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        Ok(())
    }

    fn traverse_vector(&mut self, driver: &mut VecDriver<'_, 'b, 'l>) -> Result<(), Self::Error> {
        self.0 += 1;
        while driver.next_element(self)?.is_some() {}
        Ok(())
    }

    fn traverse_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        while driver.next_field(self)?.is_some() {}
        Ok(())
    }

    fn traverse_variant(
        &mut self,
        driver: &mut VariantDriver<'_, 'b, 'l>,
    ) -> Result<(), Self::Error> {
        self.0 += 1;
        while driver.next_field(self)?.is_some() {}
        Ok(())
    }
}

impl<'b, 'l> Visitor<'b, 'l> for PrintVisitor {
    type Value = MoveValue;
    type Error = annotated_visitor::Error;

    fn visit_u8(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u8,
    ) -> Result<Self::Value, Self::Error> {
        write!(self.output, "\n[{}] {value}: u8", self.depth).unwrap();
        Ok(MoveValue::U8(value))
    }

    fn visit_u16(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u16,
    ) -> Result<Self::Value, Self::Error> {
        write!(self.output, "\n[{}] {value}: u16", self.depth).unwrap();
        Ok(MoveValue::U16(value))
    }

    fn visit_u32(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u32,
    ) -> Result<Self::Value, Self::Error> {
        write!(self.output, "\n[{}] {value}: u32", self.depth).unwrap();
        Ok(MoveValue::U32(value))
    }

    fn visit_u64(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u64,
    ) -> Result<Self::Value, Self::Error> {
        write!(self.output, "\n[{}] {value}: u64", self.depth).unwrap();
        Ok(MoveValue::U64(value))
    }

    fn visit_u128(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u128,
    ) -> Result<Self::Value, Self::Error> {
        write!(self.output, "\n[{}] {value}: u128", self.depth).unwrap();
        Ok(MoveValue::U128(value))
    }

    fn visit_u256(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: U256,
    ) -> Result<Self::Value, Self::Error> {
        write!(self.output, "\n[{}] {value}: u256", self.depth).unwrap();
        Ok(MoveValue::U256(value))
    }

    fn visit_bool(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: bool,
    ) -> Result<Self::Value, Self::Error> {
        write!(self.output, "\n[{}] {value}: bool", self.depth).unwrap();
        Ok(MoveValue::Bool(value))
    }

    fn visit_address(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        write!(self.output, "\n[{}] {value}: address", self.depth).unwrap();
        Ok(MoveValue::Address(value))
    }

    fn visit_signer(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        write!(self.output, "\n[{}] {value}: signer", self.depth).unwrap();
        Ok(MoveValue::Signer(value))
    }

    fn visit_vector(
        &mut self,
        driver: &mut VecDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let layout = driver.element_layout();
        write!(self.output, "\n[{}] vector<{layout:#}>", self.depth).unwrap();

        let mut elems = vec![];
        let mut elem_visitor = Self {
            depth: self.depth + 1,
            output: std::mem::take(&mut self.output),
        };

        while let Some(elem) = driver.next_element(&mut elem_visitor)? {
            elems.push(elem)
        }

        self.output = elem_visitor.output;
        Ok(MoveValue::Vector(elems))
    }

    fn visit_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let layout = driver.struct_layout();
        write!(self.output, "\n[{}] {layout:#}", self.depth).unwrap();

        let mut fields = vec![];
        let mut field_visitor = Self {
            depth: self.depth + 1,
            output: std::mem::take(&mut self.output),
        };

        while let Some((field, value)) = driver.next_field(&mut field_visitor)? {
            fields.push((field.name.clone(), value));
        }

        self.output = field_visitor.output;
        let type_ = driver.struct_layout().type_.clone();
        Ok(MoveValue::Struct(MoveStruct { type_, fields }))
    }

    fn visit_variant(
        &mut self,
        driver: &mut VariantDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let layout = driver.enum_layout();
        write!(self.output, "\n[{}] {layout:#}", self.depth).unwrap();

        let mut fields = vec![];
        let mut field_visitor = Self {
            depth: self.depth + 1,
            output: std::mem::take(&mut self.output),
        };

        while let Some((field, value)) = driver.next_field(&mut field_visitor)? {
            fields.push((field.name.clone(), value));
        }

        self.output = field_visitor.output;
        let type_ = driver.enum_layout().type_.clone();
        Ok(MoveValue::Variant(MoveVariant {
            type_,
            variant_name: driver.variant_name().to_owned(),
            tag: driver.tag(),
            fields,
        }))
    }
}

#[test]
fn traversal() {
    use MoveTypeLayout as T;
    use MoveValue as V;

    let type_layout = struct_layout_(
        "0x0::foo::Bar",
        vec![
            ("a", T::U8),
            ("b", T::U16),
            ("c", T::U32),
            ("d", T::U64),
            ("e", T::U128),
            ("f", T::U256),
            ("g", T::Bool),
            ("h", T::Address),
            ("i", T::Signer),
            ("j", T::Vector(Box::new(T::U8))),
            ("k", struct_layout_("0x0::foo::Baz", vec![("l", T::U8)])),
            (
                "m",
                enum_layout_("0x0::foo::Qux", vec![("n", vec![("o", T::U8)])]),
            ),
        ],
    );

    let T::Struct(struct_layout) = &type_layout else {
        panic!("Not a struct layout");
    };

    let value = struct_value_(
        "0x0::foo::Bar",
        vec![
            ("a", V::U8(1)),
            ("b", V::U16(2)),
            ("c", V::U32(3)),
            ("d", V::U64(4)),
            ("e", V::U128(5)),
            ("f", V::U256(6u32.into())),
            ("g", V::Bool(true)),
            ("h", V::Address(AccountAddress::ZERO)),
            ("i", V::Signer(AccountAddress::ZERO)),
            ("j", V::Vector(vec![V::U8(7), V::U8(8), V::U8(9)])),
            ("k", struct_value_("0x0::foo::Baz", vec![("l", V::U8(10))])),
            (
                "m",
                variant_value_("0x0::foo::Qux", "n", 0, vec![("o", V::U8(11))]),
            ),
        ],
    );

    let bytes = serialize(value);

    let mut value_traversal = CountingTraversal::default();
    MoveValue::visit_deserialize(&bytes, &type_layout, &mut value_traversal).unwrap();

    let mut struct_traversal = CountingTraversal::default();
    MoveStruct::visit_deserialize(&bytes, struct_layout, &mut struct_traversal).unwrap();

    assert_eq!(18, value_traversal.0);
    assert_eq!(18, struct_traversal.0);
}

#[test]
fn unexpected_eof() {
    use MoveTypeLayout as T;
    use MoveValue as V;

    let type_layout = struct_layout_("0x0::foo::Bar", vec![("a", T::U64)]);
    let value = struct_value_("0x0::foo::Bar", vec![("a", V::U64(42))]);

    let T::Struct(struct_layout) = &type_layout else {
        panic!("Not a struct layout");
    };

    let mut bytes = serialize(value);

    // Oops, dropped a byte
    bytes.pop();

    assert_eq!(
        "unexpected end of input",
        MoveValue::visit_deserialize(&bytes, &type_layout, &mut NullTraversal)
            .unwrap_err()
            .to_string(),
    );

    assert_eq!(
        "unexpected end of input",
        MoveStruct::visit_deserialize(&bytes, struct_layout, &mut NullTraversal)
            .unwrap_err()
            .to_string(),
    );
}

#[test]
fn no_enum_tag() {
    use MoveTypeLayout as T;
    use MoveValue as V;
    let layout = enum_layout_("0x0::foo::Bar", vec![("a", vec![("b", T::U8)])]);
    let value = variant_value_("0x0::foo::Bar", "a", 0, vec![("b", V::U8(42))]);
    let mut bytes = serialize(value);

    // drop tag
    bytes.remove(0);

    assert_eq!(
        "invalid variant tag: 42",
        MoveValue::visit_deserialize(&bytes, &layout, &mut NullTraversal)
            .unwrap_err()
            .to_string(),
    );
}

#[test]
fn out_of_range_enum_tag() {
    use MoveTypeLayout as T;
    use MoveValue as V;
    let layout = enum_layout_("0x0::foo::Bar", vec![("a", vec![("b", T::U8)])]);
    let value = variant_value_("0x0::foo::Bar", "a", 0, vec![("b", V::U8(42))]);
    let mut bytes = serialize(value);

    // Invalid tag value
    bytes[0] = VARIANT_COUNT_MAX as u8 + 1;

    assert_eq!(
        "invalid variant tag: 128",
        MoveValue::visit_deserialize(&bytes, &layout, &mut NullTraversal)
            .unwrap_err()
            .to_string(),
    );
}

#[test]
fn invalid_variant_tag() {
    use MoveTypeLayout as T;
    use MoveValue as V;
    let layout = enum_layout_("0x0::foo::Bar", vec![("a", vec![("b", T::U8)])]);
    let value = variant_value_("0x0::foo::Bar", "a", 0, vec![("b", V::U8(42))]);
    let mut bytes = serialize(value);

    // tag for variant that doesn't exist
    bytes[0] = 1;

    assert_eq!(
        "invalid variant tag: 1",
        MoveValue::visit_deserialize(&bytes, &layout, &mut NullTraversal)
            .unwrap_err()
            .to_string(),
    );
}

#[test]
fn bad_bool_byte() {
    use MoveTypeLayout as T;
    assert_eq!(
        "unexpected byte: 42",
        MoveValue::visit_deserialize(&[42], &T::Bool, &mut NullTraversal)
            .unwrap_err()
            .to_string(),
    );
}

#[test]
fn bad_vector_length() {
    use MoveTypeLayout as T;
    use MoveValue as V;

    let layout = T::Vector(Box::new(T::U8));
    let value = V::Vector(vec![V::U8(1), V::U8(2), V::U8(3)]);

    let mut bytes = serialize(value);

    // Oops, dropped a byte
    bytes.pop();

    assert_eq!(
        "unexpected end of input",
        MoveValue::visit_deserialize(&bytes, &layout, &mut NullTraversal)
            .unwrap_err()
            .to_string(),
    );
}

#[test]
fn trailing_bytes() {
    use MoveTypeLayout as T;
    assert_eq!(
        "trailing 1 byte(s) at the end of input",
        MoveValue::visit_deserialize(&[42, 42], &T::U8, &mut NullTraversal)
            .unwrap_err()
            .to_string(),
    );
}

#[test]
fn nested_datatype_visit() {
    use MoveTypeLayout as T;
    use MoveValue as V;

    let type_layout = struct_layout_(
        "0x0::foo::Bar",
        vec![
            (
                "inner",
                struct_layout_(
                    "0x0::baz::Qux",
                    vec![("f", T::U64), ("g", T::Vector(Box::new(T::U32)))],
                ),
            ),
            (
                "last",
                enum_layout_("0x0::foo::Baz", vec![("e", vec![("h", T::U64)])]),
            ),
        ],
    );

    let T::Struct(struct_layout) = &type_layout else {
        panic!("Not a struct layout");
    };

    let value = struct_value_(
        "0x0::foo::Bar",
        vec![
            (
                "inner",
                struct_value_(
                    "0x0::baz::Qux",
                    vec![
                        ("f", V::U64(7)),
                        ("g", V::Vector(vec![V::U32(1), V::U32(2), V::U32(3)])),
                    ],
                ),
            ),
            (
                "last",
                variant_value_("0x0::foo::Baz", "e", 0, vec![("h", V::U64(4))]),
            ),
        ],
    );

    let bytes = serialize(value.clone());

    let mut value_visitor = PrintVisitor::default();
    let from_value =
        MoveValue::visit_deserialize(&bytes, &type_layout, &mut value_visitor).unwrap();

    let mut struct_visitor = PrintVisitor::default();
    let from_struct =
        MoveStruct::visit_deserialize(&bytes, struct_layout, &mut struct_visitor).unwrap();

    let expected_output = r#"
[0] struct 0x0::foo::Bar {
    inner: struct 0x0::baz::Qux {
        f: u64,
        g: vector<u32>,
    },
    last: enum 0x0::foo::Baz {
        e {
            h: u64,
        },
    },
}
[1] struct 0x0::baz::Qux {
    f: u64,
    g: vector<u32>,
}
[2] 7: u64
[2] vector<u32>
[3] 1: u32
[3] 2: u32
[3] 3: u32
[1] enum 0x0::foo::Baz {
    e {
        h: u64,
    },
}
[2] 4: u64"#;

    // This is a little strange -- even though we are deserializing a struct, we still get a value.
    // This is because the return type comes from the visitor, not the deserializer.
    assert_eq!(value, from_value);
    assert_eq!(value, from_struct);

    assert_eq!(value_visitor.output, expected_output);
    assert_eq!(struct_visitor.output, expected_output);
}

#[test]
fn peek_field_test() {
    use MoveTypeLayout as T;
    use MoveValue as V;

    struct PeekU64Visitor<'f> {
        fields: &'f [&'f str],
    }

    impl<'b, 'l, 'f> Visitor<'b, 'l> for PeekU64Visitor<'f> {
        type Value = Option<u64>;
        type Error = annotated_visitor::Error;

        fn visit_u64(
            &mut self,
            _driver: &ValueDriver<'_, 'b, 'l>,
            value: u64,
        ) -> Result<Self::Value, Self::Error> {
            Ok(self.fields.is_empty().then_some(value))
        }

        fn visit_struct(
            &mut self,
            driver: &mut StructDriver<'_, 'b, 'l>,
        ) -> Result<Self::Value, Self::Error> {
            let [field, fields @ ..] = self.fields else {
                return Ok(None);
            };

            while let Some(layout) = driver.peek_field() {
                if layout.name.as_str() == *field {
                    return driver
                        .next_field(&mut Self { fields })
                        .map(|value| value.and_then(|(_, v)| v));
                } else {
                    driver.skip_field()?;
                }
            }

            Ok(None)
        }

        fn visit_variant(
            &mut self,
            driver: &mut VariantDriver<'_, 'b, 'l>,
        ) -> Result<Self::Value, Self::Error> {
            let [field, fields @ ..] = self.fields else {
                return Ok(None);
            };

            while let Some(layout) = driver.peek_field() {
                if layout.name.as_str() == *field {
                    return driver
                        .next_field(&mut Self { fields })
                        .map(|value| value.and_then(|(_, v)| v));
                } else {
                    driver.skip_field()?;
                }
            }

            Ok(None)
        }

        // === Empty/default cases ===

        fn visit_u8(
            &mut self,
            _: &ValueDriver<'_, 'b, 'l>,
            _: u8,
        ) -> Result<Self::Value, Self::Error> {
            Ok(None)
        }

        fn visit_u16(
            &mut self,
            _: &ValueDriver<'_, 'b, 'l>,
            _: u16,
        ) -> Result<Self::Value, Self::Error> {
            Ok(None)
        }

        fn visit_u32(
            &mut self,
            _: &ValueDriver<'_, 'b, 'l>,
            _: u32,
        ) -> Result<Self::Value, Self::Error> {
            Ok(None)
        }

        fn visit_u128(
            &mut self,
            _: &ValueDriver<'_, 'b, 'l>,
            _: u128,
        ) -> Result<Self::Value, Self::Error> {
            Ok(None)
        }

        fn visit_u256(
            &mut self,
            _: &ValueDriver<'_, 'b, 'l>,
            _: U256,
        ) -> Result<Self::Value, Self::Error> {
            Ok(None)
        }

        fn visit_bool(
            &mut self,
            _: &ValueDriver<'_, 'b, 'l>,
            _: bool,
        ) -> Result<Self::Value, Self::Error> {
            Ok(None)
        }

        fn visit_address(
            &mut self,
            _: &ValueDriver<'_, 'b, 'l>,
            _: AccountAddress,
        ) -> Result<Self::Value, Self::Error> {
            Ok(None)
        }

        fn visit_signer(
            &mut self,
            _: &ValueDriver<'_, 'b, 'l>,
            _: AccountAddress,
        ) -> Result<Self::Value, Self::Error> {
            Ok(None)
        }

        /// Field specifier doesn't support vectors, so we know we won't find the field we want
        /// under here.
        fn visit_vector(
            &mut self,
            _: &mut VecDriver<'_, 'b, 'l>,
        ) -> Result<Self::Value, Self::Error> {
            Ok(None)
        }
    }

    let type_layout = struct_layout_(
        "0x0::foo::Bar",
        vec![
            ("a", T::U64),
            ("b", T::U32),
            ("c", T::Vector(Box::new(T::U64))),
            ("d", struct_layout_("0x0::foo::Baz", vec![("e", T::U64)])),
            (
                "f",
                enum_layout_("0x0::foo::Qux", vec![("g", vec![("h", T::U64)])]),
            ),
        ],
    );

    let T::Struct(struct_layout) = &type_layout else {
        panic!("Not a struct layout");
    };

    let value = struct_value_(
        "0x0::foo::Bar",
        vec![
            ("a", V::U64(42)),
            ("b", V::U32(43)),
            ("c", V::Vector(vec![V::U64(44)])),
            ("d", struct_value_("0x0::foo::Baz", vec![("e", V::U64(45))])),
            (
                "f",
                variant_value_("0x0::foo::Qux", "g", 0, vec![("h", V::U64(46))]),
            ),
        ],
    );

    let bytes = serialize(value);

    let visit_value = |fields| {
        MoveValue::visit_deserialize(&bytes, &type_layout, &mut PeekU64Visitor { fields }).unwrap()
    };

    let visit_struct = |fields| {
        MoveStruct::visit_deserialize(&bytes, struct_layout, &mut PeekU64Visitor { fields })
            .unwrap()
    };

    assert_eq!(visit_value(&["a"]), Some(42));
    assert_eq!(visit_value(&["b"]), None);
    assert_eq!(visit_value(&["c"]), None);
    assert_eq!(visit_value(&["d", "e"]), Some(45));
    assert_eq!(visit_value(&["f", "h"]), Some(46));

    assert_eq!(visit_struct(&["a"]), Some(42));
    assert_eq!(visit_struct(&["b"]), None);
    assert_eq!(visit_struct(&["c"]), None);
    assert_eq!(visit_struct(&["d", "e"]), Some(45));
    assert_eq!(visit_struct(&["f", "h"]), Some(46));
}

#[test]
fn byte_offset_test() {
    use MoveTypeLayout as T;
    use MoveValue as V;

    #[derive(Default)]
    struct ByteOffsetVisitor(String);

    impl<'b, 'l> Traversal<'b, 'l> for ByteOffsetVisitor {
        type Error = annotated_visitor::Error;

        fn traverse_u8(
            &mut self,
            driver: &ValueDriver<'_, 'b, 'l>,
            value: u8,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] {value}: u8",
                driver.start(),
                driver.position()
            )
            .unwrap();
            Ok(())
        }

        fn traverse_u16(
            &mut self,
            driver: &ValueDriver<'_, 'b, 'l>,
            value: u16,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] {value}: u16",
                driver.start(),
                driver.position()
            )
            .unwrap();
            Ok(())
        }

        fn traverse_u32(
            &mut self,
            driver: &ValueDriver<'_, 'b, 'l>,
            value: u32,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] {value}: u32",
                driver.start(),
                driver.position()
            )
            .unwrap();
            Ok(())
        }

        fn traverse_u64(
            &mut self,
            driver: &ValueDriver<'_, 'b, 'l>,
            value: u64,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] {value}: u64",
                driver.start(),
                driver.position()
            )
            .unwrap();
            Ok(())
        }

        fn traverse_u128(
            &mut self,
            driver: &ValueDriver<'_, 'b, 'l>,
            value: u128,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] {value}: u128",
                driver.start(),
                driver.position()
            )
            .unwrap();
            Ok(())
        }

        fn traverse_u256(
            &mut self,
            driver: &ValueDriver<'_, 'b, 'l>,
            value: U256,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] {value}: u256",
                driver.start(),
                driver.position()
            )
            .unwrap();
            Ok(())
        }

        fn traverse_bool(
            &mut self,
            driver: &ValueDriver<'_, 'b, 'l>,
            value: bool,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] {value}: bool",
                driver.start(),
                driver.position()
            )
            .unwrap();
            Ok(())
        }

        fn traverse_address(
            &mut self,
            driver: &ValueDriver<'_, 'b, 'l>,
            value: AccountAddress,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] {}: address",
                driver.start(),
                driver.position(),
                value.to_canonical_display(/* with_prefix */ true),
            )
            .unwrap();
            Ok(())
        }

        fn traverse_signer(
            &mut self,
            driver: &ValueDriver<'_, 'b, 'l>,
            value: AccountAddress,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] {}: address",
                driver.start(),
                driver.position(),
                value.to_canonical_display(/* with_prefix */ true),
            )
            .unwrap();
            Ok(())
        }

        fn traverse_vector(
            &mut self,
            driver: &mut VecDriver<'_, 'b, 'l>,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] vector<{:#}>",
                driver.start(),
                driver.position(),
                driver.element_layout(),
            )
            .unwrap();
            while driver.next_element(self)?.is_some() {}
            Ok(())
        }

        fn traverse_struct(
            &mut self,
            driver: &mut StructDriver<'_, 'b, 'l>,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] {:#}",
                driver.start(),
                driver.position(),
                driver.struct_layout(),
            )
            .unwrap();

            while let Some((_, ())) = driver.next_field(self)? {}
            Ok(())
        }

        fn traverse_variant(
            &mut self,
            driver: &mut VariantDriver<'_, 'b, 'l>,
        ) -> Result<(), Self::Error> {
            write!(
                &mut self.0,
                "\n[{:>3} .. {:>3}] {:#}",
                driver.start(),
                driver.position(),
                driver.enum_layout(),
            )
            .unwrap();

            while let Some((_, ())) = driver.next_field(self)? {}
            Ok(())
        }
    }

    let type_layout = struct_layout_(
        "0x0::foo::Bar",
        vec![
            (
                "inner",
                struct_layout_(
                    "0x0::baz::Qux",
                    vec![("f", T::U64), ("g", T::Vector(Box::new(T::U32)))],
                ),
            ),
            (
                "last",
                enum_layout_("0x0::foo::Baz", vec![("e", vec![("h", T::U64)])]),
            ),
        ],
    );

    let T::Struct(struct_layout) = &type_layout else {
        panic!("Not a struct layout");
    };

    let bytes = serialize(struct_value_(
        "0x0::foo::Bar",
        vec![
            (
                "inner",
                struct_value_(
                    "0x0::baz::Qux",
                    vec![
                        ("f", V::U64(7)),
                        ("g", V::Vector(vec![V::U32(1), V::U32(2), V::U32(3)])),
                    ],
                ),
            ),
            (
                "last",
                variant_value_("0x0::foo::Baz", "e", 0, vec![("h", V::U64(4))]),
            ),
        ],
    ));

    let mut value_visitor = ByteOffsetVisitor::default();
    MoveValue::visit_deserialize(&bytes, &type_layout, &mut value_visitor).unwrap();

    let mut struct_visitor = ByteOffsetVisitor::default();
    MoveStruct::visit_deserialize(&bytes, struct_layout, &mut struct_visitor).unwrap();

    let expected_output = r#"
[  0 ..   0] struct 0x0::foo::Bar {
    inner: struct 0x0::baz::Qux {
        f: u64,
        g: vector<u32>,
    },
    last: enum 0x0::foo::Baz {
        e {
            h: u64,
        },
    },
}
[  0 ..   0] struct 0x0::baz::Qux {
    f: u64,
    g: vector<u32>,
}
[  0 ..   8] 7: u64
[  8 ..   9] vector<u32>
[  9 ..  13] 1: u32
[ 13 ..  17] 2: u32
[ 17 ..  21] 3: u32
[ 21 ..  22] enum 0x0::foo::Baz {
    e {
        h: u64,
    },
}
[ 22 ..  30] 4: u64"#;

    assert_eq!(value_visitor.0, expected_output);
    assert_eq!(struct_visitor.0, expected_output);
}

/// Create a struct value for test purposes.
pub(crate) fn struct_value_(rep: &str, fields: Vec<(&str, MoveValue)>) -> MoveValue {
    let type_ = StructTag::from_str(rep).unwrap();
    let fields = fields
        .into_iter()
        .map(|(name, value)| (Identifier::new(name).unwrap(), value))
        .collect();

    MoveValue::Struct(MoveStruct::new(type_, fields))
}

/// Create a struct layout for test purposes.
pub(crate) fn struct_layout_(rep: &str, fields: Vec<(&str, MoveTypeLayout)>) -> MoveTypeLayout {
    let type_ = StructTag::from_str(rep).unwrap();
    let fields = fields
        .into_iter()
        .map(|(name, layout)| MoveFieldLayout::new(Identifier::new(name).unwrap(), layout))
        .collect();

    MoveTypeLayout::Struct(Box::new(MoveStructLayout { type_, fields }))
}

/// Create a variant value for test purposes.
pub(crate) fn variant_value_(
    rep: &str,
    name: &str,
    tag: u16,
    fields: Vec<(&str, MoveValue)>,
) -> MoveValue {
    let type_ = StructTag::from_str(rep).unwrap();
    let fields = fields
        .into_iter()
        .map(|(name, value)| (Identifier::new(name).unwrap(), value))
        .collect();

    MoveValue::Variant(MoveVariant {
        type_,
        variant_name: Identifier::new(name).unwrap(),
        tag,
        fields,
    })
}

/// Create an enum layout for test purposes.
pub(crate) fn enum_layout_(
    rep: &str,
    variants: Vec<(&str, Vec<(&str, MoveTypeLayout)>)>,
) -> MoveTypeLayout {
    let type_ = StructTag::from_str(rep).unwrap();
    let variants = variants
        .into_iter()
        .enumerate()
        .map(|(t, (name, fields))| {
            let fields = fields
                .into_iter()
                .map(|(name, layout)| MoveFieldLayout::new(Identifier::new(name).unwrap(), layout))
                .collect();
            ((Identifier::new(name).unwrap(), t as u16), fields)
        })
        .collect();

    MoveTypeLayout::Enum(Box::new(MoveEnumLayout { type_, variants }))
}

/// BCS encode Move value.
pub(crate) fn serialize(value: MoveValue) -> Vec<u8> {
    value.clone().undecorate().simple_serialize().unwrap()
}
