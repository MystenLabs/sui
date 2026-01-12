// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Peephole optimization pass for fusing common instruction sequences into super-instructions.
//!
//! This pass recognizes frequently occurring instruction pairs and replaces them with
//! single super-instructions that perform the same operation more efficiently.

use crate::jit::optimization::ast::{Bytecode, Code, Function, Label, Module, Package};
use std::collections::BTreeMap;

/// Apply peephole optimizations to an entire package.
pub fn optimize_package(mut package: Package) -> Package {
    for module in package.modules.values_mut() {
        optimize_module(module);
    }
    package
}

/// Apply peephole optimizations to a module.
fn optimize_module(module: &mut Module) {
    for function in module.functions.values_mut() {
        optimize_function(function);
    }
}

/// Apply peephole optimizations to a function.
fn optimize_function(function: &mut Function) {
    if let Some(code) = &mut function.code {
        optimize_code(code);
    }
}

/// Apply peephole optimizations to function code.
fn optimize_code(code: &mut Code) {
    // Optimize each basic block
    let mut new_blocks: BTreeMap<Label, Vec<Bytecode>> = BTreeMap::new();

    for (label, block) in &code.code {
        let optimized_block = optimize_block(block);
        new_blocks.insert(*label, optimized_block);
    }

    code.code = new_blocks;
}

/// Apply peephole optimizations to a basic block.
/// Returns the optimized block.
fn optimize_block(block: &[Bytecode]) -> Vec<Bytecode> {
    let mut result = Vec::with_capacity(block.len());
    let mut i = 0;

    while i < block.len() {
        // Try to match instruction pairs for super-instruction fusion
        if i + 1 < block.len() {
            if let Some(super_instr) = try_fuse(&block[i], &block[i + 1]) {
                result.push(super_instr);
                i += 2; // Skip both instructions
                continue;
            }
        }

        // No fusion possible, copy the instruction as-is
        result.push(block[i].clone());
        i += 1;
    }

    result
}

/// Try to fuse two consecutive instructions into a super-instruction.
/// Returns Some(super_instruction) if fusion is possible, None otherwise.
fn try_fuse(first: &Bytecode, second: &Bytecode) -> Option<Bytecode> {
    match (first, second) {
        // MoveLoc -> Pop => MoveLocPop
        // This pattern occurs when a value is moved just to be discarded
        (Bytecode::MoveLoc(idx), Bytecode::Pop) => Some(Bytecode::MoveLocPop(*idx)),

        // NOTE: CallGeneric -> StLoc fusion is NOT implemented because it doesn't work well
        // with the VM's call stack model. For non-native calls, a new frame is pushed and
        // the result isn't available until the callee returns. This would require complex
        // bookkeeping to remember the pending StLoc operation.

        // ImmBorrowField -> ReadRef => ImmBorrowFieldReadRef
        // This pattern occurs when reading a field value through a reference
        (Bytecode::ImmBorrowField(field_idx), Bytecode::ReadRef) => {
            Some(Bytecode::ImmBorrowFieldReadRef(*field_idx))
        }

        // CopyLoc -> FreezeRef => CopyLocFreezeRef
        // This pattern occurs when copying a mutable reference and freezing it
        (Bytecode::CopyLoc(idx), Bytecode::FreezeRef) => Some(Bytecode::CopyLocFreezeRef(*idx)),

        // BrFalse -> Branch => BrFalseBranch
        // This pattern is the "else" branch pattern in if-else constructs
        // Note: We need to be careful here - this is only safe within a basic block
        // and the Branch must be the end of the block (which it always is)
        (Bytecode::BrFalse(false_target), Bytecode::Branch(true_target)) => {
            Some(Bytecode::BrFalseBranch(*false_target, *true_target))
        }

        // No fusion possible for other instruction pairs
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_binary_format::file_format::FieldHandleIndex;

    // -------------------------------------------------------------------------
    // MoveLocPop tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_move_loc_pop_fusion() {
        let block = vec![Bytecode::MoveLoc(5), Bytecode::Pop];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], Bytecode::MoveLocPop(5)));
    }

    #[test]
    fn test_move_loc_pop_preserves_index() {
        for idx in [0, 1, 127, 255] {
            let block = vec![Bytecode::MoveLoc(idx), Bytecode::Pop];
            let result = optimize_block(&block);
            assert_eq!(result.len(), 1);
            assert!(matches!(result[0], Bytecode::MoveLocPop(i) if i == idx));
        }
    }

    // -------------------------------------------------------------------------
    // ImmBorrowFieldReadRef tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_imm_borrow_field_read_ref_fusion() {
        let field_idx = FieldHandleIndex::new(2);
        let block = vec![Bytecode::ImmBorrowField(field_idx), Bytecode::ReadRef];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], Bytecode::ImmBorrowFieldReadRef(f) if f == field_idx));
    }

    #[test]
    fn test_mut_borrow_field_read_ref_no_fusion() {
        // MutBorrowField + ReadRef should NOT fuse (only ImmBorrowField does)
        let field_idx = FieldHandleIndex::new(2);
        let block = vec![Bytecode::MutBorrowField(field_idx), Bytecode::ReadRef];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 2);
    }

    // -------------------------------------------------------------------------
    // CopyLocFreezeRef tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_copy_loc_freeze_ref_fusion() {
        let block = vec![Bytecode::CopyLoc(10), Bytecode::FreezeRef];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], Bytecode::CopyLocFreezeRef(10)));
    }

    #[test]
    fn test_move_loc_freeze_ref_no_fusion() {
        // MoveLoc + FreezeRef should NOT fuse (only CopyLoc does)
        let block = vec![Bytecode::MoveLoc(10), Bytecode::FreezeRef];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 2);
    }

    // -------------------------------------------------------------------------
    // BrFalseBranch tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_br_false_branch_fusion() {
        let block = vec![Bytecode::BrFalse(5), Bytecode::Branch(10)];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], Bytecode::BrFalseBranch(5, 10)));
    }

    #[test]
    fn test_br_true_branch_no_fusion() {
        // BrTrue + Branch should NOT fuse (only BrFalse does)
        let block = vec![Bytecode::BrTrue(5), Bytecode::Branch(10)];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 2);
    }

    // -------------------------------------------------------------------------
    // General tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_no_fusion_when_not_adjacent() {
        let block = vec![Bytecode::MoveLoc(5), Bytecode::LdU64(42), Bytecode::Pop];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_multiple_fusions_in_block() {
        let block = vec![
            Bytecode::MoveLoc(1),
            Bytecode::Pop,
            Bytecode::MoveLoc(2),
            Bytecode::Pop,
        ];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], Bytecode::MoveLocPop(1)));
        assert!(matches!(result[1], Bytecode::MoveLocPop(2)));
    }

    #[test]
    fn test_mixed_fusions_in_block() {
        let field_idx = FieldHandleIndex::new(2);
        let block = vec![
            Bytecode::MoveLoc(0),
            Bytecode::Pop,
            Bytecode::CopyLoc(5),
            Bytecode::FreezeRef,
            Bytecode::ImmBorrowField(field_idx),
            Bytecode::ReadRef,
        ];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], Bytecode::MoveLocPop(0)));
        assert!(matches!(result[1], Bytecode::CopyLocFreezeRef(5)));
        assert!(matches!(result[2], Bytecode::ImmBorrowFieldReadRef(f) if f == field_idx));
    }

    #[test]
    fn test_partial_fusion_at_block_end() {
        // If the second instruction of a pair is at the end, we can still fuse
        let block = vec![Bytecode::CopyLoc(5), Bytecode::FreezeRef];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], Bytecode::CopyLocFreezeRef(5)));
    }

    #[test]
    fn test_no_fusion_single_instruction() {
        let block = vec![Bytecode::MoveLoc(5)];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], Bytecode::MoveLoc(5)));
    }

    #[test]
    fn test_empty_block() {
        let block: Vec<Bytecode> = vec![];
        let result = optimize_block(&block);
        assert!(result.is_empty());
    }

    #[test]
    fn test_overlapping_patterns_greedy() {
        // Test that we process greedily from left to right
        // MoveLoc(1), Pop, MoveLoc(2), Pop should become MoveLocPop(1), MoveLocPop(2)
        // not MoveLoc(1), something weird
        let block = vec![
            Bytecode::MoveLoc(1),
            Bytecode::Pop,
            Bytecode::MoveLoc(2),
            Bytecode::Pop,
        ];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], Bytecode::MoveLocPop(1)));
        assert!(matches!(result[1], Bytecode::MoveLocPop(2)));
    }

    #[test]
    fn test_alternating_fusable_unfusable() {
        let block = vec![
            Bytecode::MoveLoc(1),
            Bytecode::Pop, // Fuses with previous
            Bytecode::LdU64(42),
            Bytecode::Pop, // Does NOT fuse (LdU64+Pop is not a pattern)
            Bytecode::MoveLoc(2),
            Bytecode::Pop, // Fuses with previous
        ];
        let result = optimize_block(&block);
        assert_eq!(result.len(), 4);
        assert!(matches!(result[0], Bytecode::MoveLocPop(1)));
        assert!(matches!(result[1], Bytecode::LdU64(42)));
        assert!(matches!(result[2], Bytecode::Pop));
        assert!(matches!(result[3], Bytecode::MoveLocPop(2)));
    }
}
