// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::control_flow_graph::ControlFlowGraph;
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum JoinResult {
    Changed,
    Unchanged,
}

pub trait AbstractInterpreter {
    type Error;
    type BlockId: Copy + Ord;

    type State: Clone;
    type InstructionIndex: Copy + Ord;
    type Instruction;

    fn start(&mut self) -> Result<(), Self::Error>;
    fn join(
        &mut self,
        pre: &mut Self::State,
        post: &Self::State,
    ) -> Result<JoinResult, Self::Error>;
    fn visit_block_execution(&mut self, block_id: Self::BlockId) -> Result<(), Self::Error>;
    fn visit_successor(&mut self, block_id: Self::BlockId) -> Result<(), Self::Error>;
    fn visit_back_edge(
        &mut self,
        from: Self::BlockId,
        to: Self::BlockId,
    ) -> Result<(), Self::Error>;

    /// Execute local@instr found at index local@index in the current basic block from pre-state
    /// local@pre.
    /// Should return an Err if executing the instruction is unsuccessful, and () if
    /// the effects of successfully executing local@instr have been reflected by mutating
    /// local@pre.
    /// Auxiliary data from the analysis that is not part of the abstract state can be collected by
    /// mutating local@self.
    /// The last instruction index in the current block is local@last_index. Knowing this
    /// information allows clients to detect the end of a basic block and special-case appropriately
    /// (e.g., normalizing the abstract state before a join).
    fn execute(
        &mut self,
        state: &mut Self::State,
        bounds: (Self::InstructionIndex, Self::InstructionIndex),
        offset: Self::InstructionIndex,
        instr: &Self::Instruction,
    ) -> Result<(), Self::Error>;
}

/// Analyze procedure local@function_context starting from pre-state local@initial_state.
pub fn analyze_function<A, CFG>(
    interpreter: &mut A,
    cfg: &CFG,
    code: &<CFG as ControlFlowGraph>::Instructions,
    initial_state: A::State,
) -> Result<(), A::Error>
where
    A: AbstractInterpreter,
    CFG: ControlFlowGraph<
            BlockId = A::BlockId,
            InstructionIndex = A::InstructionIndex,
            Instruction = A::Instruction,
        >,
{
    interpreter.start()?;
    let mut inv_map = BTreeMap::new();
    let entry_block_id = cfg.entry_block_id();
    let mut next_block = Some(entry_block_id);
    inv_map.insert(entry_block_id, initial_state);

    while let Some(block_id) = next_block {
        let block_invariant = match inv_map.get_mut(&block_id) {
            Some(invariant) => invariant,
            None => {
                // This can only happen when all predecessors have errors,
                // so skip the block and move on to the next one
                next_block = cfg.next_block(block_id);
                continue;
            }
        };

        let pre_state = &block_invariant;
        // Note: this will stop analysis after the first error occurs, to avoid the risk of
        // subsequent crashes
        let post_state = execute_block(interpreter, cfg, code, block_id, pre_state)?;

        let mut next_block_candidate = cfg.next_block(block_id);
        // propagate postcondition of this block to successor blocks
        for &successor_block_id in cfg.successors(block_id) {
            interpreter.visit_successor(successor_block_id)?;
            match inv_map.get_mut(&successor_block_id) {
                Some(next_block_invariant) => {
                    let join_result = interpreter.join(next_block_invariant, &post_state)?;
                    match join_result {
                        JoinResult::Unchanged => {
                            // Pre is the same after join. Reanalyzing this block would produce
                            // the same post
                        }
                        JoinResult::Changed => {
                            // If the cur->successor is a back edge, jump back to the beginning
                            // of the loop, instead of the normal next block
                            if cfg.is_back_edge(block_id, successor_block_id) {
                                interpreter.visit_back_edge(block_id, successor_block_id)?;
                                next_block_candidate = Some(successor_block_id);
                                break;
                            }
                        }
                    }
                }
                None => {
                    // Haven't visited the next block yet. Use the post of the current block as
                    // its pre
                    inv_map.insert(successor_block_id, post_state.clone());
                }
            }
        }
        next_block = next_block_candidate;
    }
    Ok(())
}

fn execute_block<A, CFG>(
    interpreter: &mut A,
    cfg: &CFG,
    code: &<CFG as ControlFlowGraph>::Instructions,
    block_id: A::BlockId,
    pre_state: &A::State,
) -> Result<A::State, A::Error>
where
    A: AbstractInterpreter,
    CFG: ControlFlowGraph<
            BlockId = A::BlockId,
            InstructionIndex = A::InstructionIndex,
            Instruction = A::Instruction,
        >,
{
    interpreter.visit_block_execution(block_id)?;
    let mut state_acc = pre_state.clone();
    let bounds = (cfg.block_start(block_id), cfg.block_end(block_id));
    for (offset, instr) in cfg.instructions(code, block_id) {
        interpreter.execute(&mut state_acc, bounds, offset, instr)?
    }
    Ok(state_acc)
}
