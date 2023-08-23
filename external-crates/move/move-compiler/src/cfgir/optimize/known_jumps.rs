// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::{
        ast::remap_labels,
        cfg::{MutForwardCFG, CFG},
    },
    hlir::ast::{BasicBlocks, Command, Command_, FunctionSignature, Label, SingleType, Var},
    shared::unique_map::UniqueMap,
};

use std::collections::{BTreeMap, BTreeSet};

/// returns true if anything changed
pub fn optimize(
    _signature: &FunctionSignature,
    _locals: &UniqueMap<Var, SingleType>,
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
    let known_jumps = find_known_jumps(blocks);
    optimize_known_jumps(blocks, known_jumps)
}

fn find_known_jumps(blocks: &BasicBlocks) -> BTreeMap<Label, Label> {
    use Command_ as C;
    let mut forwarded_jumps = BTreeMap::new();
    for (label, block) in blocks {
        if block.len() == 1 {
            match block.iter().last() {
                Some(
                    sp!(
                        _,
                        C::Jump {
                            target,
                            from_user: _
                        }
                    ),
                ) => {
                    forwarded_jumps.insert(*label, *target);
                }
                _ => (),
            }
        }
    }

    let mut known_jumps: BTreeMap<Label, Label> = BTreeMap::new();

    for (start, target) in &forwarded_jumps {
        let mut cur_target = *target;
        let mut seen = BTreeSet::from([*target]);
        while !known_jumps.contains_key(start) {
            if known_jumps.contains_key(&cur_target) {
                known_jumps.insert(*start, known_jumps[&cur_target]);
            } else if start == &cur_target {
                known_jumps.insert(*start, *start);
            } else if !forwarded_jumps.contains_key(&cur_target) {
                known_jumps.insert(*start, cur_target);
            } else {
                cur_target = forwarded_jumps[&cur_target];
                if seen.contains(&cur_target) {
                    // in a cycle, so bail
                    known_jumps.insert(*start, cur_target);
                } else {
                    seen.insert(cur_target.clone());
                }
            }
        }
    }
    known_jumps
        .into_iter()
        .filter(|(from, to)| from != to)
        .collect()
}

fn optimize_known_jumps(blocks: &mut BasicBlocks, known_jumps: BTreeMap<Label, Label>) -> bool {
    let mut changed = false;
    for block in blocks.values_mut() {
        for cmd in block {
            changed = optimize_cmd(cmd, &known_jumps) || changed;
        }
    }
    changed
}

fn optimize_cmd(sp!(_, cmd_): &mut Command, known_jumps: &BTreeMap<Label, Label>) -> bool {
    use Command_ as C;
    match cmd_ {
        C::Jump {
            target,
            from_user: _,
        } => {
            if known_jumps.contains_key(target) {
                *target = known_jumps[&target];
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
            if known_jumps.contains_key(if_true) {
                *if_true = known_jumps[&if_true];
                result = true;
            }
            if known_jumps.contains_key(if_false) {
                *if_false = known_jumps[&if_false];
                result = true;
            }
            result
        }
        _ => false,
    }
}

// Once we've inlined all our jumps, we need to renumber the remaining blocks for compactness.
fn remap_in_order(start: Label, blocks: &mut BasicBlocks) {
    let mut sorted_labels = blocks.keys().cloned().collect::<Vec<_>>();
    sorted_labels.sort();
    let mut remapping = sorted_labels
        .into_iter()
        .enumerate()
        .map(|(ndx, lbl)| (lbl, Label(ndx)))
        .collect::<BTreeMap<Label, Label>>();
    remapping.insert(start, start);
    if remapping.is_empty() {
        return;
    }
    let owned_blocks = std::mem::take(blocks);
    let (_start, remapped_blocks) = remap_labels(&remapping, start, owned_blocks);
    *blocks = remapped_blocks;
}
