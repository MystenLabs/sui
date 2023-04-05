// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::meter::Meter;
use move_binary_format::{
    binary_views::FunctionView,
    control_flow_graph::{BlockId, ControlFlowGraph},
    errors::PartialVMResult,
    file_format::{Bytecode, CodeOffset},
};
use std::collections::BTreeMap;

/// Trait for finite-height abstract domains. Infinite height domains would require a more complex
/// trait with widening and a partial order.
pub trait AbstractDomain: Clone + Sized {
    fn join(&mut self, other: &Self, meter: &mut impl Meter) -> PartialVMResult<JoinResult>;
}

#[derive(Debug)]
pub enum JoinResult {
    Changed,
    Unchanged,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct BlockInvariant<State> {
    /// Precondition of the block
    pre: State,
}

/// A map from block id's to the pre/post of each block after a fixed point is reached.
#[allow(dead_code)]
pub type InvariantMap<State> = BTreeMap<BlockId, BlockInvariant<State>>;

/// Take a pre-state + instruction and mutate it to produce a post-state
/// Auxiliary data can be stored in self.
pub trait TransferFunctions {
    type State: AbstractDomain;
    type Error;

    /// Execute local@instr found at index local@index in the current basic block from pre-state
    /// local@pre.
    /// Should return an AnalysisError if executing the instruction is unsuccessful, and () if
    /// the effects of successfully executing local@instr have been reflected by mutatating
    /// local@pre.
    /// Auxilary data from the analysis that is not part of the abstract state can be collected by
    /// mutating local@self.
    /// The last instruction index in the current block is local@last_index. Knowing this
    /// information allows clients to detect the end of a basic block and special-case appropriately
    /// (e.g., normalizing the abstract state before a join).
    fn execute(
        &mut self,
        pre: &mut Self::State,
        instr: &Bytecode,
        index: CodeOffset,
        last_index: CodeOffset,
        meter: &mut impl Meter,
    ) -> PartialVMResult<()>;
}

pub trait AbstractInterpreter: TransferFunctions {
    /// Analyze procedure local@function_view starting from pre-state local@initial_state.
    fn analyze_function(
        &mut self,
        initial_state: Self::State,
        function_view: &FunctionView,
        meter: &mut impl Meter,
    ) -> PartialVMResult<()> {
        let mut inv_map = InvariantMap::new();
        let entry_block_id = function_view.cfg().entry_block_id();
        let mut next_block = Some(entry_block_id);
        inv_map.insert(entry_block_id, BlockInvariant { pre: initial_state });

        while let Some(block_id) = next_block {
            let block_invariant = match inv_map.get_mut(&block_id) {
                Some(invariant) => invariant,
                None => {
                    // This can only happen when all predecessors have errors,
                    // so skip the block and move on to the next one
                    next_block = function_view.cfg().next_block(block_id);
                    continue;
                }
            };

            let pre_state = &block_invariant.pre;
            // Note: this will stop analysis after the first error occurs, to avoid the risk of
            // subsequent crashes
            let post_state = self.execute_block(block_id, pre_state, function_view, meter)?;

            let mut next_block_candidate = function_view.cfg().next_block(block_id);
            // propagate postcondition of this block to successor blocks
            for successor_block_id in function_view.cfg().successors(block_id) {
                match inv_map.get_mut(successor_block_id) {
                    Some(next_block_invariant) => {
                        let join_result = {
                            let old_pre = &mut next_block_invariant.pre;
                            old_pre.join(&post_state, meter)
                        }?;
                        match join_result {
                            JoinResult::Unchanged => {
                                // Pre is the same after join. Reanalyzing this block would produce
                                // the same post
                            }
                            JoinResult::Changed => {
                                // If the cur->successor is a back edge, jump back to the beginning
                                // of the loop, instead of the normal next block
                                if function_view
                                    .cfg()
                                    .is_back_edge(block_id, *successor_block_id)
                                {
                                    next_block_candidate = Some(*successor_block_id);
                                    break;
                                }
                            }
                        }
                    }
                    None => {
                        // Haven't visited the next block yet. Use the post of the current block as
                        // its pre
                        inv_map.insert(
                            *successor_block_id,
                            BlockInvariant {
                                pre: post_state.clone(),
                            },
                        );
                    }
                }
            }
            next_block = next_block_candidate;
        }
        Ok(())
    }

    fn execute_block(
        &mut self,
        block_id: BlockId,
        pre_state: &Self::State,
        function_view: &FunctionView,
        meter: &mut impl Meter,
    ) -> PartialVMResult<Self::State> {
        let mut state_acc = pre_state.clone();
        let block_end = function_view.cfg().block_end(block_id);
        for offset in function_view.cfg().instr_indexes(block_id) {
            let instr = &function_view.code().code[offset as usize];
            self.execute(&mut state_acc, instr, offset, block_end, meter)?
        }
        Ok(state_acc)
    }
}
