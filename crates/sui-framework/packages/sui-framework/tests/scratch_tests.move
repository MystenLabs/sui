// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::scratch_tests;

use std::unit_test::assert_eq;
use sui::test_scenario;

// Key/value types are defined in this module so it is authorized to mint their
// `internal::Permit`s (which is what the `internal_*` macros do under the hood).
public struct Marker() has copy, drop;
public struct WrappedU8(u8) has copy, drop;
public struct WrappedBool(bool) has copy, drop;

#[test]
fun add_read_remove_exists() {
    let mut ctx = tx_context::dummy();

    assert!(!ctx.scratch_internal_exists!<Marker>(Marker()));
    assert!(!ctx.scratch_internal_exists_with_type!<Marker, u64>(Marker()));
    assert!(!ctx.scratch_internal_exists_with_type!<Marker, bool>(Marker()));

    // add a value
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);

    // exists / exists_with_type
    assert!(ctx.scratch_internal_exists!<Marker>(Marker()));
    assert!(ctx.scratch_internal_exists_with_type!<Marker, u64>(Marker()));
    assert!(!ctx.scratch_internal_exists_with_type!<Marker, bool>(Marker()));

    // read returns the value, and the entry persists so a second read succeeds
    assert_eq!(ctx.scratch_internal_read!<Marker, u64>(Marker()), 42);
    assert_eq!(ctx.scratch_internal_read!<Marker, u64>(Marker()), 42);

    // remove returns the value, and the key is then absent
    assert_eq!(ctx.scratch_internal_remove!<Marker, u64>(Marker()), 42);
    assert!(!ctx.scratch_internal_exists!<Marker>(Marker()));
    assert!(!ctx.scratch_internal_exists_with_type!<Marker, u64>(Marker()));
    assert!(!ctx.scratch_internal_exists_with_type!<Marker, bool>(Marker()));

    // the slot is free again after removal, so the key can be re-added (with a new value/type)
    ctx.scratch_internal_add!<Marker, bool>(Marker(), true);
    assert!(ctx.scratch_internal_exists!<Marker>(Marker()));
    assert!(!ctx.scratch_internal_exists_with_type!<Marker, u64>(Marker()));
    assert!(ctx.scratch_internal_exists_with_type!<Marker, bool>(Marker()));
    assert_eq!(ctx.scratch_internal_read!<Marker, bool>(Marker()), true);
}

#[test, expected_failure(abort_code = sui::scratch::EEntryAlreadyExists)]
fun add_duplicate() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 0);
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 1);
}

#[test, expected_failure(abort_code = sui::scratch::EEntryAlreadyExists)]
fun add_duplicate_mismatched_value_type() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 0);
    // the key already exists, regardless of the value type
    ctx.scratch_internal_add!<Marker, bool>(Marker(), true);
}

#[test, expected_failure(abort_code = sui::scratch::EEntryDoesNotExist)]
fun read_missing() {
    let ctx = tx_context::dummy();
    ctx.scratch_internal_read!<Marker, u64>(Marker());
}

#[test, expected_failure(abort_code = sui::scratch::EEntryDoesNotExist)]
fun remove_missing() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_remove!<Marker, u64>(Marker());
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun read_type_mismatch() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 0);
    ctx.scratch_internal_read!<Marker, bool>(Marker());
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun remove_type_mismatch() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 0);
    ctx.scratch_internal_remove!<Marker, bool>(Marker());
}

// `WrappedU8(1)` and `WrappedBool(true)` serialize to the same byte, but are distinct keys
// because the key type is part of the derived address. Entries under them must not collide.
#[test]
fun keys_same_bytes_distinct_types() {
    let mut ctx = tx_context::dummy();
    // both adds succeed -> the two keys derive different addresses despite identical bytes
    ctx.scratch_internal_add!<WrappedU8, u64>(WrappedU8(1), 100);
    ctx.scratch_internal_add!<WrappedBool, u64>(WrappedBool(true), 200);

    assert!(ctx.scratch_internal_exists!<WrappedU8>(WrappedU8(1)));
    assert!(ctx.scratch_internal_exists!<WrappedBool>(WrappedBool(true)));

    // each key reads back its own value, with no cross-contamination
    assert_eq!(ctx.scratch_internal_read!<WrappedU8, u64>(WrappedU8(1)), 100);
    assert_eq!(ctx.scratch_internal_read!<WrappedBool, u64>(WrappedBool(true)), 200);

    // removing one key leaves the other untouched
    assert_eq!(ctx.scratch_internal_remove!<WrappedU8, u64>(WrappedU8(1)), 100);
    assert!(!ctx.scratch_internal_exists!<WrappedU8>(WrappedU8(1)));
    assert!(ctx.scratch_internal_exists!<WrappedBool>(WrappedBool(true)));
    assert_eq!(ctx.scratch_internal_read!<WrappedBool, u64>(WrappedBool(true)), 200);
}

// Same key type, different key values are distinct keys.
#[test]
fun keys_same_type_distinct_values() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<WrappedU8, u64>(WrappedU8(1), 10);
    ctx.scratch_internal_add!<WrappedU8, u64>(WrappedU8(2), 20);
    assert_eq!(ctx.scratch_internal_read!<WrappedU8, u64>(WrappedU8(1)), 10);
    assert_eq!(ctx.scratch_internal_read!<WrappedU8, u64>(WrappedU8(2)), 20);
}

// A stored `WrappedU8(1)` value and a `WrappedBool(true)` value serialize to the same byte, but
// the entry records the value type, so read/exists_with_type never confuse them.
#[test]
fun value_types_same_bytes_not_confused() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, WrappedU8>(Marker(), WrappedU8(1));

    // exists_with_type matches only the exact value type
    assert!(ctx.scratch_internal_exists_with_type!<Marker, WrappedU8>(Marker()));
    assert!(!ctx.scratch_internal_exists_with_type!<Marker, WrappedBool>(Marker()));

    // read with the correct type yields the value
    assert_eq!(ctx.scratch_internal_read!<Marker, WrappedU8>(Marker()), WrappedU8(1));
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun read_wrong_value_type_same_bytes() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, WrappedU8>(Marker(), WrappedU8(1));
    // `WrappedBool(true)` has the same serialized byte as `WrappedU8(1)`, but is a different type
    ctx.scratch_internal_read!<Marker, WrappedBool>(Marker());
}

#[test]
fun cleared_across_transactions() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    scenario.ctx().scratch_internal_add!<Marker, u64>(Marker(), 42);
    assert!(scenario.ctx().scratch_internal_exists!<Marker>(Marker()));

    scenario.next_tx(sender);

    // scratch is per-transaction, so the entry from the previous transaction must be gone
    assert!(!scenario.ctx().scratch_internal_exists!<Marker>(Marker()));
    scenario.end();
}
