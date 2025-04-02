// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{ast as T, env::Env};
use std::collections::{BTreeMap, BTreeSet};
use sui_types::error::ExecutionError;

struct Context {
    root: Ref,
    gas_coin: Value,
    inputs: Vec<Value>,
    results: Vec<Vec<Value>>,
}

enum Value {
    // No value
    Invalid,
    // A non reference value
    NonRef,
    // A reference value
    Ref(Ref),
}

pub fn verify(env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    todo!()
}
