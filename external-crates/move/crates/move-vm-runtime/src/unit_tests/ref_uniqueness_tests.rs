// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::arena::Arena,
    execution::{
        interpreter::{check_reference_args_unique, locals::MachineHeap},
        values::{MemBox, Value},
    },
    jit::execution::ast::ArenaType,
};

fn imm_ref(arena: &Arena) -> ArenaType {
    ArenaType::Reference(arena.alloc_box(ArenaType::U64).unwrap())
}

fn mut_ref(arena: &Arena) -> ArenaType {
    ArenaType::MutableReference(arena.alloc_box(ArenaType::U64).unwrap())
}

#[test]
fn no_references_passes() {
    let args = vec![Value::u64(1), Value::u64(2), Value::bool(true)];
    let params = [ArenaType::U64, ArenaType::U64, ArenaType::Bool];
    assert!(check_reference_args_unique(&args, &params).is_ok());
}

#[test]
fn empty_args_passes() {
    let args: Vec<Value> = vec![];
    let params: [ArenaType; 0] = [];
    assert!(check_reference_args_unique(&args, &params).is_ok());
}

#[test]
fn distinct_mut_references_passes() {
    let arena = Arena::new_bounded();
    let a = MemBox::new(Value::u64(42));
    let b = MemBox::new(Value::u64(42));
    let args = vec![a.as_ref_value(), b.as_ref_value()];
    let params = [mut_ref(&arena), mut_ref(&arena)];
    assert!(check_reference_args_unique(&args, &params).is_ok());
}

#[test]
fn duplicate_mut_reference_fails() {
    let arena = Arena::new_bounded();
    let a = MemBox::new(Value::u64(42));
    let args = vec![a.as_ref_value(), a.as_ref_value()];
    let params = [mut_ref(&arena), mut_ref(&arena)];
    assert!(check_reference_args_unique(&args, &params).is_err());
}

#[test]
fn duplicate_imm_references_passes() {
    let arena = Arena::new_bounded();
    let a = MemBox::new(Value::u64(42));
    let args = vec![a.as_ref_value(), a.as_ref_value()];
    let params = [imm_ref(&arena), imm_ref(&arena)];
    assert!(check_reference_args_unique(&args, &params).is_ok());
}

#[test]
fn mut_ref_aliasing_imm_ref_fails() {
    let arena = Arena::new_bounded();
    let a = MemBox::new(Value::u64(42));
    let args = vec![a.as_ref_value(), a.as_ref_value()];
    let params = [mut_ref(&arena), imm_ref(&arena)];
    assert!(check_reference_args_unique(&args, &params).is_err());
}

#[test]
fn imm_ref_aliasing_mut_ref_fails() {
    let arena = Arena::new_bounded();
    let a = MemBox::new(Value::u64(42));
    let args = vec![a.as_ref_value(), a.as_ref_value()];
    let params = [imm_ref(&arena), mut_ref(&arena)];
    assert!(check_reference_args_unique(&args, &params).is_err());
}

#[test]
fn mixed_values_and_distinct_mut_references_passes() {
    let arena = Arena::new_bounded();
    let a = MemBox::new(Value::u64(1));
    let b = MemBox::new(Value::u64(2));
    let args = vec![
        Value::bool(true),
        a.as_ref_value(),
        Value::u64(99),
        b.as_ref_value(),
    ];
    let params = [
        ArenaType::Bool,
        mut_ref(&arena),
        ArenaType::U64,
        mut_ref(&arena),
    ];
    assert!(check_reference_args_unique(&args, &params).is_ok());
}

#[test]
fn single_mut_reference_passes() {
    let arena = Arena::new_bounded();
    let a = MemBox::new(Value::u64(42));
    let args = vec![a.as_ref_value()];
    let params = [mut_ref(&arena)];
    assert!(check_reference_args_unique(&args, &params).is_ok());
}

#[test]
fn mut_ref_duplicate_among_many_distinct_fails() {
    let arena = Arena::new_bounded();
    let boxes: Vec<_> = (0..10).map(|i| MemBox::new(Value::u64(i))).collect();
    let mut args: Vec<_> = boxes.iter().map(|b| b.as_ref_value()).collect();
    let mut params: Vec<_> = (0..10).map(|_| mut_ref(&arena)).collect();
    // Add a duplicate of the fifth reference
    args.push(boxes[4].as_ref_value());
    params.push(mut_ref(&arena));
    assert!(check_reference_args_unique(&args, &params).is_err());
}

#[test]
fn locals_distinct_mut_references_passes() {
    let arena = Arena::new_bounded();
    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 3).unwrap();
    locals.store_loc(0, Value::u64(10)).unwrap();
    locals.store_loc(1, Value::u64(20)).unwrap();
    locals.store_loc(2, Value::u64(30)).unwrap();
    let args = vec![
        locals.borrow_loc(0).unwrap(),
        locals.borrow_loc(1).unwrap(),
        locals.borrow_loc(2).unwrap(),
    ];
    let params = [mut_ref(&arena), mut_ref(&arena), mut_ref(&arena)];
    assert!(check_reference_args_unique(&args, &params).is_ok());
}

#[test]
fn locals_same_local_mut_ref_fails() {
    let arena = Arena::new_bounded();
    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 1).unwrap();
    locals.store_loc(0, Value::u64(10)).unwrap();
    let args = vec![locals.borrow_loc(0).unwrap(), locals.borrow_loc(0).unwrap()];
    let params = [mut_ref(&arena), mut_ref(&arena)];
    assert!(check_reference_args_unique(&args, &params).is_err());
}

#[test]
fn locals_same_local_imm_ref_passes() {
    let arena = Arena::new_bounded();
    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 1).unwrap();
    locals.store_loc(0, Value::u64(10)).unwrap();
    let args = vec![locals.borrow_loc(0).unwrap(), locals.borrow_loc(0).unwrap()];
    let params = [imm_ref(&arena), imm_ref(&arena)];
    assert!(check_reference_args_unique(&args, &params).is_ok());
}

#[test]
fn many_distinct_mut_refs_passes() {
    let arena = Arena::new_bounded();
    let boxes: Vec<_> = (0..200).map(|i| MemBox::new(Value::u64(i))).collect();
    let args: Vec<_> = boxes.iter().map(|b| b.as_ref_value()).collect();
    let params: Vec<_> = (0..200).map(|_| mut_ref(&arena)).collect();
    assert!(check_reference_args_unique(&args, &params).is_ok());
}

#[test]
fn many_refs_with_one_mut_alias_at_end_fails() {
    let arena = Arena::new_bounded();
    let boxes: Vec<_> = (0..200).map(|i| MemBox::new(Value::u64(i))).collect();
    let mut args: Vec<_> = boxes.iter().map(|b| b.as_ref_value()).collect();
    let mut params: Vec<_> = (0..200).map(|_| mut_ref(&arena)).collect();
    // Alias the last mutable ref with an immutable ref to the same location
    args.push(boxes[199].as_ref_value());
    params.push(imm_ref(&arena));
    assert!(check_reference_args_unique(&args, &params).is_err());
}

#[test]
fn many_refs_with_one_mut_alias_at_start_fails() {
    let arena = Arena::new_bounded();
    let boxes: Vec<_> = (0..200).map(|i| MemBox::new(Value::u64(i))).collect();
    let mut args: Vec<_> = boxes.iter().map(|b| b.as_ref_value()).collect();
    let mut params: Vec<_> = (0..200).map(|_| imm_ref(&arena)).collect();
    // Make the first one mutable and add a duplicate
    params[0] = mut_ref(&arena);
    args.push(boxes[0].as_ref_value());
    params.push(imm_ref(&arena));
    assert!(check_reference_args_unique(&args, &params).is_err());
}

#[test]
fn many_imm_refs_all_aliased_passes() {
    let arena = Arena::new_bounded();
    let shared = MemBox::new(Value::u64(42));
    let args: Vec<_> = (0..200).map(|_| shared.as_ref_value()).collect();
    let params: Vec<_> = (0..200).map(|_| imm_ref(&arena)).collect();
    assert!(check_reference_args_unique(&args, &params).is_ok());
}

#[test]
fn many_imm_refs_one_mut_in_middle_fails() {
    let arena = Arena::new_bounded();
    let shared = MemBox::new(Value::u64(42));
    let args: Vec<_> = (0..200).map(|_| shared.as_ref_value()).collect();
    let mut params: Vec<_> = (0..200).map(|_| imm_ref(&arena)).collect();
    params[100] = mut_ref(&arena);
    assert!(check_reference_args_unique(&args, &params).is_err());
}
