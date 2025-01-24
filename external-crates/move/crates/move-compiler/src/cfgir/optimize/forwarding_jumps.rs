// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//**************************************************************************************************
// OPTIMIZE FORWARDING JUMPS
// -----------------------------
// This optimization forwards and removes "forwarding" jumps within a function. For example, the
// following code will jump to 1 just to immediately jump to 3:
//
// Label 0:
//     JumpIf(<exp>, 1, 2)
// Label 1
//     Jump 3
// Label 2:
//     ...
//     Ret
// Label 3:
//     ...
//
// This optimization instead fowards these jumps, eliminating the other blocks:
// Label 0:
//     JumpIf(<exp>, 2, 1)
// Label 1:
//     ...
//     Ret
// Label 2:
//     ...

use move_proc_macros::growing_stack;

use crate::{
    cfgir::{
        ast::remap_labels,
        cfg::{MutForwardCFG, CFG},
    },
    diagnostics::DiagnosticReporter,
    expansion::ast::Mutability,
    hlir::ast::{BasicBlocks, Command, Command_, FunctionSignature, Label, SingleType, Value, Var},
    parser::ast::ConstantName,
    shared::unique_map::UniqueMap,
};

use std::collections::{BTreeMap, BTreeSet};

/// returns true if anything changed
pub fn optimize(
    _reporter: &DiagnosticReporter,
    _signature: &FunctionSignature,
    _locals: &UniqueMap<Var, (Mutability, SingleType)>,
    _constants: &UniqueMap<ConstantName, Value>,
    cfg: &mut MutForwardCFG,
) -> bool {
    let changed = optimize_(cfg.blocks_mut());
    if changed {
        cfg.recompute();
        remap_in_order(cfg.start_block(), cfg.blocks_mut());
    }
    changed
}

fn optimize_(blocks: &mut BasicBlocks) -> bool {
    let final_jumps = find_forwarding_jump_destinations(blocks);
    optimize_forwarding_jumps(blocks, final_jumps)
}

type LabelMap = BTreeMap<Label, Label>;

fn find_forwarding_jump_destinations(blocks: &BasicBlocks) -> LabelMap {
    use Command_ as C;
    let mut forwarding_jumps = BTreeMap::new();
    for (label, block) in blocks.iter().filter(|(_, block)| block.len() == 1) {
        if let Some(sp!(_, C::Jump { target, .. })) = block.iter().last() {
            forwarding_jumps.insert(*label, *target);
        }
    }

    // Computes the label map of forwarding jumps to their final destinations, collapsing any
    // forwarding blocks between the starting label and final label. Note that we have to
    // take care to detect cycles (fairly common in empty while loops). We also always look up the
    // new label up in the final map to allow us to short-circuit work we've already done.
    let mut final_jumps: LabelMap = BTreeMap::new();

    for start in forwarding_jumps.keys() {
        if final_jumps.contains_key(start) {
            break;
        };
        let mut target = *start;
        let mut seen = BTreeSet::new();
        while let Some(next_target) = forwarding_jumps.get(&target) {
            if let Some(final_jump) = final_jumps.get(next_target) {
                target = *final_jump;
                break;
            } else if start == next_target {
                target = *start;
                break;
            } else if seen.contains(next_target) {
                // in a cycle, so bail
                target = *next_target;
                break;
            } else {
                target = *next_target;
                seen.insert(target);
            }
        }
        final_jumps.insert(*start, target);
        for source in seen {
            final_jumps.insert(source, target);
        }
    }

    final_jumps
        .into_iter()
        .filter(|(from, to)| from != to)
        .collect()
}

fn optimize_forwarding_jumps(
    blocks: &mut BasicBlocks,
    final_jumps: BTreeMap<Label, Label>,
) -> bool {
    let mut changed = false;
    for block in blocks.values_mut() {
        for cmd in block {
            changed = optimize_cmd(cmd, &final_jumps) || changed;
        }
    }
    changed
}

#[growing_stack]
fn optimize_cmd(sp!(_, cmd_): &mut Command, final_jumps: &BTreeMap<Label, Label>) -> bool {
    use Command_ as C;
    match cmd_ {
        C::Jump {
            target,
            from_user: _,
        } => {
            if let Some(final_target) = final_jumps.get(target) {
                *target = *final_target;
                true
            } else {
                false
            }
        }
        C::JumpIf {
            cond: _,
            if_true,
            if_false,
        } => {
            let mut result = false;
            if let Some(final_target) = final_jumps.get(if_true) {
                *if_true = *final_target;
                result = true;
            }
            if let Some(final_target) = final_jumps.get(if_false) {
                *if_false = *final_target;
                result = true;
            }
            result
        }
        _ => false,
    }
}

// Once we've inlined all our jumps, we need to renumber the remaining blocks for compactness.
fn remap_in_order(start: Label, blocks: &mut BasicBlocks) {
    let mut remapping = blocks
        .keys()
        .copied()
        .enumerate()
        .map(|(ndx, lbl)| (lbl, Label(ndx)))
        .collect::<BTreeMap<Label, Label>>();
    remapping.insert(start, start);
    let owned_blocks = std::mem::take(blocks);
    let (_start, remapped_blocks) = remap_labels(&remapping, start, owned_blocks);
    *blocks = remapped_blocks;
}
