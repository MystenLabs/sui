// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_abstract_interpreter::control_flow_graph::{
    BlockId, ControlFlowGraph, VMControlFlowGraph,
};
use move_binary_format::{
    errors::PartialVMResult,
    file_format::{
        AbilitySet, Bytecode, CodeOffset, CodeUnit, FunctionDefinitionIndex, FunctionHandle,
        Signature,
    },
    CompiledModule,
};
use move_bytecode_verifier_meter::{Meter, Scope};
use std::collections::BTreeMap;

/// A `FunctionContext` holds all the information needed by the verifier for `FunctionDefinition`.`
/// A control flow graph is built for a function when the `FunctionContext` is created.
pub struct FunctionContext<'a> {
    index: Option<FunctionDefinitionIndex>,
    code: &'a CodeUnit,
    parameters: &'a Signature,
    return_: &'a Signature,
    locals: &'a Signature,
    type_parameters: &'a [AbilitySet],
    cfg: VMControlFlowGraph,
}

/// Trait for finite-height abstract domains. Infinite height domains would require a more complex
/// trait with widening and a partial order.
pub trait AbstractDomain: Clone + Sized {
    fn join(
        &mut self,
        other: &Self,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<JoinResult>;
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

/// Costs for metered verification
const ANALYZE_FUNCTION_BASE_COST: u128 = 10;
const EXECUTE_BLOCK_BASE_COST: u128 = 10;
const PER_BACKEDGE_COST: u128 = 10;
const PER_SUCCESSOR_COST: u128 = 10;

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
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()>;
}

pub trait AbstractInterpreter: TransferFunctions {
    /// Analyze procedure local@function_context starting from pre-state local@initial_state.
    fn analyze_function(
        &mut self,
        initial_state: Self::State,
        function_context: &FunctionContext,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        meter.add(Scope::Function, ANALYZE_FUNCTION_BASE_COST)?;
        let mut inv_map = InvariantMap::new();
        let entry_block_id = function_context.cfg().entry_block_id();
        let mut next_block = Some(entry_block_id);
        inv_map.insert(entry_block_id, BlockInvariant { pre: initial_state });

        while let Some(block_id) = next_block {
            let block_invariant = match inv_map.get_mut(&block_id) {
                Some(invariant) => invariant,
                None => {
                    // This can only happen when all predecessors have errors,
                    // so skip the block and move on to the next one
                    next_block = function_context.cfg().next_block(block_id);
                    continue;
                }
            };

            let pre_state = &block_invariant.pre;
            // Note: this will stop analysis after the first error occurs, to avoid the risk of
            // subsequent crashes
            let post_state = self.execute_block(block_id, pre_state, function_context, meter)?;

            let mut next_block_candidate = function_context.cfg().next_block(block_id);
            // propagate postcondition of this block to successor blocks
            for successor_block_id in function_context.cfg().successors(block_id) {
                meter.add(Scope::Function, PER_SUCCESSOR_COST)?;
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
                                if function_context
                                    .cfg()
                                    .is_back_edge(block_id, *successor_block_id)
                                {
                                    meter.add(Scope::Function, PER_BACKEDGE_COST)?;
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
        function_context: &FunctionContext,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<Self::State> {
        meter.add(Scope::Function, EXECUTE_BLOCK_BASE_COST)?;
        let mut state_acc = pre_state.clone();
        let block_end = function_context.cfg().block_end(block_id);
        for offset in function_context.cfg().instr_indexes(block_id) {
            let instr = &function_context.code().code[offset as usize];
            self.execute(&mut state_acc, instr, offset, block_end, meter)?
        }
        Ok(state_acc)
    }
}

impl<'a> FunctionContext<'a> {
    // Creates a `FunctionContext` for a module function.
    pub fn new(
        module: &'a CompiledModule,
        index: FunctionDefinitionIndex,
        code: &'a CodeUnit,
        function_handle: &'a FunctionHandle,
    ) -> Self {
        Self {
            index: Some(index),
            code,
            parameters: module.signature_at(function_handle.parameters),
            return_: module.signature_at(function_handle.return_),
            locals: module.signature_at(code.locals),
            type_parameters: &function_handle.type_parameters,
            cfg: VMControlFlowGraph::new(&code.code, &code.jump_tables),
        }
    }

    pub fn index(&self) -> Option<FunctionDefinitionIndex> {
        self.index
    }

    pub fn code(&self) -> &CodeUnit {
        self.code
    }

    pub fn parameters(&self) -> &Signature {
        self.parameters
    }

    pub fn return_(&self) -> &Signature {
        self.return_
    }

    pub fn locals(&self) -> &Signature {
        self.locals
    }

    pub fn type_parameters(&self) -> &[AbilitySet] {
        self.type_parameters
    }

    pub fn cfg(&self) -> &VMControlFlowGraph {
        &self.cfg
    }
}
