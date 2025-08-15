// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Write;

use crate::{
    VARIANT_COUNT_MAX, VARIANT_TAG_MAX_VALUE,
    account_address::AccountAddress,
    runtime_value::{
        MoveEnumLayout, MoveStruct, MoveStructLayout, MoveTypeLayout, MoveValue, MoveVariant,
    },
    runtime_visitor::{
        self, NullTraversal, StructDriver, Traversal, ValueDriver, VariantDriver, VecDriver,
        Visitor,
    },
    u256::U256,
};

#[derive(Default)]
pub(crate) struct CountingTraversal(usize);

#[derive(Default)]
pub(crate) struct PrintVisitor {
    depth: usize,
    pub output: String,
}

impl<'b, 'l> Traversal<'b, 'l> for CountingTraversal {
    type Error = runtime_visitor::Error;

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
    type Error = runtime_visitor::Error;

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

        while let Some((_, value)) = driver.next_field(&mut field_visitor)? {
            fields.push(value);
        }

        self.output = field_visitor.output;
        Ok(MoveValue::Struct(MoveStruct(fields)))
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

        while let Some((_, value)) = driver.next_field(&mut field_visitor)? {
            fields.push(value);
        }

        self.output = field_visitor.output;
        Ok(MoveValue::Variant(MoveVariant {
            tag: driver.tag(),
            fields,
        }))
    }
}

#[test]
fn traversal() {
    use MoveTypeLayout as T;
    use MoveValue as V;

    let type_layout = struct_layout_(vec![
        T::U8,
        T::U16,
        T::U32,
        T::U64,
        T::U128,
        T::U256,
        T::Bool,
        T::Address,
        T::Signer,
        T::Vector(Box::new(T::U8)),
        struct_layout_(vec![T::U8]),
        enum_layout_(vec![vec![T::U8]]),
    ]);

    let T::Struct(struct_layout) = &type_layout else {
        panic!("Not a struct layout");
    };

    let value = struct_value_(vec![
        V::U8(1),
        V::U16(2),
        V::U32(3),
        V::U64(4),
        V::U128(5),
        V::U256(6u32.into()),
        V::Bool(true),
        V::Address(AccountAddress::ZERO),
        V::Signer(AccountAddress::ZERO),
        V::Vector(vec![V::U8(7), V::U8(8), V::U8(9)]),
        struct_value_(vec![V::U8(10)]),
        variant_value_(0, vec![V::U8(11)]),
    ]);

    let bytes = serialize(value);

    let mut value_traversal = CountingTraversal::default();
    MoveValue::visit_deserialize(&bytes, &type_layout, &mut value_traversal).unwrap();

    let mut struct_traversal = CountingTraversal::default();
    MoveStruct::visit_deserialize(&bytes, struct_layout, &mut struct_traversal).unwrap();

    assert_eq!(18, value_traversal.0);
    assert_eq!(18, struct_traversal.0);
}

#[test]
fn max_variant() {
    let variants = (0..VARIANT_COUNT_MAX)
        .map(|_| vec![MoveTypeLayout::U8])
        .collect();
    let layout = enum_layout_(variants);

    let value = variant_value_(VARIANT_TAG_MAX_VALUE as u16, vec![MoveValue::U8(42)]);
    let bytes = serialize(value);
    let mut val_traversal = CountingTraversal::default();
    MoveValue::simple_deserialize(&bytes, &layout).unwrap();
    MoveValue::visit_deserialize(&bytes, &layout, &mut val_traversal).unwrap();
}

#[test]
fn unexpected_eof() {
    use MoveTypeLayout as T;
    use MoveValue as V;

    let type_layout = struct_layout_(vec![T::U64]);
    let value = struct_value_(vec![V::U64(42)]);

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
    let layout = enum_layout_(vec![vec![T::U8]]);
    let value = variant_value_(0, vec![V::U8(42)]);
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
    let layout = enum_layout_(vec![vec![T::U8]]);
    let value = variant_value_(0, vec![V::U8(42)]);
    let mut bytes = serialize(value);

    // Invalid tag value
    bytes[0] = VARIANT_TAG_MAX_VALUE as u8 + 1;

    assert_eq!(
        "invalid variant tag: 127",
        MoveValue::visit_deserialize(&bytes, &layout, &mut NullTraversal)
            .unwrap_err()
            .to_string(),
    );
}

#[test]
fn invalid_variant_tag() {
    use MoveTypeLayout as T;
    use MoveValue as V;
    let layout = enum_layout_(vec![vec![T::U8]]);
    let value = variant_value_(0, vec![V::U8(42)]);
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

    let type_layout = struct_layout_(vec![
        struct_layout_(vec![T::U64, T::Vector(Box::new(T::U32))]),
        enum_layout_(vec![vec![T::U64]]),
    ]);

    let T::Struct(struct_layout) = &type_layout else {
        panic!("Not a struct layout");
    };

    let value = struct_value_(vec![
        struct_value_(vec![
            V::U64(7),
            V::Vector(vec![V::U32(1), V::U32(2), V::U32(3)]),
        ]),
        variant_value_(0, vec![V::U64(4)]),
    ]);

    let bytes = serialize(value.clone());

    let mut value_visitor = PrintVisitor::default();
    let from_value =
        MoveValue::visit_deserialize(&bytes, &type_layout, &mut value_visitor).unwrap();

    let mut struct_visitor = PrintVisitor::default();
    let from_struct =
        MoveStruct::visit_deserialize(&bytes, struct_layout, &mut struct_visitor).unwrap();

    // This is a little strange -- even though we are deserializing a struct, we still get a value.
    // This is because the return type comes from the visitor, not the deserializer.
    assert_eq!(value, from_value);
    assert_eq!(value, from_struct);

    assert_eq!(value_visitor.output, struct_visitor.output);
    insta::assert_snapshot!(value_visitor.output);
}

#[test]
fn peek_field_test() {
    use MoveTypeLayout as T;
    use MoveValue as V;

    struct PeekU64Visitor<'f> {
        fields: &'f [u64],
    }

    impl<'b, 'l> Visitor<'b, 'l> for PeekU64Visitor<'_> {
        type Value = Option<u64>;
        type Error = runtime_visitor::Error;

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

            while driver.peek_field().is_some() {
                if driver.off() == *field {
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

            while driver.peek_field().is_some() {
                if driver.off() == *field {
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

    let type_layout = struct_layout_(vec![
        T::U64,
        T::U32,
        T::Vector(Box::new(T::U64)),
        struct_layout_(vec![T::U64]),
        enum_layout_(vec![vec![T::U64]]),
    ]);

    let T::Struct(struct_layout) = &type_layout else {
        panic!("Not a struct layout");
    };

    let value = struct_value_(vec![
        V::U64(42),
        V::U32(43),
        V::Vector(vec![V::U64(44)]),
        struct_value_(vec![V::U64(45)]),
        variant_value_(0, vec![V::U64(46)]),
    ]);

    let bytes = serialize(value);

    let visit_value = |fields| {
        MoveValue::visit_deserialize(&bytes, &type_layout, &mut PeekU64Visitor { fields }).unwrap()
    };

    let visit_struct = |fields| {
        MoveStruct::visit_deserialize(&bytes, struct_layout, &mut PeekU64Visitor { fields })
            .unwrap()
    };

    assert_eq!(visit_value(&[0]), Some(42));
    assert_eq!(visit_value(&[1]), None);
    assert_eq!(visit_value(&[2]), None);
    assert_eq!(visit_value(&[3, 0]), Some(45));
    assert_eq!(visit_value(&[4, 0]), Some(46));

    assert_eq!(visit_struct(&[0]), Some(42));
    assert_eq!(visit_struct(&[1]), None);
    assert_eq!(visit_struct(&[2]), None);
    assert_eq!(visit_struct(&[3, 0]), Some(45));
    assert_eq!(visit_struct(&[4, 0]), Some(46));
}

#[test]
fn byte_offset_test() {
    use MoveTypeLayout as T;
    use MoveValue as V;

    #[derive(Default)]
    struct ByteOffsetVisitor(String);

    impl<'b, 'l> Traversal<'b, 'l> for ByteOffsetVisitor {
        type Error = runtime_visitor::Error;

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

    let type_layout = struct_layout_(vec![
        struct_layout_(vec![T::U64, T::Vector(Box::new(T::U32))]),
        enum_layout_(vec![vec![T::U64]]),
    ]);

    let T::Struct(struct_layout) = &type_layout else {
        panic!("Not a struct layout");
    };

    let bytes = serialize(struct_value_(vec![
        struct_value_(vec![
            V::U64(7),
            V::Vector(vec![V::U32(1), V::U32(2), V::U32(3)]),
        ]),
        variant_value_(0, vec![V::U64(4)]),
    ]));

    let mut value_visitor = ByteOffsetVisitor::default();
    MoveValue::visit_deserialize(&bytes, &type_layout, &mut value_visitor).unwrap();

    let mut struct_visitor = ByteOffsetVisitor::default();
    MoveStruct::visit_deserialize(&bytes, struct_layout, &mut struct_visitor).unwrap();

    assert_eq!(value_visitor.0, struct_visitor.0);
    insta::assert_snapshot!(value_visitor.0);
}

/// Create a struct value for test purposes.
pub(crate) fn struct_value_(fields: Vec<MoveValue>) -> MoveValue {
    MoveValue::Struct(MoveStruct::new(fields))
}

/// Create a struct layout for test purposes.
pub(crate) fn struct_layout_(fields: Vec<MoveTypeLayout>) -> MoveTypeLayout {
    MoveTypeLayout::Struct(Box::new(MoveStructLayout(Box::new(fields))))
}

/// Create a variant value for test purposes.
pub(crate) fn variant_value_(tag: u16, fields: Vec<MoveValue>) -> MoveValue {
    MoveValue::Variant(MoveVariant { tag, fields })
}

/// Create an enum layout for test purposes.
pub(crate) fn enum_layout_(variants: Vec<Vec<MoveTypeLayout>>) -> MoveTypeLayout {
    MoveTypeLayout::Enum(Box::new(MoveEnumLayout(Box::new(variants))))
}

/// BCS encode Move value.
pub(crate) fn serialize(value: MoveValue) -> Vec<u8> {
    value.simple_serialize().unwrap()
}
