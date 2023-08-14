// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

// ISO-8601 Date and Time
// Encoded as a 64-bit unix timestamp
struct DateTime(u64);

// TODO: unit tests
// TODO: implement DateTime scalar type
#[Scalar]
impl ScalarType for DateTime {
    fn parse(_value: Value) -> InputValueResult<Self> {
        unimplemented!()
    }

    fn to_value(&self) -> Value {
        unimplemented!()
    }
}
