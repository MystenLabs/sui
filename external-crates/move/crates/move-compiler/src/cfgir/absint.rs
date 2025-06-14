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

pub fn analyze_function<C: CFG, TF: TransferFunctions>(
    transfer_functions: &mut TF,
    cfg: &C,
    initial_state: TF::State,
) -> (BTreeMap<Label, TF::State>, Diagnostics) {
    fn collect_states_and_diagnostics<State>(
        map: BTreeMap<Label, absint::BlockInvariant<(State, Option<Rc<Diagnostics>>)>>,
    ) -> (BTreeMap<Label, State>, Diagnostics) {
        let mut diags = Diagnostics::new();
        let final_states = map
            .into_iter()
            .map(|(lbl, inv)| {
                let absint::BlockInvariant {
                    pre: (pre, _pre_diags),
                    post,
                } = inv;
                debug_assert!(_pre_diags.is_none());
                // can be None for empty blocks
                if let absint::BlockPostCondition::Processed((_, Some(rc_diags))) = post {
                    diags.extend(Rc::into_inner(rc_diags).unwrap());
                }
                (lbl, pre)
            })
            .collect();
        (final_states, diags)
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
        _block_id: Self::BlockId,
        invariant: &mut absint::BlockInvariant<(TF::State, Option<Rc<Diagnostics>>)>,
    ) -> Result<(), Self::Error> {
        // each block needs its own unique Diagnostics, so set to None to be then initialized
        // in the `execute`
        invariant.pre.1 = None;
        Ok(())
    }

    fn visit_block_post_execution(
        &mut self,
        _block_id: Self::BlockId,
        _invariant: &mut absint::BlockInvariant<Self::State>,
    ) -> Result<(), Self::Error> {
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
