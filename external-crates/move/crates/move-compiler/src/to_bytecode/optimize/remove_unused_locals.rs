// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::parser::ast::FunctionName;
use move_ir_types::ast as IR;
use std::collections::BTreeSet;

// Removes any unused locals. Most likely generated from other optimizations

pub fn optimize(
    _f: &FunctionName,
    _loop_heads: &BTreeSet<IR::BlockLabel_>,
    locals: &mut Vec<(IR::Var, IR::Type)>,
    blocks: &mut IR::BytecodeBlocks,
) -> bool {
    let mut unused = locals
        .iter()
        .map(|(sp!(_, v_), _)| v_.clone())
        .collect::<BTreeSet<_>>();
    for (_lbl, block) in blocks {
        for sp!(_, instr_) in block {
            match instr_ {
                IR::Bytecode_::CopyLoc(sp!(_, v_))
                | IR::Bytecode_::MoveLoc(sp!(_, v_))
                | IR::Bytecode_::StLoc(sp!(_, v_))
                | IR::Bytecode_::MutBorrowLoc(sp!(_, v_))
                | IR::Bytecode_::ImmBorrowLoc(sp!(_, v_)) => {
                    unused.remove(v_);
                }
                _ => (),
            }
        }
    }
    if unused.is_empty() {
        return false;
    }
    *locals = std::mem::take(locals)
        .into_iter()
        .filter(|(sp!(_, v_), _)| !unused.contains(v_))
        .collect();
    true
}
