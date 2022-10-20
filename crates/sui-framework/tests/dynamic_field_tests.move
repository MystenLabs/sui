// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::dynamic_field_tests {

use sui::dynamic_field::{add, exists_with_type, borrow, borrow_mut, remove};
use sui::object;
use sui::test_scenario as ts;

#[test]
fun simple_all_functions() {
    let sender = @0x0;
    let scenario = ts::begin(sender);
    let id = ts::new_object(&mut scenario);
    // add fields
    add<u64, u64>(&mut id, 0, 0);
    add<vector<u8>, u64>(&mut id, b"", 1);
    add<bool, u64>(&mut id, false, 2);
    // check they exist
    assert!(exists_with_type<u64, u64>(&id, 0), 0);
    assert!(exists_with_type<vector<u8>, u64>(&id, b""), 0);
    assert!(exists_with_type<bool, u64>(&id, false), 0);
    // check the values
    assert!(*borrow(&id, 0) == 0, 0);
    assert!(*borrow(&id, b"") == 1, 0);
    assert!(*borrow(&id, false) == 2, 0);
    // mutate them
    *borrow_mut(&mut id, 0) = 3 + *borrow(&id, 0);
    *borrow_mut(&mut id, b"") = 4 + *borrow(&id, b"");
    *borrow_mut(&mut id, false) = 5 + *borrow(&id, false);
    // check the new value
    assert!(*borrow(&id, 0) == 3, 0);
    assert!(*borrow(&id, b"") == 5, 0);
    assert!(*borrow(&id, false) == 7, 0);
    // remove the value and check it
    assert!(remove(&mut id, 0) == 3, 0);
    assert!(remove(&mut id, b"") == 5, 0);
    assert!(remove(&mut id, false) == 7, 0);
    // verify that they are not there
    assert!(!exists_with_type<u64, u64>(&id, 0), 0);
    assert!(!exists_with_type<vector<u8>, u64>(&id, b""), 0);
    assert!(!exists_with_type<bool, u64>(&id, false), 0);
    ts::end(scenario);
    object::delete(id);
}

}
