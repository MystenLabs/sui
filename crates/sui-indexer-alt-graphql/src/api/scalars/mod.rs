// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod base64;
pub(crate) mod big_int;
pub(crate) mod cursor;
pub(crate) mod date_time;
pub(crate) mod digest;
pub(crate) mod domain;
pub(crate) mod json;
pub(crate) mod owner_kind;
pub(crate) mod sui_address;
pub(crate) mod type_filter;
pub(crate) mod uint53;

/// Opt-in to an implementation of `ScalarType` for a `$Type` that implements `FromStr`, solely for
/// use as an input (not an output). The type masquarades as a `String` in the GraphQL schema, to
/// avoid adding a new scalar.
macro_rules! impl_string_input {
    ($Type:ident) => {
        #[async_graphql::Scalar(name = "String", visible = false)]
        impl async_graphql::ScalarType for $Type {
            fn parse(value: async_graphql::Value) -> async_graphql::InputValueResult<Self> {
                if let async_graphql::Value::String(s) = value {
                    Ok(Self::from_str(&s)?)
                } else {
                    Err(async_graphql::InputValueError::expected_type(value))
                }
            }

            fn to_value(&self) -> async_graphql::Value {
                unimplemented!("String inputs should not be output");
            }
        }
    };
}

pub(crate) use impl_string_input;
