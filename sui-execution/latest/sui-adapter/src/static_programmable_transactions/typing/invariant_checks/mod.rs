// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_mode::ExecutionMode,
    static_programmable_transactions::{env, typing::ast as T},
};
use sui_types::error::ExecutionError;

pub mod defining_ids_in_types;
pub mod memory_safety;
pub mod type_check;

pub fn transaction<Mode: ExecutionMode>(
    env: &env::Env,
    tt: &T::Transaction,
) -> Result<(), ExecutionError> {
    defining_ids_in_types::verify(env, tt)?;
    type_check::verify::<Mode>(env, tt)?;
    memory_safety::verify(env, tt)?;
    // Add in other invariants checks here as needed/desired.
    Ok(())
}
