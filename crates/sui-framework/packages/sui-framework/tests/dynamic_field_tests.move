// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::dynamic_field_tests;

use std::unit_test::assert_eq;
use sui::dynamic_field::{Self, add, exists_with_type, borrow, borrow_mut, remove, exists};
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

#[test]
fun remove_opt_existing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    let old: Option<u64> = dynamic_field::remove_opt(&mut id, 0u64);
    assert_eq!(old, option::some(42));
    assert!(!exists<u64>(&id, 0));
    scenario.end();
    id.delete();
}

#[test]
fun remove_opt_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    let old: Option<u64> = dynamic_field::remove_opt(&mut id, 0u64);
    assert_eq!(old, option::none());
    scenario.end();
    id.delete();
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
fun remove_opt_wrong_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    // Value is u8 but actual value is u64, so remove_opt should abort
    let _old: Option<u8> = dynamic_field::remove_opt(&mut id, 0u64);
    abort
}

#[test]
fun replace_existing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    let old = dynamic_field::replace<u64, u64, u64>(&mut id, 0, 100);
    assert_eq!(old, option::some(42));
    assert_eq!(*borrow<u64, u64>(&id, 0), 100);
    scenario.end();
    id.delete();
}

#[test]
fun replace_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    let old = dynamic_field::replace<u64, u64, u64>(&mut id, 0, 100);
    assert_eq!(old, option::none());
    assert_eq!(*borrow<u64, u64>(&id, 0), 100);
    scenario.end();
    id.delete();
}

#[test]
fun replace_different_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    let old = dynamic_field::replace<u64, u8, u64>(&mut id, 0, 7u8);
    assert_eq!(old, option::some(42u64));
    assert_eq!(*borrow<u64, u8>(&id, 0), 7);
    scenario.end();
    id.delete();
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
fun replace_wrong_old_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    // ValueOld is u8 but actual value is u64, so remove_opt call in replace should abort
    let _old = dynamic_field::replace<u64, u8, u8>(&mut id, 0, 7u8);
    abort
}

// === Macro Tests ===

#[test]
fun borrow_or_add_existing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    assert!(exists_with_type<u64, u64>(&id, 0));
    assert_eq!(*dynamic_field::borrow_or_add!(&mut id, 0u64, { assert!(false); 99u64 }), 42);
    assert!(exists_with_type<u64, u64>(&id, 0));
    scenario.end();
    id.delete();
}

#[test]
fun borrow_or_add_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    assert!(!exists<u64>(&id, 0));
    assert_eq!(*dynamic_field::borrow_or_add!(&mut id, 0u64, 99u64), 99);
    assert!(exists_with_type<u64, u64>(&id, 0));
    scenario.end();
    id.delete();
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
fun borrow_or_add_wrong_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    // borrow aborts on type mismatch
    dynamic_field::borrow_or_add!(&mut id, 0u64, { assert!(false); 0u8 });
    abort
}

#[test]
fun borrow_mut_or_add_existing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    assert!(exists_with_type<u64, u64>(&id, 0));
    *dynamic_field::borrow_mut_or_add!(&mut id, 0u64, { assert!(false); 99u64 }) = 100;
    assert_eq!(*borrow(&id, 0u64), 100u64);
    scenario.end();
    id.delete();
}

#[test]
fun borrow_mut_or_add_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    assert!(!exists<u64>(&id, 0));
    *dynamic_field::borrow_mut_or_add!(&mut id, 0u64, 99u64) = 100;
    assert_eq!(*borrow(&id, 0u64), 100u64);
    scenario.end();
    id.delete();
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
fun borrow_mut_or_add_wrong_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    // borrow_mut aborts on type mismatch
    dynamic_field::borrow_mut_or_add!(&mut id, 0u64, { assert!(false); 0u8 });
    abort
}

#[test]
fun get_do_existing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    let mut called = false;
    dynamic_field::get_do!(&id, 0u64, |v: &u64| {
        assert_eq!(*v, 42);
        called = true;
    });
    assert!(called);
    scenario.end();
    id.delete();
}

#[test]
fun get_do_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let id = scenario.new_object();
    let mut called = false;
    dynamic_field::get_do!(&id, 0u64, |_v: &u64| { called = true; assert!(false) });
    assert!(!called);
    scenario.end();
    id.delete();
}

#[test]
fun get_do_wrong_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    let mut called = false;
    dynamic_field::get_do!(&id, 0u64, |_v: &u8| { called = true; assert!(false) });
    assert!(!called);
    scenario.end();
    id.delete();
}

#[test]
fun get_mut_do_existing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    dynamic_field::get_mut_do!(&mut id, 0u64, |v: &mut u64| { *v = 100; });
    assert_eq!(*borrow(&id, 0u64), 100u64);
    scenario.end();
    id.delete();
}

#[test]
fun get_mut_do_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    let mut called = false;
    dynamic_field::get_mut_do!(&mut id, 0u64, |_v: &mut u64| { called = true; assert!(false) });
    assert!(!called);
    scenario.end();
    id.delete();
}

#[test]
fun get_mut_do_wrong_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    let mut called = false;
    dynamic_field::get_mut_do!(&mut id, 0u64, |_v: &mut u8| { called = true; assert!(false) });
    assert!(!called);
    scenario.end();
    id.delete();
}

#[test]
fun get_fold_existing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    let result = dynamic_field::get_fold!(&id, 0u64, abort 0, |v: &u64| *v + 1);
    assert_eq!(result, 43);
    scenario.end();
    id.delete();
}

#[test]
fun get_fold_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let id = scenario.new_object();
    let result: u64 = dynamic_field::get_fold!(&id, 0u64, 0u64, |_: &u64| abort 0);
    assert_eq!(result, 0);
    scenario.end();
    id.delete();
}

#[test]
fun get_fold_wrong_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    let result: u8 = dynamic_field::get_fold!(&id, 0u64, 0u8, |_: &u8| abort 0);
    assert_eq!(result, 0);
    scenario.end();
    id.delete();
}

#[test]
fun get_mut_fold_existing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    let result = dynamic_field::get_mut_fold!(&mut id, 0u64, abort 0, |v: &mut u64| {
        *v = 100;
        99u64
    });
    assert_eq!(result, 99);
    assert_eq!(*borrow(&id, 0u64), 100u64);
    scenario.end();
    id.delete();
}

#[test]
fun get_mut_fold_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    let result: u64 = dynamic_field::get_mut_fold!(&mut id, 0u64, 0u64, |_: &mut u64| abort 0);
    assert_eq!(result, 0);
    scenario.end();
    id.delete();
}

#[test]
fun get_mut_fold_wrong_type() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    add(&mut id, 0u64, 42u64);
    let result: u8 = dynamic_field::get_mut_fold!(&mut id, 0u64, 0u8, |_: &mut u8| abort 0);
    assert_eq!(result, 0);
    scenario.end();
    id.delete();
}

// === Deprecated Tests ===

#[test, allow(deprecated_usage)]
fun deprecated_exists_() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    assert_eq!(dynamic_field::exists_<u64>(&id, 0), exists<u64>(&id, 0));
    assert_eq!(exists<u64>(&id, 0), false);
    add(&mut id, 0u64, 42u64);
    assert_eq!(dynamic_field::exists_<u64>(&id, 0), exists<u64>(&id, 0));
    assert_eq!(exists<u64>(&id, 0), true);
    scenario.end();
    id.delete();
}

#[test, allow(deprecated_usage)]
fun deprecated_remove_if_exists() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut id = scenario.new_object();
    // missing — both should return none
    let old: Option<u64> = dynamic_field::remove_if_exists(&mut id, 0u64);
    assert_eq!(old, option::none());
    // existing — both should return the value
    add(&mut id, 0u64, 42u64);
    let old: Option<u64> = dynamic_field::remove_if_exists(&mut id, 0u64);
    assert_eq!(old, option::some(42));
    // verify remove_opt behaves the same
    add(&mut id, 0u64, 99u64);
    let new: Option<u64> = dynamic_field::remove_opt(&mut id, 0u64);
    assert_eq!(new, option::some(99));
    scenario.end();
    id.delete();
}
