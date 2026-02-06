// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Generic dataflow analysis framework for Move stackless bytecode.
//!
//! Supports both forward and backward analyses over the CFG of a
//! `move_stackless_bytecode_2::ast::Function`. The framework is parameterized by:
//!
//! - An `AbstractDomain` for the lattice state
//! - A `TransferFunctions` implementation for per-instruction effects
//! - A direction flag (`BACKWARD`)

use crate::domains::{AbstractDomain, JoinResult};
use move_stackless_bytecode_2::ast::{BasicBlock, Function, Instruction, Label};
use std::collections::{BTreeMap, VecDeque};

// =============================================================================
// Block-level state

/// Pre- and post-state for a single basic block.
#[derive(Clone, Debug)]
pub struct BlockState<State: Clone> {
    pub pre: State,
    pub post: State,
}

/// Map from block label to its pre/post state pair.
pub type StateMap<State> = BTreeMap<Label, BlockState<State>>;

// =============================================================================
// CFG helpers

/// Extract successor labels from the terminator of a basic block.
pub fn block_successors(block: &BasicBlock) -> Vec<Label> {
    match block.instructions.last() {
        Some(Instruction::Jump(lbl)) => vec![*lbl],
        Some(Instruction::JumpIf {
            then_label,
            else_label,
            ..
        }) => vec![*then_label, *else_label],
        Some(Instruction::VariantSwitch { labels, .. }) => labels.clone(),
        Some(Instruction::Return(_) | Instruction::Abort(_)) => vec![],
        // Non-terminator or empty block: no successors.
        _ => vec![],
    }
}

/// Compute predecessor map from the function's basic blocks.
fn compute_predecessors(func: &Function) -> BTreeMap<Label, Vec<Label>> {
    let mut preds: BTreeMap<Label, Vec<Label>> = BTreeMap::new();
    for (&label, _) in &func.basic_blocks {
        preds.entry(label).or_default();
    }
    for (&label, block) in &func.basic_blocks {
        for succ in block_successors(block) {
            preds.entry(succ).or_default().push(label);
        }
    }
    preds
}

/// Find exit blocks (those ending in Return or Abort).
fn find_exit_blocks(func: &Function) -> Vec<Label> {
    func.basic_blocks
        .iter()
        .filter_map(|(&label, block)| match block.instructions.last() {
            Some(Instruction::Return(_) | Instruction::Abort(_)) => Some(label),
            _ => None,
        })
        .collect()
}

// =============================================================================
// Transfer functions trait

/// Defines per-instruction transfer functions for a dataflow analysis.
pub trait TransferFunctions {
    type State: AbstractDomain + Clone + Default;

    /// `true` for backward analysis, `false` for forward.
    const BACKWARD: bool;

    /// Apply the effect of `instr` (at position `idx` within its block) to `state`.
    ///
    /// - Forward: `state` is the pre-state; mutate to post-state.
    /// - Backward: instructions are visited in reverse; `state` is the state
    ///   after the instruction, mutate to the state before.
    fn execute(&self, state: &mut Self::State, instr: &Instruction, idx: usize);
}

// =============================================================================
// DataflowAnalysis

/// Fixpoint solver for dataflow analyses.
///
/// Blanket-implemented for all `TransferFunctions` implementations.
pub trait DataflowAnalysis: TransferFunctions {
    /// Run the fixpoint analysis, returning per-block pre/post states.
    fn analyze(&self, initial_state: Self::State, func: &Function) -> StateMap<Self::State>
    where
        Self: Sized,
    {
        if Self::BACKWARD {
            analyze_backward(self, initial_state, func)
        } else {
            analyze_forward(self, initial_state, func)
        }
    }

    /// Re-execute the analysis within each block to recover per-instruction states.
    ///
    /// Returns a map from `(Label, instruction_index)` to the value produced by `f`,
    /// which receives `(state_before, state_after)` for each instruction.
    fn state_per_instruction<A>(
        &self,
        state_map: &StateMap<Self::State>,
        func: &Function,
        mut f: impl FnMut(&Self::State, &Self::State) -> A,
    ) -> BTreeMap<(Label, usize), A>
    where
        Self: Sized,
    {
        let mut result = BTreeMap::new();
        for (&label, block_state) in state_map {
            let block = &func.basic_blocks[&label];
            if Self::BACKWARD {
                let mut state = block_state.pre.clone();
                for idx in (0..block.instructions.len()).rev() {
                    let after = state.clone();
                    self.execute(&mut state, &block.instructions[idx], idx);
                    result.insert((label, idx), f(&state, &after));
                }
            } else {
                let mut state = block_state.pre.clone();
                for idx in 0..block.instructions.len() {
                    let before = state.clone();
                    self.execute(&mut state, &block.instructions[idx], idx);
                    result.insert((label, idx), f(&before, &state));
                }
            }
        }
        result
    }
}

impl<T: TransferFunctions> DataflowAnalysis for T {}

// =============================================================================
// Forward fixpoint solver

fn analyze_forward<T: TransferFunctions>(
    transfer: &T,
    initial_state: T::State,
    func: &Function,
) -> StateMap<T::State> {
    let mut state_map: StateMap<T::State> = StateMap::new();
    let mut work_list = VecDeque::new();

    // Seed all blocks so every block appears in the state map.
    // The entry block gets the caller-provided initial state; all others
    // start at bottom and are refined by the fixpoint.
    let bottom = T::State::default();
    for &label in func.basic_blocks.keys() {
        let pre = if label == func.entry_label {
            initial_state.clone()
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
    // Also seed all blocks onto the worklist so each is processed at least once.
    for &label in func.basic_blocks.keys() {
        if !work_list.contains(&label) {
            work_list.push_back(label);
        }
    }

    while let Some(label) = work_list.pop_front() {
        let pre = state_map[&label].pre.clone();
        let block = &func.basic_blocks[&label];
        let post = execute_block_forward(transfer, block, pre);

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

fn execute_block_forward<T: TransferFunctions>(
    transfer: &T,
    block: &BasicBlock,
    mut state: T::State,
) -> T::State {
    for (idx, instr) in block.instructions.iter().enumerate() {
        transfer.execute(&mut state, instr, idx);
    }
    state
}

// =============================================================================
// Backward fixpoint solver

fn analyze_backward<T: TransferFunctions>(
    transfer: &T,
    initial_state: T::State,
    func: &Function,
) -> StateMap<T::State> {
    let preds = compute_predecessors(func);
    let exit_blocks = find_exit_blocks(func);

    let mut state_map: StateMap<T::State> = StateMap::new();
    let mut work_list = VecDeque::new();

    // Seed all blocks so every block has an entry in the state map.
    // Exit blocks start with the initial state; other blocks also start with
    // the initial state (bottom of the lattice) and will be refined by the fixpoint.
    for &label in func.basic_blocks.keys() {
        state_map.insert(
            label,
            BlockState {
                pre: initial_state.clone(),
                post: initial_state.clone(),
            },
        );
    }

    // Seed ALL blocks into the worklist so each is processed at least once.
    // Start with exit blocks, then remaining blocks.  Even blocks whose initial
    // `pre` is bottom need processing: their backward execution may gen facts
    // that must propagate to predecessors.
    for &exit in &exit_blocks {
        work_list.push_back(exit);
    }
    for &label in func.basic_blocks.keys() {
        if !work_list.contains(&label) {
            work_list.push_back(label);
        }
    }

    while let Some(label) = work_list.pop_front() {
        let pre = state_map[&label].pre.clone();
        let block = &func.basic_blocks[&label];
        let post = execute_block_backward(transfer, block, pre);

        // Propagate to predecessors.
        if let Some(pred_list) = preds.get(&label) {
            for &pred in pred_list {
                match state_map.get_mut(&pred) {
                    Some(pred_state) => {
                        if pred_state.pre.join(&post) == JoinResult::Changed
                            && !work_list.contains(&pred)
                        {
                            work_list.push_back(pred);
                        }
                    }
                    None => {
                        state_map.insert(
                            pred,
                            BlockState {
                                pre: post.clone(),
                                post: initial_state.clone(),
                            },
                        );
                        if !work_list.contains(&pred) {
                            work_list.push_back(pred);
                        }
                    }
                }
            }
        }
        state_map.get_mut(&label).unwrap().post = post;
    }
    state_map
}

fn execute_block_backward<T: TransferFunctions>(
    transfer: &T,
    block: &BasicBlock,
    mut state: T::State,
) -> T::State {
    for idx in (0..block.instructions.len()).rev() {
        transfer.execute(&mut state, &block.instructions[idx], idx);
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::SetDomain;

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

    struct CountingForward;

    impl TransferFunctions for CountingForward {
        type State = SetDomain<usize>;
        const BACKWARD: bool = false;

        fn execute(&self, state: &mut Self::State, _instr: &Instruction, idx: usize) {
            state.insert(idx);
        }
    }

    struct CountingBackward;

    impl TransferFunctions for CountingBackward {
        type State = SetDomain<usize>;
        const BACKWARD: bool = true;

        fn execute(&self, state: &mut Self::State, _instr: &Instruction, idx: usize) {
            state.insert(idx);
        }
    }

    #[test]
    fn test_forward_single_block() {
        let func = make_function(
            vec![(0, vec![Instruction::Nop, Instruction::Return(vec![])])],
            0,
        );
        let result = CountingForward.analyze(SetDomain::default(), &func);
        assert_eq!(result.len(), 1);
        assert!(result[&0].post.contains(&0));
        assert!(result[&0].post.contains(&1));
    }

    #[test]
    fn test_backward_single_block() {
        let func = make_function(
            vec![(0, vec![Instruction::Nop, Instruction::Return(vec![])])],
            0,
        );
        let result = CountingBackward.analyze(SetDomain::default(), &func);
        assert_eq!(result.len(), 1);
        // Backward: post = state at beginning of block after backward processing
        assert!(result[&0].post.contains(&0));
        assert!(result[&0].post.contains(&1));
    }

    #[test]
    fn test_forward_diamond() {
        // Entry(0) -> JumpIf -> block 1 or block 2 -> both jump to block 3 (exit)
        let func = make_function(
            vec![
                (
                    0,
                    vec![Instruction::JumpIf {
                        condition: move_stackless_bytecode_2::ast::Trivial::Immediate(
                            move_core_types::runtime_value::MoveValue::Bool(true),
                        ),
                        then_label: 1,
                        else_label: 2,
                    }],
                ),
                (1, vec![Instruction::Jump(3)]),
                (2, vec![Instruction::Jump(3)]),
                (3, vec![Instruction::Return(vec![])]),
            ],
            0,
        );
        let result = CountingForward.analyze(SetDomain::default(), &func);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_backward_diamond() {
        let func = make_function(
            vec![
                (
                    0,
                    vec![Instruction::JumpIf {
                        condition: move_stackless_bytecode_2::ast::Trivial::Immediate(
                            move_core_types::runtime_value::MoveValue::Bool(true),
                        ),
                        then_label: 1,
                        else_label: 2,
                    }],
                ),
                (1, vec![Instruction::Jump(3)]),
                (2, vec![Instruction::Jump(3)]),
                (3, vec![Instruction::Return(vec![])]),
            ],
            0,
        );
        let result = CountingBackward.analyze(SetDomain::default(), &func);
        // All 4 blocks should be reached backward from exit.
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_forward_loop_converges() {
        // block 0 -> block 1 -> JumpIf -> block 1 (loop) or block 2 (exit)
        let func = make_function(
            vec![
                (0, vec![Instruction::Jump(1)]),
                (
                    1,
                    vec![Instruction::JumpIf {
                        condition: move_stackless_bytecode_2::ast::Trivial::Immediate(
                            move_core_types::runtime_value::MoveValue::Bool(true),
                        ),
                        then_label: 1,
                        else_label: 2,
                    }],
                ),
                (2, vec![Instruction::Return(vec![])]),
            ],
            0,
        );
        let result = CountingForward.analyze(SetDomain::default(), &func);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_predecessors() {
        let func = make_function(
            vec![
                (0, vec![Instruction::Jump(1)]),
                (1, vec![Instruction::Jump(2)]),
                (2, vec![Instruction::Return(vec![])]),
            ],
            0,
        );
        let preds = compute_predecessors(&func);
        assert!(preds[&0].is_empty());
        assert_eq!(preds[&1], vec![0]);
        assert_eq!(preds[&2], vec![1]);
    }

    #[test]
    fn test_state_per_instruction_forward() {
        let func = make_function(
            vec![(
                0,
                vec![Instruction::Nop, Instruction::Nop, Instruction::Return(vec![])],
            )],
            0,
        );
        let state_map = CountingForward.analyze(SetDomain::default(), &func);
        let per_instr =
            CountingForward.state_per_instruction(&state_map, &func, |before, after| {
                (before.len(), after.len())
            });
        // Instruction 0: before=0 items, after=1 item
        assert_eq!(per_instr[&(0, 0)], (0, 1));
        // Instruction 1: before=1 item, after=2 items
        assert_eq!(per_instr[&(0, 1)], (1, 2));
    }
}
