// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::jit::optimization::ast;

use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn package(pkg: &mut ast::Package) -> bool {
    let mut changed = false;
    pkg.modules.iter_mut().for_each(|(_, m)| {
        module(&mut changed, m);
    });
    changed
}

fn module(changed: &mut bool, m: &mut ast::Module) {
    m.functions.iter_mut().for_each(|(_ndx, code)| {
        code.iter_mut()
            .for_each(|code| blocks(changed, &mut code.code))
    });
}

fn blocks(changed: &mut bool, blocks: &mut BTreeMap<ast::Label, Vec<ast::Bytecode>>) {
    for (_, block_code) in blocks.iter_mut() {
        eliminate_dead_code(changed, block_code);
    }
}

fn eliminate_dead_code(changed: &mut bool, code: &mut Vec<ast::Bytecode>) {
    use ast::Bytecode;
    let mut output_code = vec![];
    while let Some(instr) = code.pop() {
        if matches!(instr, Bytecode::Nop) {
            *changed = true;
            continue;
        }
        match &instr {
            Bytecode::Ret | Bytecode::Abort | Bytecode::Branch(_) => {
                *changed = true;
                output_code = vec![];
            }
            _ => (),
        }
        output_code.push(instr);
    }
    output_code.reverse();
    assert!(std::mem::replace(code, output_code).is_empty());
}
