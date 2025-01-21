// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::{FunctionDefinitionIndex, VariantJumpTable};

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
    let ast::Module {
        functions,
        compiled_module: _,
    } = m;
    functions
        .iter_mut()
        .for_each(|(ndx, code)| function(changed, *ndx, code));
}

struct BlockContext<'changed, 'labels, 'tables> {
    changed: &'changed mut bool,
    live_labels: &'labels mut BTreeSet<ast::Label>,
    jump_tables: &'tables [VariantJumpTable],
}

fn function(changed: &mut bool, _ndx: FunctionDefinitionIndex, fun: &mut ast::Function) {
    let Some(code) = &mut fun.code else { return };

    let ast::Code { jump_tables, code } = code;
    let mut live_labels = BTreeSet::new();
    let mut context = BlockContext {
        changed,
        live_labels: &mut live_labels,
        jump_tables,
    };
    blocks(&mut context, code);
}

fn blocks(
    context: &mut BlockContext<'_, '_, '_>,
    blocks: &mut BTreeMap<ast::Label, Vec<ast::Bytecode>>,
) {
    // First, eliminate all of the intra-block dead code
    for (_, block_code) in blocks.iter_mut() {
        eliminate_unreachable(context, block_code);
    }
    // Now, write down the live labels:
    // - Record any instruction that is a valid jump target
    // - Record any instruction that is a fall-through target
    let labels = blocks.keys().collect::<BTreeSet<_>>();
    for ((_, block), next) in blocks.iter().zip(labels.into_iter().skip(1)) {
        // Find jump targets
        for instr in block {
            if let Some(labels) = instr.branch_target(context.jump_tables) {
                context.live_labels.extend(labels);
            }
        }
        // Check for fall-through
        if let Some(instr) = block.last() {
            if !instr.is_unconditional_branch() {
                context.live_labels.insert(*next);
            }
        }
    }
}

fn eliminate_unreachable(context: &mut BlockContext<'_, '_, '_>, code: &mut Vec<ast::Bytecode>) {
    use ast::Bytecode;
    let mut output_code = vec![];
    while let Some(instr) = code.pop() {
        if matches!(instr, Bytecode::Nop) {
            *context.changed = true;
            continue;
        }
        if instr.is_unconditional_branch() {
            *context.changed = true;
            output_code = vec![];
        }
        output_code.push(instr);
    }
    output_code.reverse();
    assert!(std::mem::replace(code, output_code).is_empty());
}
