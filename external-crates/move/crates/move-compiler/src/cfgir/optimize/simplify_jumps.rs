// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_proc_macros::growing_stack;

use super::Context;
use crate::{
    cfgir::cfg::MutForwardCFG,
    hlir::ast::{Command, Command_, Value_},
};

/// returns true if anything changed
pub fn optimize(context: &mut Context, cfg: &mut MutForwardCFG) -> bool {
    let mut changed = false;
    for block in cfg.blocks_mut().values_mut() {
        for cmd in block {
            changed = optimize_cmd(context, cmd) || changed;
        }
    }
    if changed {
        let _dead_blocks = cfg.recompute();
    }
    changed
}

#[growing_stack]
fn optimize_cmd(context: &Context, sp!(_, cmd_): &mut Command) -> bool {
    use Command_ as C;
    let C::JumpIf {
        cond,
        if_true,
        if_false,
    } = cmd_
    else {
        return false;
    };
    let Some(Value_::Bool(cond)) = context.foldable_exp(cond) else {
        return false;
    };
    let lbl = if *cond { *if_true } else { *if_false };
    *cmd_ = C::Jump {
        target: lbl,
        from_user: false,
    };
    true
}
