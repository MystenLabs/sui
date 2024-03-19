// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::object_table_tests {
    use std::option;
    use sui::object_table::{Self, add, contains, borrow, borrow_mut, remove, value_id};
    use sui::object::{Self, UID};
    use sui::test_scenario as ts;

    struct Counter has key, store {
        id: UID,
        count: u64,
    }

    #[test]
    fun simple_all_functions() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = object_table::new(ts::ctx(&mut scenario));
        let counter1 = new(&mut scenario);
        let id1 = object::id(&counter1);
        let counter2 = new(&mut scenario);
        let id2 = object::id(&counter2);
        // add fields
        add(&mut table, b"hello", counter1);
        add(&mut table, b"goodbye", counter2);
        // check they exist
        assert!(contains(&table, b"hello"), 0);
        assert!(contains(&table, b"goodbye"), 0);
        // check the IDs
        assert!(option::borrow(&value_id(&table, b"hello")) == &id1, 0);
        assert!(option::borrow(&value_id(&table, b"goodbye")) == &id2, 0);
        // check the values
        assert!(count(borrow(&table, b"hello")) == 0, 0);
        assert!(count(borrow(&table, b"goodbye")) == 0, 0);
        // mutate them
        bump(borrow_mut(&mut table, b"hello"));
        bump(bump(borrow_mut(&mut table, b"goodbye")));
        // check the new value
        assert!(count(borrow(&table, b"hello")) == 1, 0);
        assert!(count(borrow(&table, b"goodbye")) == 2, 0);
        // remove the value and check it
        assert!(destroy(remove(&mut table, b"hello")) == 1, 0);
        assert!(destroy(remove(&mut table, b"goodbye")) == 2, 0);
        // verify that they are not there
        assert!(!contains(&table, b"hello"), 0);
        assert!(!contains(&table, b"goodbye"), 0);
        ts::end(scenario);
        object_table::destroy_empty(table);
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun add_duplicate() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = object_table::new(ts::ctx(&mut scenario));
        add(&mut table, b"hello", new(&mut scenario));
        add(&mut table, b"hello", new(&mut scenario));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = object_table::new<u64, Counter>(ts::ctx(&mut scenario));
        borrow(&table, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_mut_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = object_table::new<u64, Counter>(ts::ctx(&mut scenario));
        borrow_mut(&mut table, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun remove_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = object_table::new<u64, Counter>(ts::ctx(&mut scenario));
        destroy(remove(&mut table, 0));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::object_table::ETableNotEmpty)]
    fun destroy_non_empty() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = object_table::new(ts::ctx(&mut scenario));
        add(&mut table, 0, new(&mut scenario));
        object_table::destroy_empty(table);
        ts::end(scenario);
    }

    #[test]
    fun sanity_check_contains() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = object_table::new(ts::ctx(&mut scenario));
        assert!(!contains(&table, 0), 0);
        add(&mut table, 0, new(&mut scenario));
        assert!(contains(&table, 0), 0);
        assert!(!contains(&table, 1), 0);
        ts::end(scenario);
        destroy(remove(&mut table, 0));
        object_table::destroy_empty(table)
    }

    #[test]
    fun sanity_check_size() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = object_table::new(ts::ctx(&mut scenario));
        assert!(object_table::is_empty(&table), 0);
        assert!(object_table::length(&table) == 0, 0);
        add(&mut table, 0, new(&mut scenario));
        assert!(!object_table::is_empty(&table), 0);
        assert!(object_table::length(&table) == 1, 0);
        add(&mut table, 1, new(&mut scenario));
        assert!(!object_table::is_empty(&table), 0);
        assert!(object_table::length(&table) == 2, 0);
        ts::end(scenario);
        destroy(remove(&mut table, 0));
        destroy(remove(&mut table, 1));
        object_table::destroy_empty(table);
    }

    // transfer an object field from one "parent" to another
    #[test]
    fun transfer_object() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table1 = object_table::new<u64, Counter>(ts::ctx(&mut scenario));
        let table2 = object_table::new<u64, Counter>(ts::ctx(&mut scenario));
        add(&mut table1, 0, new(&mut scenario));
        assert!(contains(&table1, 0), 0);
        assert!(!contains(&table2, 0), 0);
        bump(borrow_mut(&mut table1, 0));
        let c = remove(&mut table1, 0);
        add(&mut table2, 0, c);
        assert!(!contains(&table1, 0), 0);
        assert!(contains(&table2, 0), 0);
        bump(borrow_mut(&mut table2, 0));
        assert!(count(borrow(&table2, 0)) == 2, 0);
        ts::end(scenario);
        destroy(remove(&mut table2, 0));
        object_table::destroy_empty(table1);
        object_table::destroy_empty(table2);
    }

    fun new(scenario: &mut ts::Scenario): Counter {
        Counter { id: ts::new_object(scenario), count: 0 }
    }

    fun count(counter: &Counter): u64 {
        counter.count
    }

    fun bump(counter: &mut Counter): &mut Counter {
        counter.count = counter.count + 1;
        counter
    }

    fun destroy(counter: Counter): u64 {
        let Counter { id, count } = counter;
        object::delete(id);
        count
    }
}
