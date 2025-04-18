// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{env::Env, typing::ast as T};
use sui_types::error::ExecutionError;

pub fn verify(_env: &Env, _txn: &T::Transaction) -> Result<(), ExecutionError> {
    todo!()
}
