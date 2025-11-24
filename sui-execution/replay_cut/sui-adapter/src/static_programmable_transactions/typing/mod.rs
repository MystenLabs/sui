// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_mode::ExecutionMode,
    static_programmable_transactions::{
        env,
        loading::ast as L,
        metering::{self, translation_meter::TranslationMeter},
    },
};
use sui_types::error::ExecutionError;

pub mod ast;
pub mod invariant_checks;
pub mod translate;
pub mod verify;

pub fn translate_and_verify<Mode: ExecutionMode>(
    meter: &mut TranslationMeter<'_, '_>,
    env: &env::Env,
    lt: L::Transaction,
) -> Result<ast::Transaction, ExecutionError> {
    let mut ast = translate::transaction::<Mode>(env, lt)?;
    metering::typing::meter(meter, &ast)?;
    verify::transaction::<Mode>(env, &mut ast)?;
    invariant_checks::transaction::<Mode>(env, &ast)?;
    Ok(ast)
}
