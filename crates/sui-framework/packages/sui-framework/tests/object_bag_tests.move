// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::object_bag_tests {
    use std::option;
    use sui::object_bag::{
        Self,
        add,
        contains,
        contains_with_type,
        borrow,
        borrow_mut,
        remove,
        value_id,
    };
    use sui::object::{Self, UID};
    use sui::test_scenario as ts;

    struct Counter has key, store {
        id: UID,
        count: u64,
    }

    struct Fake has key, store {
        id: UID,
    }


    #[test]
    fun simple_all_functions() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let bag = object_bag::new(ts::ctx(&mut scenario));
        let counter1 = new(&mut scenario);
        let id1 = object::id(&counter1);
        let counter2 = new(&mut scenario);
        let id2 = object::id(&counter2);
        // add fields
        add(&mut bag, b"hello", counter1);
        add(&mut bag, 1, counter2);
        // check they exist
        assert!(contains(&bag, b"hello"), 0);
        assert!(contains(&bag, 1), 0);
        assert!(contains_with_type<vector<u8>, Counter>(&bag, b"hello"), 0);
        assert!(contains_with_type<u64, Counter>(&bag, 1), 0);
        // check the IDs
        assert!(option::borrow(&value_id(&bag, b"hello")) == &id1, 0);
        assert!(option::borrow(&value_id(&bag, 1)) == &id2, 0);
        // check the values
        assert!(count(borrow(&bag, b"hello")) == 0, 0);
        assert!(count(borrow(&bag, 1)) == 0, 0);
        // mutate them
        bump(borrow_mut(&mut bag, b"hello"));
        bump(bump(borrow_mut(&mut bag, 1)));
        // check the new value
        assert!(count(borrow(&bag, b"hello")) == 1, 0);
        assert!(count(borrow(&bag, 1)) == 2, 0);
        // remove the value and check it
        assert!(destroy(remove(&mut bag, b"hello")) == 1, 0);
        assert!(destroy(remove(&mut bag, 1)) == 2, 0);
        // verify that they are not there
        assert!(!contains(&bag, b"hello"), 0);
        assert!(!contains(&bag, 1), 0);
        ts::end(scenario);
        object_bag::destroy_empty(bag);
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun add_duplicate() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let bag = object_bag::new(ts::ctx(&mut scenario));
        add(&mut bag, b"hello", new(&mut scenario));
        add(&mut bag, b"hello", new(&mut scenario));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let bag = object_bag::new(ts::ctx(&mut scenario));
        borrow<u64, Counter>(&bag, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_mut_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let bag = object_bag::new(ts::ctx(&mut scenario));
        borrow_mut<u64, Counter>(&mut bag, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun remove_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let bag = object_bag::new(ts::ctx(&mut scenario));
        destroy(remove<u64, Counter>(&mut bag, 0));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::object_bag::EBagNotEmpty)]
    fun destroy_non_empty() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let bag = object_bag::new(ts::ctx(&mut scenario));
        let counter = new(&mut scenario);
        add(&mut bag, 0, counter);
        object_bag::destroy_empty(bag);
        ts::end(scenario);
    }

    #[test]
    fun sanity_check_contains() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let bag = object_bag::new(ts::ctx(&mut scenario));
        let counter = new(&mut scenario);
        assert!(!contains(&bag, 0), 0);
        add(&mut bag, 0, counter);
        assert!(contains(&bag, 0), 0);
        assert!(!contains(&bag, 1), 0);
        ts::end(scenario);
        destroy(remove(&mut bag, 0));
        object_bag::destroy_empty(bag)
    }

    #[test]
    fun sanity_check_contains_with_type() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let bag = object_bag::new(ts::ctx(&mut scenario));
        let counter = new(&mut scenario);
        assert!(!contains_with_type<u64, Counter>(&bag, 0), 0);
        assert!(!contains_with_type<u64, Fake>(&bag, 0), 0);
        add(&mut bag, 0, counter);
        assert!(contains_with_type<u64, Counter>(&bag, 0), 0);
        assert!(!contains_with_type<u8, Counter>(&bag, 0), 0);
        assert!(!contains_with_type<u8, Fake>(&bag, 0), 0);
        ts::end(scenario);
        destroy(remove(&mut bag, 0));
        object_bag::destroy_empty(bag)
    }

    #[test]
    fun sanity_check_size() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let bag = object_bag::new(ts::ctx(&mut scenario));
        let counter1 = new(&mut scenario);
        let counter2 = new(&mut scenario);
        assert!(object_bag::is_empty(&bag), 0);
        assert!(object_bag::length(&bag) == 0, 0);
        add(&mut bag, 0, counter1);
        assert!(!object_bag::is_empty(&bag), 0);
        assert!(object_bag::length(&bag) == 1, 0);
        add(&mut bag, 1, counter2);
        assert!(!object_bag::is_empty(&bag), 0);
        assert!(object_bag::length(&bag) == 2, 0);
        ts::end(scenario);
        destroy(remove(&mut bag, 0));
        destroy(remove(&mut bag, 1));
        object_bag::destroy_empty(bag);
    }

    // transfer an object field from one "parent" to another
    #[test]
    fun transfer_object() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let bag1 = object_bag::new(ts::ctx(&mut scenario));
        let bag2 = object_bag::new(ts::ctx(&mut scenario));
        add(&mut bag1, 0, new(&mut scenario));
        assert!(contains(&bag1, 0), 0);
        assert!(!contains(&bag2, 0), 0);
        bump(borrow_mut(&mut bag1, 0));
        let c = remove<u64, Counter>(&mut bag1, 0);
        add(&mut bag2, 0, c);
        assert!(!contains(&bag1, 0), 0);
        assert!(contains(&bag2, 0), 0);
        bump(borrow_mut(&mut bag2, 0));
        assert!(count(borrow(&bag2, 0)) == 2, 0);
        ts::end(scenario);
        destroy(remove(&mut bag2, 0));
        object_bag::destroy_empty(bag1);
        object_bag::destroy_empty(bag2);
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
