// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::cfg::BlockCFG,
    hlir::ast::{
        Command, Command_, Exp, FunctionSignature, SingleType, UnannotatedExp_, Value_, Var,
    },
    shared::unique_map::UniqueMap,
};

/// returns true if anything changed
pub fn optimize(
    _signature: &FunctionSignature,
    _locals: &UniqueMap<Var, SingleType>,
    cfg: &mut BlockCFG,
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
