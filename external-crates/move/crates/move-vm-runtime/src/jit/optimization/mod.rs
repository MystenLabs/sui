// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::validation::verification::ast as input;
use move_binary_format::errors::PartialVMResult;

pub mod ast;
pub mod insert_charge;
pub mod translate;

pub fn to_optimized_form(
    input: input::Package,
    enable_charge_instruction: bool,
) -> PartialVMResult<ast::Package> {
    let pkg = translate::package(input)?;
    optimize(pkg, enable_charge_instruction)
}

/// Entry point for optimization passes. Currently applies the Charge-insertion
/// pass which hoists per-instruction gas costs out of the interpreter loop.
/// The pass is only run when `enable_charge_instruction` is true; otherwise
/// the bytecode stream is left in its pre-batched form so that protocol
/// versions without the Charge opcode see identical bytecode.
pub fn optimize(
    pkg: ast::Package,
    enable_charge_instruction: bool,
) -> PartialVMResult<ast::Package> {
    if enable_charge_instruction {
        insert_charge::pass(pkg)
    } else {
        Ok(pkg)
    }
}
