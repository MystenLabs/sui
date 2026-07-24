// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::scratch_tests;

use std::unit_test::assert_eq;
use sui::scratch;
use sui::test_scenario;

// Key/value types are defined in this module so it is authorized to issue their
// `internal::Permit`s (which is what the `internal_*` macros do under the hood).
public struct Marker() has copy, drop;
public struct WrappedU8(u8) has copy, drop;
public struct WrappedBool(bool) has copy, drop;
// A value type without `copy`, to exercise the borrow-style macros which do not require it.
public struct Counter has drop { n: u64 }

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
fun read_opt_present_and_absent() {
    let mut ctx = tx_context::dummy();

    // absent -> none
    assert!(ctx.scratch_internal_read_opt!<Marker, u64>(Marker()).is_none());
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);

    // present -> some, and the entry persists (read_opt does not remove)
    assert_eq!(ctx.scratch_internal_read_opt!<Marker, u64>(Marker()), option::some(42));
    assert!(ctx.scratch_internal_exists!<Marker>(Marker()));
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun read_opt_type_mismatch() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 0);
    ctx.scratch_internal_read_opt!<Marker, bool>(Marker());
}

#[test]
fun remove_opt_present_and_absent() {
    let mut ctx = tx_context::dummy();

    // absent -> none
    assert!(ctx.scratch_internal_remove_opt!<Marker, u64>(Marker()).is_none());
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);

    // present -> some(value), and the entry is then gone
    assert_eq!(ctx.scratch_internal_remove_opt!<Marker, u64>(Marker()), option::some(42));
    assert!(!ctx.scratch_internal_exists!<Marker>(Marker()));
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun remove_opt_type_mismatch() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 0);
    ctx.scratch_internal_remove_opt!<Marker, bool>(Marker());
}

#[test]
fun replace_present_and_absent() {
    let mut ctx = tx_context::dummy();

    // absent -> none, and the new value is inserted
    assert!(ctx.scratch_internal_replace!<Marker, u64, u64>(Marker(), 1).is_none());
    assert_eq!(ctx.scratch_internal_read!<Marker, u64>(Marker()), 1);

    // present -> some(old), and the new value takes its place
    assert_eq!(ctx.scratch_internal_replace!<Marker, u64, u64>(Marker(), 2), option::some(1));
    assert_eq!(ctx.scratch_internal_read!<Marker, u64>(Marker()), 2);
}

// The old and new value types may differ.
#[test]
fun replace_changes_value_type() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);

    assert_eq!(ctx.scratch_internal_replace!<Marker, bool, u64>(Marker(), true), option::some(42));
    assert!(!ctx.scratch_internal_exists_with_type!<Marker, u64>(Marker()));
    assert!(ctx.scratch_internal_exists_with_type!<Marker, bool>(Marker()));
    assert_eq!(ctx.scratch_internal_read!<Marker, bool>(Marker()), true);
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun replace_wrong_old_type() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 0);
    // `VOld` does not match the stored value type
    ctx.scratch_internal_replace!<Marker, bool, bool>(Marker(), true);
}

// Mint `Permit`s for the key types this module defines, for tests that call `begin_borrow` /
// `end_borrow` directly (the `internal_*` macros do this under the hood). `internal::permit` must
// be called from the module defining the type, so these cannot be a single generic helper.
fun marker_permit(): scratch::Permit<Marker> {
    scratch::permit(internal::permit<Marker>())
}

fun wrapped_u8_permit(): scratch::Permit<WrappedU8> {
    scratch::permit(internal::permit<WrappedU8>())
}

#[test]
fun get_do_basic() {
    let mut ctx = tx_context::dummy();

    // absent
    ctx.scratch_internal_get_do!(Marker(), |_v: &u64| assert!(false));

    // present
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);
    let mut seen = 0;
    ctx.scratch_internal_get_do!(Marker(), |v: &u64| seen = *v);
    assert_eq!(seen, 42);
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun get_do_type_mismatch() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 0);
    ctx.scratch_internal_get_do!(Marker(), |_v: &bool| ());
}

// While a value is borrowed out (as the `get_*` macros do internally via `begin_borrow`), its slot
// is held by a `BorrowMarker`, so an `add` to that key aborts.
#[test, expected_failure(abort_code = sui::scratch::EEntryAlreadyExists)]
fun get_do_borrow_protection() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);
    ctx.scratch_internal_get_do!(Marker(), |_v: &u64| {
        // add fails due to borrow protection, even though the value was temporarily removed
        // TODO(regex) Remove once we have the regex borrow checker
        let mut ctx = tx_context::dummy();
        ctx.scratch_internal_add!<Marker, bool>(Marker(), true);
        ctx.scratch_internal_remove!<Marker, bool>(Marker());
    });
}

#[test]
fun get_mut_do_basic() {
    let mut ctx = tx_context::dummy();

    // absent
    ctx.scratch_internal_get_mut_do!(Marker(), |v: &mut u64| { assert!(false); *v = 0 });
    assert!(!ctx.scratch_internal_exists!<Marker>(Marker()));

    // present
    ctx.scratch_internal_add!<Marker, Counter>(Marker(), Counter { n: 1 });
    ctx.scratch_internal_get_mut_do!(Marker(), |c: &mut Counter| c.n = c.n + 9);
    let Counter { n } = ctx.scratch_internal_remove!<Marker, Counter>(Marker());
    assert_eq!(n, 10);
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun get_mut_do_type_mismatch() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 0);
    ctx.scratch_internal_get_mut_do!(Marker(), |v: &mut bool| *v = !*v);
}

#[test, expected_failure(abort_code = sui::scratch::EEntryAlreadyExists)]
fun get_mut_do_borrow_protection() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);
    ctx.scratch_internal_get_mut_do!(Marker(), |v: &mut u64| {
        // add fails due to borrow protection, even though the value was temporarily removed
        // TODO(regex) Remove once we have the regex borrow checker
        let mut ctx = tx_context::dummy();
        ctx.scratch_internal_add!<Marker, bool>(Marker(), true);
        ctx.scratch_internal_remove!<Marker, bool>(Marker());
        *v = 0;
    });
}

#[test]
fun get_fold_basic() {
    let mut ctx = tx_context::dummy();

    // absent
    assert_eq!(ctx.scratch_internal_get_fold!(Marker(), 7u64, |_v: &u64| { assert!(false); 0 }), 7);

    // present
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);
    assert_eq!(
        ctx.scratch_internal_get_fold!(Marker(), { assert!(false); 0 }, |v: &u64| *v + 1),
        43,
    );
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun get_fold_type_mismatch() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 0);
    ctx.scratch_internal_get_fold!(Marker(), false, |_v: &bool| true);
}

#[test, expected_failure(abort_code = sui::scratch::EEntryAlreadyExists)]
fun get_fold_borrow_protection() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);
    ctx.scratch_internal_get_fold!(Marker(), 0, |v: &u64| {
        // add fails due to borrow protection, even though the value was temporarily removed
        // TODO(regex) Remove once we have the regex borrow checker
        let mut ctx = tx_context::dummy();
        ctx.scratch_internal_add!<Marker, bool>(Marker(), true);
        ctx.scratch_internal_remove!<Marker, bool>(Marker());
        *v
    });
}

#[test]
fun get_mut_fold_basic() {
    let mut ctx = tx_context::dummy();

    // absent
    let d = ctx.scratch_internal_get_mut_fold!(
        Marker(),
        99u64,
        |v: &mut u64| { assert!(false); *v = 0; 0 },
    );
    assert_eq!(d, 99);

    // present
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 10);
    let old = ctx.scratch_internal_get_mut_fold!(Marker(), { assert!(false); 0 }, |v: &mut u64| {
        let old = *v;
        *v = *v + 5;
        old
    });
    assert_eq!(old, 10);
    assert_eq!(ctx.scratch_internal_read!<Marker, u64>(Marker()), 15);
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun get_mut_fold_type_mismatch() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 0);
    ctx.scratch_internal_get_mut_fold!(Marker(), false, |v: &mut bool| {
        *v = !*v;
        *v
    });
}

#[test, expected_failure(abort_code = sui::scratch::EEntryAlreadyExists)]
fun get_mut_fold_borrow_protection() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);
    ctx.scratch_internal_get_mut_fold!(Marker(), 0u64, |v: &mut u64| {
        // add fails due to borrow protection, even though the value was temporarily removed
        // TODO(regex) Remove once we have the regex borrow checker
        let mut ctx = tx_context::dummy();
        ctx.scratch_internal_add!<Marker, bool>(Marker(), true);
        ctx.scratch_internal_remove!<Marker, bool>(Marker());
        *v = 0;
        0
    });
}

#[test]
fun begin_end_borrow_basic() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);

    let (value, marker) = scratch::begin_borrow<Marker, u64>(&mut ctx, marker_permit(), Marker());
    assert_eq!(value, 42);
    assert!(ctx.scratch_internal_exists_with_type!<Marker, scratch::BorrowMarker<u64>>(Marker()));
    scratch::end_borrow(&mut ctx, marker_permit(), Marker(), value, marker);
    assert_eq!(ctx.scratch_internal_read!<Marker, u64>(Marker()), 42);
}

#[test, expected_failure(abort_code = sui::scratch::EEntryDoesNotExist)]
fun begin_borrow_missing() {
    let mut ctx = tx_context::dummy();
    scratch::begin_borrow<Marker, u64>(&mut ctx, marker_permit(), Marker());
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun begin_borrow_type_mismatch() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);
    scratch::begin_borrow<Marker, bool>(&mut ctx, marker_permit(), Marker());
}

#[test, expected_failure(abort_code = sui::scratch::EEntryDoesNotExist)]
fun end_borrow_missing() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);
    let (value, marker) = scratch::begin_borrow<Marker, u64>(&mut ctx, marker_permit(), Marker());
    ctx.scratch_internal_remove!<Marker, scratch::BorrowMarker<u64>>(Marker());
    // aborts since it was already removed
    scratch::end_borrow(&mut ctx, marker_permit(), Marker(), value, marker);
}

#[test, expected_failure(abort_code = sui::scratch::EEntryTypeMismatch)]
fun end_borrow_type_mismatch() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 42);
    let (value, marker) = scratch::begin_borrow<Marker, u64>(&mut ctx, marker_permit(), Marker());
    // replace the marker with a plain value, so the slot is not a `BorrowMarker<u64>`
    ctx.scratch_internal_remove!<Marker, scratch::BorrowMarker<u64>>(Marker());
    ctx.scratch_internal_add!<Marker, u64>(Marker(), 7);
    scratch::end_borrow(&mut ctx, marker_permit(), Marker(), value, marker);
}

#[test, expected_failure(abort_code = sui::scratch::EBorrowMarkerMismatch)]
fun end_borrow_wrong_marker() {
    let mut ctx = tx_context::dummy();
    ctx.scratch_internal_add!<WrappedU8, u64>(WrappedU8(1), 10);
    ctx.scratch_internal_add!<WrappedU8, u64>(WrappedU8(2), 20);

    let (value1, _marker1) = scratch::begin_borrow<WrappedU8, u64>(
        &mut ctx,
        wrapped_u8_permit(),
        WrappedU8(1),
    );
    let (_value2, marker2) = scratch::begin_borrow<WrappedU8, u64>(
        &mut ctx,
        wrapped_u8_permit(),
        WrappedU8(2),
    );
    // ending key 1's borrow with key 2's (distinct) marker aborts
    scratch::end_borrow(&mut ctx, wrapped_u8_permit(), WrappedU8(1), value1, marker2);
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
