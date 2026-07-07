// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::scratch_tests;

use std::unit_test::assert_eq;
use sui::scratch;
use sui::test_scenario;

// Key/value types are defined in this module so it is authorized to mint their
// `internal::Permit`s (which is what the `internal_*` macros do under the hood).
public struct Marker() has copy, drop;
public struct WrappedU8(u8) has copy, drop;
public struct WrappedBool(bool) has copy, drop;

#[test]
fun add_read_remove_exists() {
    let mut ctx = tx_context::dummy();

    assert!(!scratch::internal_exists!<Marker>(&ctx, Marker()));
    assert!(!scratch::internal_exists_with_type!<Marker, u64>(&ctx, Marker()));
    assert!(!scratch::internal_exists_with_type!<Marker, bool>(&ctx, Marker()));

    // add a value
    scratch::internal_add!<Marker, u64>(&mut ctx, Marker(), 42);

    // exists / exists_with_type
    assert!(scratch::internal_exists!<Marker>(&ctx, Marker()));
    assert!(scratch::internal_exists_with_type!<Marker, u64>(&ctx, Marker()));
    assert!(!scratch::internal_exists_with_type!<Marker, bool>(&ctx, Marker()));

    // read returns the value, and the entry persists so a second read succeeds
    assert_eq!(scratch::internal_read!<Marker, u64>(&ctx, Marker()), 42);
    assert_eq!(scratch::internal_read!<Marker, u64>(&ctx, Marker()), 42);

    // remove returns the value, and the key is then absent
    assert_eq!(scratch::internal_remove!<Marker, u64>(&mut ctx, Marker()), 42);
    assert!(!scratch::internal_exists!<Marker>(&ctx, Marker()));
    assert!(!scratch::internal_exists_with_type!<Marker, u64>(&ctx, Marker()));
    assert!(!scratch::internal_exists_with_type!<Marker, bool>(&ctx, Marker()));

    // the slot is free again after removal, so the key can be re-added (with a new value/type)
    scratch::internal_add!<Marker, bool>(&mut ctx, Marker(), true);
    assert!(scratch::internal_exists!<Marker>(&ctx, Marker()));
    assert!(!scratch::internal_exists_with_type!<Marker, u64>(&ctx, Marker()));
    assert!(scratch::internal_exists_with_type!<Marker, bool>(&ctx, Marker()));
    assert_eq!(scratch::internal_read!<Marker, bool>(&ctx, Marker()), true);
}

#[test]
#[expected_failure(abort_code = sui::scratch::EEntryAlreadyExists)]
fun add_duplicate() {
    let mut ctx = tx_context::dummy();
    scratch::internal_add!<Marker, u64>(&mut ctx, Marker(), 0);
    scratch::internal_add!<Marker, u64>(&mut ctx, Marker(), 1);
}

#[test]
#[expected_failure(abort_code = sui::scratch::EEntryAlreadyExists)]
fun add_duplicate_mismatched_value_type() {
    let mut ctx = tx_context::dummy();
    scratch::internal_add!<Marker, u64>(&mut ctx, Marker(), 0);
    // the key already exists, regardless of the value type
    scratch::internal_add!<Marker, bool>(&mut ctx, Marker(), true);
}

#[test]
#[expected_failure(abort_code = sui::scratch::EEntryDoesNotExist)]
fun read_missing() {
    let ctx = tx_context::dummy();
    scratch::internal_read!<Marker, u64>(&ctx, Marker());
}

#[test]
#[expected_failure(abort_code = sui::scratch::EEntryDoesNotExist)]
fun remove_missing() {
    let mut ctx = tx_context::dummy();
    scratch::internal_remove!<Marker, u64>(&mut ctx, Marker());
}

#[test]
#[expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun read_type_mismatch() {
    let mut ctx = tx_context::dummy();
    scratch::internal_add!<Marker, u64>(&mut ctx, Marker(), 0);
    scratch::internal_read!<Marker, bool>(&ctx, Marker());
}

#[test]
#[expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun remove_type_mismatch() {
    let mut ctx = tx_context::dummy();
    scratch::internal_add!<Marker, u64>(&mut ctx, Marker(), 0);
    scratch::internal_remove!<Marker, bool>(&mut ctx, Marker());
}

// `WrappedU8(1)` and `WrappedBool(true)` serialize to the same byte, but are distinct keys
// because the key type is part of the derived address. Entries under them must not collide.
#[test]
fun keys_same_bytes_distinct_types() {
    let mut ctx = tx_context::dummy();
    // both adds succeed -> the two keys derive different addresses despite identical bytes
    scratch::internal_add!<WrappedU8, u64>(&mut ctx, WrappedU8(1), 100);
    scratch::internal_add!<WrappedBool, u64>(&mut ctx, WrappedBool(true), 200);

    assert!(scratch::internal_exists!<WrappedU8>(&ctx, WrappedU8(1)));
    assert!(scratch::internal_exists!<WrappedBool>(&ctx, WrappedBool(true)));

    // each key reads back its own value, with no cross-contamination
    assert_eq!(scratch::internal_read!<WrappedU8, u64>(&ctx, WrappedU8(1)), 100);
    assert_eq!(scratch::internal_read!<WrappedBool, u64>(&ctx, WrappedBool(true)), 200);

    // removing one key leaves the other untouched
    assert_eq!(scratch::internal_remove!<WrappedU8, u64>(&mut ctx, WrappedU8(1)), 100);
    assert!(!scratch::internal_exists!<WrappedU8>(&ctx, WrappedU8(1)));
    assert!(scratch::internal_exists!<WrappedBool>(&ctx, WrappedBool(true)));
    assert_eq!(scratch::internal_read!<WrappedBool, u64>(&ctx, WrappedBool(true)), 200);
}

// Same key type, different key values are distinct keys.
#[test]
fun keys_same_type_distinct_values() {
    let mut ctx = tx_context::dummy();
    scratch::internal_add!<WrappedU8, u64>(&mut ctx, WrappedU8(1), 10);
    scratch::internal_add!<WrappedU8, u64>(&mut ctx, WrappedU8(2), 20);
    assert_eq!(scratch::internal_read!<WrappedU8, u64>(&ctx, WrappedU8(1)), 10);
    assert_eq!(scratch::internal_read!<WrappedU8, u64>(&ctx, WrappedU8(2)), 20);
}

// A stored `WrappedU8(1)` value and a `WrappedBool(true)` value serialize to the same byte, but
// the entry records the value type, so read/exists_with_type never confuse them.
#[test]
fun value_types_same_bytes_not_confused() {
    let mut ctx = tx_context::dummy();
    scratch::internal_add!<Marker, WrappedU8>(&mut ctx, Marker(), WrappedU8(1));

    // exists_with_type matches only the exact value type
    assert!(scratch::internal_exists_with_type!<Marker, WrappedU8>(&ctx, Marker()));
    assert!(!scratch::internal_exists_with_type!<Marker, WrappedBool>(&ctx, Marker()));

    // read with the correct type yields the value
    assert_eq!(scratch::internal_read!<Marker, WrappedU8>(&ctx, Marker()), WrappedU8(1));
}

#[test]
#[expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun read_wrong_value_type_same_bytes() {
    let mut ctx = tx_context::dummy();
    scratch::internal_add!<Marker, WrappedU8>(&mut ctx, Marker(), WrappedU8(1));
    // `WrappedBool(true)` has the same serialized byte as `WrappedU8(1)`, but is a different type
    scratch::internal_read!<Marker, WrappedBool>(&ctx, Marker());
}

#[test]
fun cleared_across_transactions() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    scratch::internal_add!<Marker, u64>(scenario.ctx(), Marker(), 42);
    assert!(scratch::internal_exists!<Marker>(scenario.ctx(), Marker()));

    scenario.next_tx(sender);

    // scratch is per-transaction, so the entry from the previous transaction must be gone
    assert!(!scratch::internal_exists!<Marker>(scenario.ctx(), Marker()));
    scenario.end();
}
