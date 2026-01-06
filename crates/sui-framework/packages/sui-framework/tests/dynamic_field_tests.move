// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::dynamic_field_tests;

use sui::dynamic_field::{add, exists_with_type, borrow, borrow_mut, remove};
use sui::test_scenario;

#[test]
fun simple_all_functions() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    // add fields
    add<u64, u64>(&mut id, 0, 0);
    add<vector<u8>, u64>(&mut id, b"", 1);
    add<bool, u64>(&mut id, false, 2);
    // check they exist
    assert!(exists_with_type<u64, u64>(&id, 0));
    assert!(exists_with_type<vector<u8>, u64>(&id, b""));
    assert!(exists_with_type<bool, u64>(&id, false));
    // check the values
    assert!(*borrow(&id, 0u64) == 0u64);
    assert!(*borrow(&id, b"") == 1u64);
    assert!(*borrow(&id, false) == 2u64);
    // mutate them
    *borrow_mut(&mut id, 0u64) = 3u64 + *borrow(&id, 0u64);
    *borrow_mut(&mut id, b"") = 4u64 + *borrow(&id, b"");
    *borrow_mut(&mut id, false) = 5u64 + *borrow(&id, false);
    // check the new value
    assert!(*borrow(&id, 0u64) == 3u64);
    assert!(*borrow(&id, b"") == 5u64);
    assert!(*borrow(&id, false) == 7u64);
    // remove the value and check it
    assert!(remove(&mut id, 0u64) == 3u64);
    assert!(remove(&mut id, b"") == 5u64);
    assert!(remove(&mut id, false) == 7u64);
    // verify that they are not there
    assert!(!exists_with_type<u64, u64>(&id, 0));
    assert!(!exists_with_type<vector<u8>, u64>(&id, b""));
    assert!(!exists_with_type<bool, u64>(&id, false));
    scenario.end();
    id.delete();
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun add_duplicate() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add<u64, u64>(&mut id, 0, 0);
    add<u64, u64>(&mut id, 0, 1);
    abort 42
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun add_duplicate_mismatched_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add<u64, u64>(&mut id, 0, 0u64);
    add<u64, u8>(&mut id, 0, 1u8);
    abort 42
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun borrow_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let id = scenario.new_object();
    borrow<u64, u64>(&id, 0);
    abort 42
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
fun borrow_wrong_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 0u64);
    borrow<u64, u8>(&id, 0);
    abort 42
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun borrow_mut_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    borrow_mut<u64, u64>(&mut id, 0);
    abort 42
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
fun borrow_mut_wrong_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 0u64);
    borrow_mut<u64, u8>(&mut id, 0);
    abort 42
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun remove_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    remove<u64, u64>(&mut id, 0);
    abort 42
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
fun remove_wrong_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 0u64);
    remove<u64, u8>(&mut id, 0);
    abort 42
}

#[test]
fun sanity_check_exists() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    assert!(!exists_with_type<u64, u64>(&id, 0));
    add(&mut id, 0u64, 0u64);
    assert!(exists_with_type<u64, u64>(&id, 0));
    assert!(!exists_with_type<u64, u8>(&id, 0));
    scenario.end();
    id.delete();
}

// should be able to do delete a UID even though it has a dynamic field
#[test]
fun delete_uid_with_fields() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 0u64);
    assert!(exists_with_type<u64, u64>(&id, 0));
    scenario.end();
    id.delete();
}
