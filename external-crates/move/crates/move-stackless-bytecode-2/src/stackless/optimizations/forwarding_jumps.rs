use crate::stackless::ast::{BasicBlock, BasicBlocks, Function, Instruction, Label};
use std::collections::{BTreeMap, BTreeSet};

type LabelMap = BTreeMap<Label, Label>;

struct Env {
    removed_blocks: Vec<Label>,
}

pub fn optimize(function: &mut Function) -> bool {
    let mut env = Env {
        removed_blocks: Vec::new(),
    };
    let changed = optimize_(&mut function.basic_blocks, &mut env);
    if changed {
        // TODO should we recompute CFG??
        // cfg.recopmute();
        // remap_in_order(function.entry_label, &mut function.basic_blocks);
        // let instrs = function.basic_blocks.iter().map(|(_, bb)|bb.to_string()).collect::<Vec<String>>().join("\n");
        env.removed_blocks.iter().for_each(|label| {
            function.basic_blocks.remove(label);
        });
    }
    changed
}

fn optimize_(basic_blocks: &mut BasicBlocks, env: &mut Env) -> bool {
    let final_jumps = find_forwarding_jump_destinations(basic_blocks, env);
    optimize_forwarding_jumps(basic_blocks, final_jumps)
}

fn find_forwarding_jump_destinations(blocks: &BasicBlocks, env: &mut Env) -> LabelMap {
    let mut forwarding_jumps = BTreeMap::new();
    blocks
        .iter()
        .filter(|(_, block)| block.instructions.len() == 1)
        .for_each(|(label, block)| {
            if let Some(Instruction::Jump(target)) = block.instructions.last() {
                forwarding_jumps.insert(*label, *target);
                env.removed_blocks.push(*label);
            }
        });

    let mut final_jumps = BTreeMap::new();
    for start in forwarding_jumps.keys() {
        if final_jumps.contains_key(start) {
            break;
        }
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
    blocks: &mut BTreeMap<usize, BasicBlock>,
    final_jumps: LabelMap,
) -> bool {
    let mut changed = false;
    for block in blocks.values_mut() {
        for instruction in &mut block.instructions {
            changed = optimize_instruction(instruction, &final_jumps) || changed;
        }
    }
    changed
}

fn optimize_instruction(instruction: &mut Instruction, final_jumps: &LabelMap) -> bool {
    match instruction {
        Instruction::Jump(target) => {
            if let Some(final_target) = final_jumps.get(target) {
                *target = *final_target;
                true
            } else {
                false
            }
        }
        Instruction::JumpIf {
            condition: _,
            then_label,
            else_label,
        } => {
            let mut result = false;
            if let Some(final_then) = final_jumps.get(then_label) {
                *then_label = *final_then;
                result = true;
            }
            if let Some(final_else) = final_jumps.get(else_label) {
                *else_label = *final_else;
                result = true;
            }
            result
        }
        _ => false,
    }
}

// TODO remove this function if not needed
// fn remap_in_order(start: Label, blocks: &mut BasicBlocks) {
//     let mut remapping = blocks
//         .keys()
//         .copied().
//         enumerate()
//         .map(|(i, label)| (label, i as Label))
//         .collect::<LabelMap>();
//     remapping.insert(start, start);
//     let owned_blocks = std::mem::take(blocks);
//     let (_start, remapped_blocks) = remap_labels(&remapping, start, owned_blocks);
//     *blocks = remapped_blocks;
// }

//**************************************************************************************************
// Label util
//**************************************************************************************************

// TODO remove this function if not needed
// pub fn remap_labels(
//     remapping: &LabelMap,
//     start: Label,
//     blocks: BasicBlocks,
// ) -> (Label, BasicBlocks) {
//     let blocks = blocks
//         .into_iter()
//         .map(|(label, mut block)| {
//             remap_block_labels(remapping, &mut block);
//             (remapping[&label], block)
//         })
//         .collect();
//     (remapping[&start], blocks)
// }

// TODO remove this function if not needed
// fn remap_block_labels(remapping: &LabelMap, block: &mut BasicBlock) {
//     for instruction in block.instructions.iter_mut() {
//         remap_instruction_labels(remapping, instruction);
//     }
// }

// TODO remove this function if not needed
// fn remap_instruction_labels(remapping: &LabelMap, instruction: &mut Instruction) {
//     match instruction {
//         Instruction::Jump(target) => {
//             if let Some(label) = remapping.get(target) {
//                 *target = *label;
//             } else {
//                 panic!("Jump target {:?} not found in remapping {:?}", target, remapping);
//             }
//         }
//         Instruction::JumpIf {
//             then_label,
//             else_label,
//             ..
//         } => {
//             *then_label = remapping[then_label];
//             *else_label = remapping[else_label];
//         }
//         Instruction::VariantSwitch { cases, .. } => {
//             for case in cases {
//                 *case = remapping[case];
//             }
//         }
//         Instruction::Return(_)
//         | Instruction::AssignReg { .. }
//         | Instruction::StoreLoc { .. }
//         | Instruction::Abort(_)
//         | Instruction::Nop
//         | Instruction::Drop(_)
//         | Instruction::NotImplemented(_) => (),
//     }
// }
