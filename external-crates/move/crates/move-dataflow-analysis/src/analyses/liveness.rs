// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Live variables analysis (backward).
//!
//! A local variable is *live* at a program point if its current value may be read
//! before being overwritten. This is a backward analysis:
//!
//! - **Gen**: instructions that *use* a local add it to the live set.
//! - **Kill**: instructions that *define* a local remove it from the live set.
//!
//! Uses:
//! - `RValue::Local { arg, .. }` inside `AssignReg` — reads local via Move/Copy/Borrow
//! - `StoreLoc { value: Trivial::Register(..), .. }` doesn't use a local, but
//!   `Trivial::Register` isn't a local — it's a register. Locals are only read via `RValue::Local`.
//!
//! Definitions (kills):
//! - `StoreLoc { loc, .. }` — writes to local `loc`

use crate::{
    analysis::{DataflowAnalysis, TransferFunctions},
    domains::SetDomain,
};
use move_stackless_bytecode_2::ast::{Function, Instruction, LocalId, RValue};

/// The abstract state: set of locals that are live.
pub type LivenessState = SetDomain<LocalId>;

/// Liveness transfer functions (backward analysis).
pub struct Liveness;

impl TransferFunctions for Liveness {
    type State = LivenessState;
    const BACKWARD: bool = true;

    fn execute(&self, state: &mut Self::State, instr: &Instruction, _idx: usize) {
        // Backward: we see instructions from bottom to top.
        // Kill before gen: if an instruction both defines and uses a local,
        // the use happens before the def in program order, so the local should
        // be live before this instruction. Process def (kill) first, then use (gen).
        match instr {
            Instruction::StoreLoc { loc, .. } => {
                // Kill: this defines the local.
                state.remove(loc);
            }
            Instruction::AssignReg { rhs, .. } => {
                // Check if rhs uses a local.
                if let RValue::Local { arg, .. } = rhs {
                    // Gen: the local is used here, so it must be live before.
                    state.insert(*arg);
                }
            }
            _ => {}
        }
    }
}

/// Run liveness analysis on a function, returning per-block pre/post states.
///
/// For backward analysis, `pre` in the state map is the state at the *end* of a block
/// (input to backward processing), and `post` is the state at the *beginning* of the block
/// (result of backward processing).
pub fn analyze(func: &Function) -> crate::analysis::StateMap<LivenessState> {
    Liveness.analyze(LivenessState::default(), func)
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_stackless_bytecode_2::ast::*;
    use std::rc::Rc;

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

    fn u64_type() -> Rc<move_binary_format::normalized::Type<move_symbol_pool::Symbol>> {
        Rc::new(move_binary_format::normalized::Type::U64)
    }

    fn make_reg(id: RegId) -> Register {
        Register {
            name: id,
            ty: u64_type(),
        }
    }

    fn copy_local(loc: LocalId) -> Instruction {
        Instruction::AssignReg {
            lhs: vec![make_reg(100)],
            rhs: RValue::Local {
                op: LocalOp::Copy,
                arg: loc,
            },
        }
    }

    fn move_local(loc: LocalId) -> Instruction {
        Instruction::AssignReg {
            lhs: vec![make_reg(100)],
            rhs: RValue::Local {
                op: LocalOp::Move,
                arg: loc,
            },
        }
    }

    fn store_local(loc: LocalId) -> Instruction {
        Instruction::StoreLoc {
            loc,
            value: trivial_imm(),
        }
    }

    #[test]
    fn test_use_makes_live() {
        // Block 0: CopyLocal(0), Return
        // Local 0 should be live at the beginning.
        let func = make_function(
            vec![(0, vec![copy_local(0), Instruction::Return(vec![])])],
            0,
        );
        let result = analyze(&func);
        let post = &result[&0].post;
        assert!(post.contains(&0));
    }

    #[test]
    fn test_def_kills() {
        // Block 0: StoreLoc(0), Return
        // Local 0 is defined but never used afterward → not live at beginning.
        let func = make_function(
            vec![(0, vec![store_local(0), Instruction::Return(vec![])])],
            0,
        );
        let result = analyze(&func);
        let post = &result[&0].post;
        assert!(!post.contains(&0));
    }

    #[test]
    fn test_def_then_use() {
        // Block 0: StoreLoc(0), CopyLocal(0), Return
        // The def at StoreLoc(0) kills liveness, but CopyLocal(0) gens it.
        // Backward: see Return (nothing), CopyLocal(0) → gen 0, StoreLoc(0) → kill 0.
        // At beginning: local 0 is NOT live (killed by the store before the use).
        let func = make_function(
            vec![(
                0,
                vec![store_local(0), copy_local(0), Instruction::Return(vec![])],
            )],
            0,
        );
        let result = analyze(&func);
        let post = &result[&0].post;
        assert!(!post.contains(&0));
    }

    #[test]
    fn test_use_then_def() {
        // Block 0: CopyLocal(0), StoreLoc(0), Return
        // Backward: Return → nothing, StoreLoc(0) → kill 0, CopyLocal(0) → gen 0.
        // At beginning: local 0 IS live (used before the store).
        let func = make_function(
            vec![(
                0,
                vec![copy_local(0), store_local(0), Instruction::Return(vec![])],
            )],
            0,
        );
        let result = analyze(&func);
        let post = &result[&0].post;
        assert!(post.contains(&0));
    }

    #[test]
    fn test_diamond_liveness() {
        // Block 0: JumpIf -> 1 or 2
        // Block 1: CopyLocal(0), Jump(3)  — uses local 0
        // Block 2: Jump(3)                — doesn't use local 0
        // Block 3: Return
        // At entry of block 0: local 0 should be live (used on one path).
        let func = make_function(
            vec![
                (
                    0,
                    vec![Instruction::JumpIf {
                        condition: trivial_imm(),
                        then_label: 1,
                        else_label: 2,
                    }],
                ),
                (1, vec![copy_local(0), Instruction::Jump(3)]),
                (2, vec![Instruction::Jump(3)]),
                (3, vec![Instruction::Return(vec![])]),
            ],
            0,
        );
        let result = analyze(&func);
        let post_0 = &result[&0].post;
        assert!(post_0.contains(&0));
    }

    #[test]
    fn test_move_gens_liveness() {
        // Block 0: MoveLocal(0), Return
        // Move uses the local, so it should be live at the beginning.
        let func = make_function(
            vec![(0, vec![move_local(0), Instruction::Return(vec![])])],
            0,
        );
        let result = analyze(&func);
        let post = &result[&0].post;
        assert!(post.contains(&0));
    }
}
