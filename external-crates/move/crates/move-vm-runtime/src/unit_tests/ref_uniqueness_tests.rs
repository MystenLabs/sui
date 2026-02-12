// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution::{
    interpreter::{check_reference_args_unique, locals::MachineHeap},
    values::{MemBox, Reference, VMValueCast, Value},
};

#[test]
fn no_references_passes() {
    let args = vec![Value::u64(1), Value::u64(2), Value::bool(true)];
    assert!(check_reference_args_unique(&args).is_ok());
}

#[test]
fn empty_args_passes() {
    let args: Vec<Value> = vec![];
    assert!(check_reference_args_unique(&args).is_ok());
}

#[test]
fn distinct_references_passes() {
    let a = MemBox::new(Value::u64(42));
    let b = MemBox::new(Value::u64(42));
    let args = vec![a.as_ref_value(), b.as_ref_value()];
    assert!(check_reference_args_unique(&args).is_ok());
}

#[test]
fn duplicate_reference_fails() {
    let a = MemBox::new(Value::u64(42));
    let ref1 = a.as_ref_value();
    let ref2 = a.as_ref_value();
    let args = vec![ref1, ref2];
    assert!(check_reference_args_unique(&args).is_err());
}

#[test]
fn mixed_values_and_distinct_references_passes() {
    let a = MemBox::new(Value::u64(1));
    let b = MemBox::new(Value::u64(2));
    let args = vec![
        Value::bool(true),
        a.as_ref_value(),
        Value::u64(99),
        b.as_ref_value(),
    ];
    assert!(check_reference_args_unique(&args).is_ok());
}

#[test]
fn mixed_values_and_duplicate_reference_fails() {
    let a = MemBox::new(Value::u64(1));
    let args = vec![Value::bool(true), a.as_ref_value(), a.as_ref_value()];
    assert!(check_reference_args_unique(&args).is_err());
}

#[test]
fn single_reference_passes() {
    let a = MemBox::new(Value::u64(42));
    let args = vec![a.as_ref_value()];
    assert!(check_reference_args_unique(&args).is_ok());
}

#[test]
fn duplicate_among_many_distinct_fails() {
    let boxes: Vec<_> = (0..10).map(|i| MemBox::new(Value::u64(i))).collect();
    let mut args: Vec<_> = boxes.iter().map(|b| b.as_ref_value()).collect();
    // Add a duplicate of the fifth reference
    args.push(boxes[4].as_ref_value());
    assert!(check_reference_args_unique(&args).is_err());
}

#[test]
fn references_from_locals_distinct_passes() {
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
    assert!(check_reference_args_unique(&args).is_ok());
}

#[test]
fn references_from_same_local_fails() {
    let mut heap = MachineHeap::new();
    let mut locals = heap.allocate_stack_frame(vec![], 1).unwrap();
    locals.store_loc(0, Value::u64(10)).unwrap();
    let args = vec![locals.borrow_loc(0).unwrap(), locals.borrow_loc(0).unwrap()];
    assert!(check_reference_args_unique(&args).is_err());
}
