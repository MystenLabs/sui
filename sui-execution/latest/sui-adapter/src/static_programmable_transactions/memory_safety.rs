// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{ast as T, env::Env};
use sui_types::error::ExecutionError;

pub fn verify(env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    todo!()
}
