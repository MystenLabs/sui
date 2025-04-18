// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::env;
use sui_types::error::ExecutionError;

pub mod ast;
pub mod optimize;
pub mod translate;
pub mod verify;

pub fn translate_and_verify(
    env: &env::Env,
    pt: sui_types::transaction::ProgrammableTransaction,
) -> Result<ast::Transaction, ExecutionError> {
    let ast = translate::transaction(env, pt)?;
    verify::transaction(env, &ast)?;
    Ok(ast)
}
