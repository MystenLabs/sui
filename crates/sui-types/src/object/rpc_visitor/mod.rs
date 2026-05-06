// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod format;
pub mod json;
mod meter;
pub mod proto;

pub use format::Format;
pub use meter::LocalMeter;
pub use meter::Meter;
pub use meter::MeterError;
pub use meter::Unmetered;

use std::marker::PhantomData;

use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_visitor as AV;
use move_core_types::language_storage::TypeTag;
use move_core_types::u256::U256;

use crate::balance::Balance;
use crate::base_types::RESOLVED_STD_OPTION;
use crate::base_types::move_ascii_str_layout;
use crate::base_types::move_utf8_str_layout;
use crate::base_types::type_name_layout;
use crate::base_types::url_layout;
use crate::id::ID;
use crate::id::UID;
use crate::object::option_visitor as OV;

/// A visitor that serializes Move values into some representation appropriate for RPC outputs.
///
/// The `F: Format` type parameter determines the output format and charging semantics, while the
/// `M: Meter` type parameter determines how budgets are tracked.
pub struct RpcVisitor<F: Format, M: Meter> {
    meter: M,
    phantom: PhantomData<fn() -> F>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Visitor(#[from] AV::Error),

    #[error(transparent)]
    Option(#[from] OV::Error),

    #[error(transparent)]
    Meter(#[from] MeterError),

    #[error("Unexpected type")]
    UnexpectedType,
}

impl<F: Format, M: Meter> RpcVisitor<F, M> {
    /// Create a new RPC visitor that writes into the chosen format with the given meter.
    pub fn new(meter: M) -> Self {
        Self {
            meter,
            phantom: PhantomData,
        }
    }
}

impl<'b, 'l, F: Format, M: Meter> AV::Visitor<'b, 'l> for RpcVisitor<F, M> {
    type Value = F;
    type Error = Error;

    fn visit_u8(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: u8,
    ) -> Result<Self::Value, Self::Error> {
        Ok(F::number(&mut self.meter, value as u32)?)
    }

    fn visit_u16(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: u16,
    ) -> Result<Self::Value, Self::Error> {
        Ok(F::number(&mut self.meter, value as u32)?)
    }

    fn visit_u32(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: u32,
    ) -> Result<Self::Value, Self::Error> {
        Ok(F::number(&mut self.meter, value)?)
    }

    fn visit_u64(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: u64,
    ) -> Result<Self::Value, Self::Error> {
        Ok(F::string(&mut self.meter, value.to_string())?)
    }

    fn visit_u128(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: u128,
    ) -> Result<Self::Value, Self::Error> {
        Ok(F::string(&mut self.meter, value.to_string())?)
    }

    fn visit_u256(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: U256,
    ) -> Result<Self::Value, Self::Error> {
        Ok(F::string(&mut self.meter, value.to_string())?)
    }

    fn visit_bool(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: bool,
    ) -> Result<Self::Value, Self::Error> {
        Ok(F::bool(&mut self.meter, value)?)
    }

    fn visit_address(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(F::string(&mut self.meter, value.to_canonical_string(true))?)
    }

    fn visit_signer(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(F::string(&mut self.meter, value.to_canonical_string(true))?)
    }

    fn visit_vector(
        &mut self,
        driver: &mut AV::VecDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        if driver.element_layout().is_type(&TypeTag::U8) {
            use base64::Engine;
            use base64::engine::general_purpose::STANDARD;

            // Base64 encode arbitrary bytes
            if let Some(bytes) = driver
                .bytes()
                .get(driver.position()..(driver.position() + driver.len() as usize))
            {
                let b64 = STANDARD.encode(bytes);
                Ok(F::string(&mut self.meter, b64)?)
            } else {
                Err(AV::Error::UnexpectedEof.into())
            }
        } else {
            let mut elems = F::Vec::default();

            {
                let nested = self.meter.nest()?;
                let mut visitor = RpcVisitor::new(nested);
                while let Some(elem) = driver.next_element(&mut visitor)? {
                    F::vec_push_element(&mut visitor.meter, &mut elems, elem)?;
                }
            }

            Ok(F::vec(&mut self.meter, elems)?)
        }
    }

    fn visit_struct(
        &mut self,
        driver: &mut AV::StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let ty = &driver.struct_layout().type_;
        let layout = driver.struct_layout();

        if layout == &move_ascii_str_layout()
            || layout == &move_utf8_str_layout()
            || layout == &type_name_layout()
            || layout == &url_layout()
        {
            // 0x1::ascii::String or 0x1::string::String or 0x1::type_name::TypeName or 0x2::url::Url

            let lo = driver.position();
            driver.skip_field()?;
            let hi = driver.position();

            // HACK: Bypassing the layout to deserialize its bytes as a Rust type.
            let bytes = &driver.bytes()[lo..hi];
            let s: String = bcs::from_bytes(bytes).map_err(|_| Error::UnexpectedType)?;
            Ok(F::string(&mut self.meter, s)?)
        } else if layout == &UID::layout() || layout == &ID::layout() {
            // 0x2::object::UID or 0x2::object::ID

            let lo = driver.position();
            driver.skip_field()?;
            let hi = driver.position();

            // HACK: Bypassing the layout to deserialize its bytes as a Rust type.
            let bytes = &driver.bytes()[lo..hi];
            let id = AccountAddress::from_bytes(bytes)
                .map_err(|_| Error::UnexpectedType)?
                .to_canonical_string(true);

            Ok(F::string(&mut self.meter, id)?)
        } else if (&ty.address, ty.module.as_ref(), ty.name.as_ref()) == RESOLVED_STD_OPTION {
            // 0x1::option::Option

            match OV::OptionVisitor(self).visit_struct(driver)? {
                Some(value) => Ok(value),
                None => Ok(F::null(&mut self.meter)?),
            }
        } else if Balance::is_balance_layout(layout) {
            // 0x2::balance::Balance

            let lo = driver.position();
            driver.skip_field()?;
            let hi = driver.position();

            // HACK: Bypassing the layout to deserialize its bytes as a Rust type.
            let bytes = &driver.bytes()[lo..hi];
            let balance = bcs::from_bytes::<u64>(bytes)
                .map_err(|_| Error::UnexpectedType)?
                .to_string();

            Ok(F::string(&mut self.meter, balance)?)
        } else {
            // Arbitrary structs

            let mut map = F::Map::default();

            {
                let nested = self.meter.nest()?;
                let mut visitor = RpcVisitor::<F, _>::new(nested);
                while let Some((field, elem)) = driver.next_field(&mut visitor)? {
                    let name = field.name.to_string();
                    F::map_push_field(&mut visitor.meter, &mut map, name, elem)?;
                }
            }

            Ok(F::map(&mut self.meter, map)?)
        }
    }

    fn visit_variant(
        &mut self,
        driver: &mut AV::VariantDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let mut map = F::Map::default();
        {
            let mut nested_meter = self.meter.nest()?;

            let variant = F::string(&mut nested_meter, driver.variant_name().to_string())?;
            F::map_push_field(&mut nested_meter, &mut map, "@variant".to_owned(), variant)?;

            let mut visitor = RpcVisitor::<F, _>::new(nested_meter);
            while let Some((field, elem)) = driver.next_field(&mut visitor)? {
                let name = field.name.to_string();
                F::map_push_field(&mut visitor.meter, &mut map, name, elem)?;
            }
        }

        Ok(F::map(&mut self.meter, map)?)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use move_core_types::annotated_value as A;
    use move_core_types::ident_str;
    use move_core_types::language_storage::StructTag;
    use serde::Serialize;
    use serde_json::Value;
    use serde_json::json;

    use super::*;

    use A::MoveTypeLayout as L;

    macro_rules! struct_ {
        ($type:literal { $($name:literal : $layout:expr),* $(,)?}) => {
            A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
                type_: StructTag::from_str($type).expect("Failed to parse struct"),
                fields: vec![$(A::MoveFieldLayout {
                    name: ident_str!($name).to_owned(),
                    layout: $layout,
                }),*]
            }))
        }
    }

    macro_rules! vector_ {
        ($inner:expr) => {
            A::MoveTypeLayout::Vector(Box::new($inner))
        };
    }

    fn json<T: Serialize>(layout: A::MoveTypeLayout, data: T) -> Value {
        let bcs = bcs::to_bytes(&data).unwrap();
        let mut visitor = RpcVisitor::new(Unmetered);
        A::MoveValue::visit_deserialize(&bcs, &layout, &mut visitor).unwrap()
    }

    fn address(a: &str) -> sui_sdk_types::Address {
        sui_sdk_types::Address::from_str(a).unwrap()
    }

    #[test]
    fn json_bool() {
        let actual = json(L::Bool, true);
        let expect = json!(true);
        assert_eq!(actual, expect);

        let actual = json(L::Bool, false);
        let expect = json!(false);
        assert_eq!(actual, expect);
    }

    #[test]
    fn json_u8() {
        let actual = json(L::U8, 42u8);
        let expect = json!(42u8);
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_u16() {
        let actual = json(L::U16, 424u16);
        let expect = json!(424u16);
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_u32() {
        let actual = json(L::U32, 432_432u32);
        let expect = json!(432_432u32);
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_u64() {
        let actual = json(L::U64, 432_432_432_432u64);
        let expect = json!(432_432_432_432u64.to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_u128() {
        let actual = json(L::U128, 424_242_424_242_424_242_424u128);
        let expect = json!(424_242_424_242_424_242_424u128.to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_u256() {
        let actual = json(
            L::U256,
            U256::from_str("42424242424242424242424242424242424242424").unwrap(),
        );
        let expect = json!("42424242424242424242424242424242424242424");
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_ascii_string() {
        let l = A::MoveTypeLayout::Struct(Box::new(move_ascii_str_layout()));
        let actual = json(l, "The quick brown fox");
        let expect = json!("The quick brown fox");
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_utf8_string() {
        let l = A::MoveTypeLayout::Struct(Box::new(move_utf8_str_layout()));
        let actual = json(l, "The quick brown fox");
        let expect = json!("The quick brown fox");
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_type_name() {
        let l = A::MoveTypeLayout::Struct(Box::new(type_name_layout()));
        let actual = json(
            l,
            "0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>",
        );
        let expect = json!(
            "0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>"
        );
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_url() {
        let l = A::MoveTypeLayout::Struct(Box::new(url_layout()));
        let actual = json(l, "https://example.com");
        let expect = json!("https://example.com");
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_address() {
        let actual = json(L::Address, address("0x42"));
        let expect = json!(address("0x42").to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_signer() {
        let actual = json(L::Signer, address("0x42"));
        let expect = json!(address("0x42").to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_id() {
        let l = struct_!("0x2::object::ID" {
            "bytes": L::Address,
        });
        let actual = json(l, address("0x42"));
        let expect = json!(address("0x42").to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_uid() {
        let l = struct_!("0x2::object::UID" {
            "id": struct_!("0x2::object::ID" {
                "bytes": L::Address,
            })
        });
        let actual = json(l, address("0x42"));
        let expect = json!(address("0x42").to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_option() {
        let l = struct_!("0x42::foo::Bar" {
            "baz": struct_!("0x1::option::Option<u8>" { "vec": vector_!(L::U8) }),
        });

        let actual = json(l, Option::<Vec<u8>>::None);
        let expect = json!({
            "baz": null,
        });
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_balance() {
        let l = struct_!("0x2::balance::Balance<0x2::sui::SUI>" {
            "value": L::U64,
        });

        let actual = json(l, 100u64);
        let expect = json!(100u64.to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_compound() {
        let l = struct_!("0x42::foo::Bar" {
            "baz": struct_!("0x1::option::Option<u8>" { "vec": vector_!(L::U8) }),
            "qux": vector_!(struct_!("0x43::xy::Zzy" {
                "quy": L::U16,
                "quz": struct_!("0x1::option::Option<0x1::ascii::String>" {
                    "vec": vector_!(struct_!("0x1::ascii::String" {
                        "bytes": vector_!(L::U8),
                    }))
                }),
                "frob": L::Address,
            })),
        });

        let actual = json(
            l,
            (
                Option::<Vec<u8>>::None,
                vec![
                    (44u16, Some("Hello, world!"), address("0x45")),
                    (46u16, None, address("0x47")),
                ],
            ),
        );
        let expect = json!({
            "baz": null,
            "qux": [{
                "quy": 44,
                "quz": "Hello, world!",
                "frob": address("0x45").to_string(),
            },
            {
                "quy": 46,
                "quz": null,
                "frob": address("0x47").to_string(),
            }
            ],
        });
        assert_eq!(expect, actual);
    }
}
