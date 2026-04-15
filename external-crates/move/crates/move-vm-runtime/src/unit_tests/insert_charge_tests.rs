// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for the `optimization::insert_charge` pass that computes
//! per-block fixed gas costs and inserts the synthetic `Charge` instruction.

use crate::jit::optimization::ast::{Bytecode, ChargeInfo};
use crate::jit::optimization::insert_charge::compute_block_fixed_costs;
use move_binary_format::file_format::{FieldHandleIndex, FieldInstantiationIndex};

fn assert_cost(code: &[Bytecode], expected_instrs: u64, expected_pushes: u64, expected_pops: u64) {
    let cost = compute_block_fixed_costs(code);
    assert_eq!(
        cost.instructions(),
        expected_instrs,
        "instructions: expected {}, got {}",
        expected_instrs,
        cost.instructions()
    );
    assert_eq!(
        cost.pushes(),
        expected_pushes,
        "pushes: expected {}, got {}",
        expected_pushes,
        cost.pushes()
    );
    assert_eq!(
        cost.pops(),
        expected_pops,
        "pops: expected {}, got {}",
        expected_pops,
        cost.pops()
    );
}

#[test]
fn test_all_fixed_arithmetic() {
    // Each arithmetic op: 2 pops, 1 push
    let code = vec![
        Bytecode::Add,
        Bytecode::Sub,
        Bytecode::Mul,
        Bytecode::Div,
        Bytecode::Mod,
    ];
    assert_cost(&code, 5, 5, 10);
}

#[test]
fn test_all_variable_cost() {
    let code = vec![
        Bytecode::CopyLoc(0),
        Bytecode::MoveLoc(0),
        Bytecode::StLoc(0),
        Bytecode::Pop,
    ];
    let cost = compute_block_fixed_costs(&code);
    assert_eq!(cost.instructions(), 0);
    assert!(!cost.has_fixed_costs());
}

#[test]
fn test_mixed_instructions() {
    // LdU64(42): 0 pops, 1 push
    // LdU64(7):  0 pops, 1 push
    // Add:       2 pops, 1 push
    // StLoc(0):  variable-cost, skipped
    let code = vec![
        Bytecode::LdU64(42),
        Bytecode::LdU64(7),
        Bytecode::Add,
        Bytecode::StLoc(0),
    ];
    assert_cost(&code, 3, 3, 2);
}

#[test]
fn test_loads_and_booleans() {
    let code = vec![
        Bytecode::LdU8(1),
        Bytecode::LdU64(2),
        Bytecode::LdTrue,
        Bytecode::LdFalse,
    ];
    assert_cost(&code, 4, 4, 0);
}

#[test]
fn test_branches() {
    // BrTrue: 1 pop, 0 push
    // BrFalse: 1 pop, 0 push
    // Branch: 0 pop, 0 push
    let code = vec![
        Bytecode::BrTrue(5),
        Bytecode::BrFalse(3),
        Bytecode::Branch(0),
    ];
    assert_cost(&code, 3, 0, 2);
}

#[test]
fn test_comparisons_and_boolean_ops() {
    // Lt: 2 pops, 1 push
    // Gt: 2 pops, 1 push
    // Or: 2 pops, 1 push
    // And: 2 pops, 1 push
    // Not: 1 pop, 1 push
    let code = vec![
        Bytecode::Lt,
        Bytecode::Gt,
        Bytecode::Or,
        Bytecode::And,
        Bytecode::Not,
    ];
    assert_cost(&code, 5, 5, 9);
}

#[test]
fn test_casts() {
    // Each cast: 1 pop, 1 push
    let code = vec![Bytecode::CastU8, Bytecode::CastU64, Bytecode::CastU256];
    assert_cost(&code, 3, 3, 3);
}

#[test]
fn test_empty_block() {
    let cost = compute_block_fixed_costs(&[]);
    assert_eq!(cost.instructions(), 0);
    assert!(!cost.has_fixed_costs());
}

#[test]
fn test_single_ret() {
    // Ret: 0 pops, 0 pushes
    let code = vec![Bytecode::Ret];
    assert_cost(&code, 1, 0, 0);
}

#[test]
fn test_charge_ignored() {
    let code = vec![Bytecode::Charge(Box::new(ChargeInfo {
        instructions: 99,
        pushes: 99,
        pops: 99,
        push_size: 99,
        pop_size: 99,
    }))];
    let cost = compute_block_fixed_costs(&code);
    assert_eq!(cost.instructions(), 0);
    assert!(!cost.has_fixed_costs());
}

#[test]
fn test_reference_ops() {
    // FreezeRef: 1 pop, 1 push
    // MutBorrowLoc: 0 pops, 1 push
    // ImmBorrowLoc: 0 pops, 1 push
    let code = vec![
        Bytecode::FreezeRef,
        Bytecode::MutBorrowLoc(0),
        Bytecode::ImmBorrowLoc(1),
    ];
    assert_cost(&code, 3, 3, 1);
}

#[test]
fn test_bitwise_ops() {
    // Each: 2 pops, 1 push
    let code = vec![
        Bytecode::BitOr,
        Bytecode::BitAnd,
        Bytecode::Xor,
        Bytecode::Shl,
        Bytecode::Shr,
    ];
    assert_cost(&code, 5, 5, 10);
}

#[test]
fn test_abort() {
    // Abort: 1 pop, 0 pushes
    let code = vec![Bytecode::Abort];
    assert_cost(&code, 1, 0, 1);
}

#[test]
fn test_field_borrow_ops() {
    // MutBorrowField: 1 pop, 1 push
    // ImmBorrowField: 1 pop, 1 push
    let code = vec![
        Bytecode::MutBorrowField(FieldHandleIndex::new(0)),
        Bytecode::ImmBorrowField(FieldHandleIndex::new(0)),
        Bytecode::MutBorrowFieldGeneric(FieldInstantiationIndex::new(0)),
        Bytecode::ImmBorrowFieldGeneric(FieldInstantiationIndex::new(0)),
    ];
    assert_cost(&code, 4, 4, 4);
}

#[test]
fn test_all_load_sizes() {
    let code = vec![
        Bytecode::LdU8(0),
        Bytecode::LdU16(0),
        Bytecode::LdU32(0),
        Bytecode::LdU64(0),
        Bytecode::LdU128(Box::new(0)),
        Bytecode::LdU256(Box::new(move_core_types::u256::U256::zero())),
    ];
    assert_cost(&code, 6, 6, 0);
}

#[test]
fn test_all_comparison_ops() {
    // Each: 2 pops, 1 push
    let code = vec![Bytecode::Lt, Bytecode::Gt, Bytecode::Le, Bytecode::Ge];
    assert_cost(&code, 4, 4, 8);
}

#[test]
fn test_eq_neq_are_variable_cost() {
    let code = vec![Bytecode::Eq, Bytecode::Neq];
    let cost = compute_block_fixed_costs(&code);
    assert_eq!(cost.instructions(), 0);
    assert!(!cost.has_fixed_costs());
}

#[test]
fn test_nop() {
    let code = vec![Bytecode::Nop, Bytecode::Nop, Bytecode::Nop];
    assert_cost(&code, 3, 0, 0);
}
