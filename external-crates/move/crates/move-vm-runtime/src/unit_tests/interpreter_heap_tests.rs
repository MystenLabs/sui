// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::execution::{interpreter::locals, values::Value};

macro_rules! store_loc {
    ($frame:expr, $index:expr, $value:expr) => {
        $frame
            .store_loc($index, $value)
            .expect(&format!("Failed to store value in slot {}", $index));
    };
}

#[test]
fn test_drop_all_values() {
    let mut heap = locals::MachineHeap::new();
    let mut frame = heap
        .allocate_stack_frame(vec![], 5)
        .expect("Failed to allocate frame");

    store_loc!(frame, 0, Value::U64(42));
    store_loc!(frame, 1, Value::Invalid);
    let ref_value = frame
        .borrow_loc(0)
        .expect("Failed to borrow value from slot 0");
    store_loc!(frame, 2, ref_value);
    store_loc!(frame, 4, Value::Bool(false));

    let result = frame.drop_all_values().expect("Failed to drop all values");

    assert!(result.len() == 2);
    assert!(matches!(result[0], Value::U64(42)));
    assert!(matches!(result[1], Value::Bool(false)));

    for membox in frame.UNSAFE_borrow_slice().iter() {
        let value_ref = membox
            .try_borrow_mut()
            .expect("Failed to borrow mutable reference to value");
        assert!(matches!(*value_ref, Value::Invalid));
    }
}

#[test]
fn test_drop_all_values_borrow_error() {
    let mut heap = locals::MachineHeap::new();
    let mut frame = heap
        .allocate_stack_frame(vec![], 5)
        .expect("Failed to allocate frame");

    store_loc!(frame, 0, Value::U64(42));
    store_loc!(frame, 1, Value::Bool(true));

    // Hold a mutable borrow to cause borrow_mut to fail

    // We need unsafe behavior to bypass the borrow checker for this test
    let frame_ptr = &mut frame as *mut locals::StackFrame;

    #[allow(unsafe_code)]
    let _held_borrow = unsafe { (*frame_ptr).UNSAFE_borrow_slice()[0].try_borrow_mut().unwrap() };

    // This should return an error because slot 0 is already borrowed
    #[allow(unsafe_code)]
    let result = unsafe { (*frame_ptr).drop_all_values() };
    assert!(result.is_err());
}
