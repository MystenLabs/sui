// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use move_core_types::u256::U256;

struct Base64(String);

/// TODO: implement Base64 scalar type
#[Scalar]
impl ScalarType for Base64 {
    fn parse(_value: Value) -> InputValueResult<Self> {
        unimplemented!()
    }

    fn to_value(&self) -> Value {
        unimplemented!()
    }
}
