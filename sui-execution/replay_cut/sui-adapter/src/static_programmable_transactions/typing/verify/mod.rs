// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::error::ExecutionError;

use crate::{
    execution_mode::ExecutionMode,
    static_programmable_transactions::{env, typing::ast as T},
};

pub mod drop_safety;
pub mod input_arguments;
pub mod memory_safety;
pub mod move_functions;
pub mod private_entry_arguments;

pub fn transaction<Mode: ExecutionMode>(
    env: &env::Env,
    ast: &mut T::Transaction,
) -> Result<(), ExecutionError> {
    input_arguments::verify::<Mode>(env, &*ast)?;
    move_functions::verify::<Mode>(env, &*ast)?;
    memory_safety::verify(env, &*ast)?;
    drop_safety::refine_and_verify::<Mode>(env, ast)?;
    private_entry_arguments::verify::<Mode>(env, &*ast)?;
    Ok(())
}
