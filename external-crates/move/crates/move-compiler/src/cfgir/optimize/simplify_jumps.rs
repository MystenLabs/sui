// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_proc_macros::growing_stack;

use crate::{
    cfgir::cfg::MutForwardCFG,
    diagnostics::DiagnosticReporter,
    expansion::ast::{ModuleIdent, Mutability},
    hlir::ast::{
        Command, Command_, Exp, FunctionSignature, SingleType, UnannotatedExp_, Value, Value_, Var,
    },
    parser::ast::ConstantName,
    shared::unique_map::UniqueMap,
};
use std::collections::BTreeMap;

/// returns true if anything changed
pub fn optimize(
    _reporter: &DiagnosticReporter,
    _signature: &FunctionSignature,
    _locals: &UniqueMap<Var, (Mutability, SingleType)>,
    constants: &BTreeMap<(ModuleIdent, ConstantName), Value>,
    cfg: &mut MutForwardCFG,
) -> bool {
    let mut changed = false;
    for block in cfg.blocks_mut().values_mut() {
        for cmd in block {
            changed = optimize_cmd(constants, cmd) || changed;
        }
    }
    if changed {
        let _dead_blocks = cfg.recompute();
    }
    changed
}

#[growing_stack]
fn optimize_cmd(
    constants: &BTreeMap<(ModuleIdent, ConstantName), Value>,
    sp!(_, cmd_): &mut Command,
) -> bool {
    use Command_ as C;
    let C::JumpIf {
        cond,
        if_true,
        if_false,
    } = cmd_
    else {
        return false;
    };
    let Some(Value_::Bool(cond)) = foldable_exp(constants, cond) else {
        return false;
    };
    let lbl = if *cond { *if_true } else { *if_false };
    *cmd_ = C::Jump {
        target: lbl,
        from_user: false,
    };
    true
}

/// Returns `Some` if value of `e` if it is statically known.
fn foldable_exp<'a>(
    constants: &'a BTreeMap<(ModuleIdent, ConstantName), Value>,
    e: &'a Exp,
) -> Option<&'a Value_> {
    use UnannotatedExp_ as E;
    match &e.exp.value {
        E::Constant(module, name) => constants.get(&(*module, *name)).map(|sp!(_, v_)| v_),
        E::Value(sp!(_, v_)) => Some(v_),
        _ => None,
    }
}
