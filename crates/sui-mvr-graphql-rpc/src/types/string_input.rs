// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Opt-in to an implementation of `ScalarType` for a `$Type` that implements `FromStr`, solely for
/// use as an input (not an output). The type masquarades as a `String` in the GraphQL schema, to
/// avoid adding a new scalar.
macro_rules! impl_string_input {
    ($Type:ident) => {
        #[Scalar(name = "String", visible = false)]
        impl ScalarType for $Type {
            fn parse(value: Value) -> InputValueResult<Self> {
                if let Value::String(s) = value {
                    Ok(Self::from_str(&s)?)
                } else {
                    Err(InputValueError::expected_type(value))
                }
            }

            fn to_value(&self) -> Value {
                unimplemented!("String inputs should not be output");
            }
        }
    };
}

pub(crate) use impl_string_input;
