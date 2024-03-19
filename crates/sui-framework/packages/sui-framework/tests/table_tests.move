// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::table_tests {
    use sui::table::{Self, add, contains, borrow, borrow_mut, remove};
    use sui::test_scenario as ts;

    #[test]
    fun simple_all_functions() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = table::new(ts::ctx(&mut scenario));
        // add fields
        add(&mut table, b"hello", 0);
        add(&mut table, b"goodbye", 1);
        // check they exist
        assert!(contains(&table, b"hello"), 0);
        assert!(contains(&table, b"goodbye"), 0);
        // check the values
        assert!(*borrow(&table, b"hello") == 0, 0);
        assert!(*borrow(&table, b"goodbye") == 1, 0);
        // mutate them
        *borrow_mut(&mut table, b"hello") = *borrow(&table, b"hello") * 2;
        *borrow_mut(&mut table, b"goodbye") = *borrow(&table, b"goodbye") * 2;
        // check the new value
        assert!(*borrow(&table, b"hello") == 0, 0);
        assert!(*borrow(&table, b"goodbye") == 2, 0);
        // remove the value and check it
        assert!(remove(&mut table, b"hello") == 0, 0);
        assert!(remove(&mut table, b"goodbye") == 2, 0);
        // verify that they are not there
        assert!(!contains(&table, b"hello"), 0);
        assert!(!contains(&table, b"goodbye"), 0);
        ts::end(scenario);
        table::destroy_empty(table);
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun add_duplicate() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = table::new(ts::ctx(&mut scenario));
        add(&mut table, b"hello", 0);
        add(&mut table, b"hello", 1);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = table::new<u64, u64>(ts::ctx(&mut scenario));
        borrow(&table, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_mut_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = table::new<u64, u64>(ts::ctx(&mut scenario));
        borrow_mut(&mut table, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun remove_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = table::new<u64, u64>(ts::ctx(&mut scenario));
        remove(&mut table, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::table::ETableNotEmpty)]
    fun destroy_non_empty() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = table::new<u64, u64>(ts::ctx(&mut scenario));
        add(&mut table, 0, 0);
        table::destroy_empty(table);
        ts::end(scenario);
    }

    #[test]
    fun sanity_check_contains() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = table::new<u64, u64>(ts::ctx(&mut scenario));
        assert!(!contains(&table, 0), 0);
        add(&mut table, 0, 0);
        assert!(contains<u64, u64>(&table, 0), 0);
        assert!(!contains<u64, u64>(&table, 1), 0);
        ts::end(scenario);
        table::drop(table);
    }

    #[test]
    fun sanity_check_drop() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = table::new<u64, u64>(ts::ctx(&mut scenario));
        add(&mut table, 0, 0);
        assert!(table::length(&table) == 1, 0);
        ts::end(scenario);
        table::drop(table);
    }

    #[test]
    fun sanity_check_size() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = table::new<u64, u64>(ts::ctx(&mut scenario));
        assert!(table::is_empty(&table), 0);
        assert!(table::length(&table) == 0, 0);
        add(&mut table, 0, 0);
        assert!(!table::is_empty(&table), 0);
        assert!(table::length(&table) == 1, 0);
        add(&mut table, 1, 0);
        assert!(!table::is_empty(&table), 0);
        assert!(table::length(&table) == 2, 0);
        ts::end(scenario);
        table::drop(table);
    }
}
