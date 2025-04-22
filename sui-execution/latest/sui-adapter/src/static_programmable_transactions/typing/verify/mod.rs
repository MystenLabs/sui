// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::error::ExecutionError;

use crate::static_programmable_transactions::{env, typing::ast as T};

pub mod input_arguments;
pub mod memory_safety;
pub mod move_functions;

pub fn transaction(env: &env::Env, ast: &T::Transaction) -> Result<(), ExecutionError> {
    input_arguments::verify(env, ast)?;
    move_functions::verify(env, ast)?;
    memory_safety::verify(env, ast)?;
    Ok(())
}
