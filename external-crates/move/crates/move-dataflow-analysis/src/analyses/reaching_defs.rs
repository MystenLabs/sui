// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Reaching definitions analysis (forward).
//!
//! For each local variable, tracks the set of definition sites `(Label, instruction_index)`
//! that may reach the current program point without an intervening redefinition.
//!
//! A local is *defined* by:
//! - `StoreLoc { loc, .. }` — stores a value into local `loc`
//!
//! A `Move` of a local kills its definitions (the local is no longer usable after a move).

use crate::{
    analysis::{block_successors, BlockState, StateMap, TransferFunctions},
    domains::{AbstractDomain, JoinResult, MapDomain, SetDomain},
};
use move_stackless_bytecode_2::ast::{Function, Instruction, Label, LocalId, RValue};
use std::collections::VecDeque;

/// A definition site: (block label, instruction index within that block).
pub type DefSite = (Label, usize);

/// The abstract state: for each local, the set of definition sites that may reach here.
pub type ReachingDefsState = MapDomain<LocalId, SetDomain<DefSite>>;

/// Reaching definitions transfer functions.
///
/// Uses an interior-mutable `current_label` so the manual analysis loop can
/// communicate which block is being processed (the generic framework doesn't
/// pass block labels to `execute`).
pub struct ReachingDefs {
    current_label: std::cell::Cell<Label>,
}

impl ReachingDefs {
    pub fn new() -> Self {
        Self {
            current_label: std::cell::Cell::new(0),
        }
    }
}

impl TransferFunctions for ReachingDefs {
    type State = ReachingDefsState;
    const BACKWARD: bool = false;

    fn execute(&self, state: &mut Self::State, instr: &Instruction, idx: usize) {
        let label = self.current_label.get();
        match instr {
            Instruction::StoreLoc { loc, .. } => {
                // Kill previous definitions, gen this one.
                state.insert(*loc, SetDomain::singleton((label, idx)));
            }
            Instruction::AssignReg { rhs, .. } => {
                // A Move consumes the local, killing its definitions.
                if let RValue::Local {
                    op: move_stackless_bytecode_2::ast::LocalOp::Move,
                    arg,
                } = rhs
                {
                    state.remove(arg);
                }
            }
            _ => {}
        }
    }
}

/// Create an initial state where each local `0..num_locals` is defined at a
/// synthetic entry site `(Label::MAX, local_id)`.
pub fn initial_state(num_locals: usize) -> ReachingDefsState {
    let mut state = ReachingDefsState::default();
    for i in 0..num_locals {
        state.insert(i, SetDomain::singleton((Label::MAX, i)));
    }
    state
}

/// Run reaching definitions on a function, returning per-block pre/post states.
pub fn analyze(func: &Function, num_locals: usize) -> StateMap<ReachingDefsState> {
    let transfer = ReachingDefs::new();
    let init = initial_state(num_locals);
    let mut state_map: StateMap<ReachingDefsState> = StateMap::new();
    let mut work_list = VecDeque::new();

    // Seed all blocks so every block appears in the state map.
    // Entry block gets the initial state; others start at bottom.
    let bottom = ReachingDefsState::default();
    for &label in func.basic_blocks.keys() {
        let pre = if label == func.entry_label {
            init.clone()
        } else {
            bottom.clone()
        };
        state_map.insert(
            label,
            BlockState {
                pre: pre.clone(),
                post: pre,
            },
        );
    }
    work_list.push_back(func.entry_label);
    for &label in func.basic_blocks.keys() {
        if !work_list.contains(&label) {
            work_list.push_back(label);
        }
    }

    while let Some(label) = work_list.pop_front() {
        transfer.current_label.set(label);
        let pre = state_map[&label].pre.clone();
        let block = &func.basic_blocks[&label];

        let mut state = pre;
        for (idx, instr) in block.instructions.iter().enumerate() {
            transfer.execute(&mut state, instr, idx);
        }
        let post = state;

        for succ in block_successors(block) {
            if let Some(succ_state) = state_map.get_mut(&succ) {
                if succ_state.pre.join(&post) == JoinResult::Changed
                    && !work_list.contains(&succ)
                {
                    work_list.push_back(succ);
                }
            }
        }
        state_map.get_mut(&label).unwrap().post = post;
    }
    state_map
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_stackless_bytecode_2::ast::*;

    fn make_function(blocks: Vec<(Label, Vec<Instruction>)>, entry: Label) -> Function {
        let basic_blocks = blocks
            .into_iter()
            .map(|(label, instrs)| (label, BasicBlock::from_instructions(label, instrs)))
            .collect();
        Function {
            name: move_symbol_pool::Symbol::from("test"),
            entry_label: entry,
            basic_blocks,
        }
    }

    fn trivial_imm() -> Trivial {
        Trivial::Immediate(move_core_types::runtime_value::MoveValue::U64(0))
    }

    #[test]
    fn test_single_store() {
        let func = make_function(
            vec![(
                0,
                vec![
                    Instruction::StoreLoc {
                        loc: 0,
                        value: trivial_imm(),
                    },
                    Instruction::Return(vec![]),
                ],
            )],
            0,
        );
        let result = analyze(&func, 1);
        let post = &result[&0].post;
        assert_eq!(
            post.get(&0).unwrap(),
            &SetDomain::singleton((0_usize, 0_usize))
        );
    }

    #[test]
    fn test_two_stores_kill() {
        let func = make_function(
            vec![(
                0,
                vec![
                    Instruction::StoreLoc {
                        loc: 0,
                        value: trivial_imm(),
                    },
                    Instruction::StoreLoc {
                        loc: 0,
                        value: trivial_imm(),
                    },
                    Instruction::Return(vec![]),
                ],
            )],
            0,
        );
        let result = analyze(&func, 1);
        let post = &result[&0].post;
        assert_eq!(
            post.get(&0).unwrap(),
            &SetDomain::singleton((0_usize, 1_usize))
        );
    }

    #[test]
    fn test_diamond_merge() {
        // Block 0: StoreLoc(0), JumpIf -> 1 or 2
        // Block 1: StoreLoc(0), Jump(3)
        // Block 2: Jump(3)  (no store, so def from block 0 flows through)
        // Block 3: Return
        let func = make_function(
            vec![
                (
                    0,
                    vec![
                        Instruction::StoreLoc {
                            loc: 0,
                            value: trivial_imm(),
                        },
                        Instruction::JumpIf {
                            condition: trivial_imm(),
                            then_label: 1,
                            else_label: 2,
                        },
                    ],
                ),
                (
                    1,
                    vec![
                        Instruction::StoreLoc {
                            loc: 0,
                            value: trivial_imm(),
                        },
                        Instruction::Jump(3),
                    ],
                ),
                (2, vec![Instruction::Jump(3)]),
                (3, vec![Instruction::Return(vec![])]),
            ],
            0,
        );
        let result = analyze(&func, 1);
        let pre_3 = &result[&3].pre;
        let defs = pre_3.get(&0).unwrap();
        // From block 1: def at (1, 0). From block 2: def (0, 0) passes through.
        assert!(defs.contains(&(0_usize, 0_usize)));
        assert!(defs.contains(&(1_usize, 0_usize)));
        assert_eq!(defs.len(), 2);
    }
}
