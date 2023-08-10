// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use move_core_types::u256::U256;

struct BigInt(U256);

/// TODO: implement BigInt scalar type using u256
#[Scalar]
impl ScalarType for BigInt {
    fn parse(_value: Value) -> InputValueResult<Self> {
        unimplemented!()
    }

    fn to_value(&self) -> Value {
        unimplemented!()
    }
}
