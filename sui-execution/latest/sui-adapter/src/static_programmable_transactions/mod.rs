// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_value::ExecutionState;
use move_vm_runtime::move_vm::MoveVM;
use sui_protocol_config::ProtocolConfig;
use sui_types::error::ExecutionError;

// TODO we might replace this with a new one
pub use crate::programmable_transactions::linkage_view::LinkageView;

pub mod ast;
pub mod env;
pub mod execution;
pub mod input_arguments;
pub mod memory_safety;
pub mod optimize;
pub mod typing;

pub fn verify(
    env: &env::Env,
    pt: sui_types::transaction::ProgrammableTransaction,
) -> Result<ast::Transaction, ExecutionError> {
    let ast = typing::translate(env, pt)?;
    input_arguments::verify(env, &ast)?;
    Ok(ast)
}
