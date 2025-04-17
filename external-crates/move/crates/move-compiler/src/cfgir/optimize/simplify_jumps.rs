// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_proc_macros::growing_stack;

use crate::{
    cfgir::cfg::MutForwardCFG,
    diagnostics::DiagnosticReporter,
    expansion::ast::Mutability,
    hlir::ast::{
        Command, Command_, Exp, FunctionSignature, SingleType, UnannotatedExp_, Value, Value_, Var,
    },
    parser::ast::ConstantName,
    shared::unique_map::UniqueMap,
};

/// returns true if anything changed
pub fn optimize(
    _reporter: &DiagnosticReporter,
    _signature: &FunctionSignature,
    _locals: &UniqueMap<Var, (Mutability, SingleType)>,
    _constants: &UniqueMap<ConstantName, Value>,
    cfg: &mut MutForwardCFG,
) -> bool {
    let mut changed = false;
    for block in cfg.blocks_mut().values_mut() {
        for cmd in block {
            changed = optimize_cmd(cmd) || changed;
        }
    }
    if changed {
        let _dead_blocks = cfg.recompute();
    }
    changed
}

#[growing_stack]
fn optimize_cmd(sp!(_, cmd_): &mut Command) -> bool {
    use Command_ as C;
    use UnannotatedExp_ as E;
    use Value_ as V;
    match cmd_ {
        C::JumpIf {
            cond:
                Exp {
                    exp: sp!(_, E::Value(sp!(_, V::Bool(cond)))),
                    ..
                },
            if_true,
            if_false,
        } => {
            let lbl = if *cond { *if_true } else { *if_false };
            *cmd_ = C::Jump {
                target: lbl,
                from_user: false,
            };
            true
        }
        _ => false,
    }
}
