// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::cfg::CFG;
use crate::{diagnostics::Diagnostics, hlir::ast::*};
use move_abstract_interpreter::absint;
use std::{collections::BTreeMap, rc::Rc};

pub use absint::JoinResult;

/// Trait for finite-height abstract domains. Infinite height domains would require a more complex
/// trait with widening and a partial order.
pub trait AbstractDomain: Clone + Sized {
    fn join(&mut self, other: &Self) -> JoinResult;
}

/// Take a pre-state + instruction and mutate it to produce a post-state
/// Auxiliary data can be stored in self.
pub trait TransferFunctions {
    type State: AbstractDomain;

    /// Called before any commands in the block are executed, with the block's pre-state.
    fn start_block(&mut self, _label: Label, _pre: &Self::State) {}

    /// Called after all commands in the block have been executed, with the block's post-state.
    fn finish_block(&mut self, _label: Label, _post: &Self::State) {}

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
        lbl: Label,
        idx: usize,
        command: &Command,
    ) -> Diagnostics;
}

/// The pre- and post-states of a block, as observed at the fixpoint of the analysis. The
/// post-state is `None` if the block was never processed (e.g. unreachable).
pub struct BlockStates<State> {
    pub pre: State,
    pub post: Option<State>,
}

pub fn analyze_function<C: CFG, TF: TransferFunctions>(
    transfer_functions: &mut TF,
    cfg: &C,
    initial_state: TF::State,
) -> (BTreeMap<Label, TF::State>, Diagnostics) {
    let (states, diags) = analyze_function_with_post_states(transfer_functions, cfg, initial_state);
    let pre_states = states
        .into_iter()
        .map(|(lbl, BlockStates { pre, .. })| (lbl, pre))
        .collect();
    (pre_states, diags)
}

/// Like [`analyze_function`] but exposes both the pre- and post-state of every block.
pub fn analyze_function_with_post_states<C: CFG, TF: TransferFunctions>(
    transfer_functions: &mut TF,
    cfg: &C,
    initial_state: TF::State,
) -> (BTreeMap<Label, BlockStates<TF::State>>, Diagnostics) {
    fn collect_states_and_diagnostics<State>(
        map: BTreeMap<Label, absint::BlockInvariant<(State, Option<Rc<Diagnostics>>)>>,
    ) -> (BTreeMap<Label, BlockStates<State>>, Diagnostics) {
        let mut diags = Diagnostics::new();
        let states = map
            .into_iter()
            .map(|(lbl, inv)| {
                let absint::BlockInvariant {
                    pre: (pre, _pre_diags),
                    post,
                } = inv;
                debug_assert!(_pre_diags.is_none());
                // `post` is `Unprocessed` for empty / unreachable blocks.
                let post = match post {
                    absint::BlockPostCondition::Processed((post, post_diags)) => {
                        if let Some(rc_diags) = post_diags {
                            diags.extend(Rc::into_inner(rc_diags).unwrap());
                        }
                        Some(post)
                    }
                    absint::BlockPostCondition::Unprocessed => None,
                };
                (lbl, BlockStates { pre, post })
            })
            .collect();
        (states, diags)
    }

    let mut interpreter = AbstractInterpreter { transfer_functions };
    let inv_map = absint::analyze_function(
        &mut interpreter,
        &CFGWrapper(cfg),
        &(),
        (initial_state, None),
    )
    .unwrap();
    collect_states_and_diagnostics(inv_map)
}

struct AbstractInterpreter<'a, TF: TransferFunctions> {
    pub transfer_functions: &'a mut TF,
}

impl<TF: TransferFunctions> absint::AbstractInterpreter for AbstractInterpreter<'_, TF> {
    type Error = ();
    type BlockId = Label;

    type State = (<TF as TransferFunctions>::State, Option<Rc<Diagnostics>>);

    type InstructionIndex = usize;
    type Instruction = Command;

    fn start(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn join(
        &mut self,
        (pre, _): &mut (TF::State, Option<Rc<Diagnostics>>),
        (post, _): &(TF::State, Option<Rc<Diagnostics>>),
    ) -> Result<JoinResult, Self::Error> {
        Ok(pre.join(post))
    }

    fn visit_block_pre_execution(
        &mut self,
        block_id: Self::BlockId,
        invariant: &mut absint::BlockInvariant<(TF::State, Option<Rc<Diagnostics>>)>,
    ) -> Result<(), Self::Error> {
        // each block needs its own unique Diagnostics, so set to None to be then initialized
        // in the `execute`
        invariant.pre.1 = None;
        self.transfer_functions
            .start_block(block_id, &invariant.pre.0);
        Ok(())
    }

    fn visit_block_post_execution(
        &mut self,
        block_id: Self::BlockId,
        invariant: &mut absint::BlockInvariant<Self::State>,
    ) -> Result<(), Self::Error> {
        if let absint::BlockPostCondition::Processed((post, _)) = &invariant.post {
            self.transfer_functions.finish_block(block_id, post);
        }
        Ok(())
    }

    fn visit_successor(&mut self, _block_id: Self::BlockId) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_back_edge(
        &mut self,
        _from: Self::BlockId,
        _to: Self::BlockId,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn execute(
        &mut self,
        lbl: Self::BlockId,
        (start, _stop): (Self::InstructionIndex, Self::InstructionIndex),
        (state, diags): &mut (TF::State, Option<Rc<Diagnostics>>),
        idx: Self::InstructionIndex,
        command: &Self::Instruction,
    ) -> Result<(), Self::Error> {
        if idx == start {
            *diags = Some(Rc::new(Diagnostics::new()));
        }
        let diags: &mut Diagnostics = Rc::get_mut(diags.as_mut().expect("Set as some"))
            .expect("unique ownership while iterating through block");
        diags.extend(self.transfer_functions.execute(state, lbl, idx, command));
        Ok(())
    }
}

struct CFGWrapper<'a, T: CFG>(&'a T);
impl<T: CFG> move_abstract_interpreter::control_flow_graph::ControlFlowGraph for CFGWrapper<'_, T> {
    type BlockId = Label;
    type InstructionIndex = usize;
    type Instruction = Command;
    type Instructions = ();

    fn block_start(&self, label: Self::BlockId) -> Self::InstructionIndex {
        self.0.block_start(label)
    }

    fn block_end(&self, label: Self::BlockId) -> Self::InstructionIndex {
        self.0.block_end(label)
    }

    fn successors(&self, label: Self::BlockId) -> impl Iterator<Item = Self::BlockId> {
        self.0.successors(label).iter().copied()
    }

    fn next_block(&self, label: Self::BlockId) -> Option<Self::BlockId> {
        self.0.next_block(label)
    }

    fn instructions<'a>(
        &'a self,
        _: &'a (),
        block_id: Self::BlockId,
    ) -> impl Iterator<Item = (Self::InstructionIndex, &'a Self::Instruction)>
    where
        Self::Instruction: 'a,
    {
        self.0.commands(block_id)
    }

    fn blocks(&self) -> impl Iterator<Item = Self::BlockId> {
        self.0.block_labels()
    }

    fn num_blocks(&self) -> usize {
        self.0.num_blocks()
    }

    fn entry_block_id(&self) -> Self::BlockId {
        self.0.start_block()
    }

    fn is_loop_head(&self, label: Self::BlockId) -> bool {
        self.0.is_loop_head(label)
    }

    fn is_back_edge(&self, cur: Self::BlockId, next: Self::BlockId) -> bool {
        self.0.is_back_edge(cur, next)
    }
}
