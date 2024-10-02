// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::object_bag_tests {
    use sui::object_bag;
    use sui::test_scenario;

    public struct Counter has key, store {
        id: UID,
        count: u64,
    }

    public struct Fake has key, store {
        id: UID,
    }


    #[test]
    fun simple_all_functions() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = object_bag::new(scenario.ctx());
        let counter1 = new(&mut scenario);
        let id1 = object::id(&counter1);
        let counter2 = new(&mut scenario);
        let id2 = object::id(&counter2);
        // add fields
        bag.add(b"hello", counter1);
        bag.add(1u64, counter2);
        // check they exist
        assert!(bag.contains(b"hello"));
        assert!(bag.contains(1));
        assert!(bag.contains_with_type<vector<u8>, Counter>(b"hello"));
        assert!(bag.contains_with_type<u64, Counter>(1));
        // check the IDs
        assert!(bag.value_id(b"hello").borrow() == &id1);
        assert!(bag.value_id(1).borrow() == &id2);
        // check the values
        assert!((&bag[b"hello"] : &Counter).count() == 0);
        assert!((&bag[1] : &Counter).count() == 0);
        // mutate them
        bump(&mut bag[b"hello"]);
        bump(bump(&mut bag[1]));
        // check the new value
        assert!((&bag[b"hello"] : &Counter).count() == 1);
        assert!((&bag[1] : &Counter).count() == 2);
        // remove the value and check it
        assert!(bag.remove<vector<u8>, Counter>(b"hello").destroy() == 1);
        assert!(bag.remove<u64, Counter>(1).destroy() == 2);
        // verify that they are not there
        assert!(!bag.contains(b"hello"));
        assert!(!bag.contains(1));
        scenario.end();
        bag.destroy_empty();
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun add_duplicate() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = object_bag::new(scenario.ctx());
        bag.add(b"hello", new(&mut scenario));
        bag.add(b"hello", new(&mut scenario));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let bag = object_bag::new(scenario.ctx());
        let _ : &Counter = &bag[0];
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_mut_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = object_bag::new(scenario.ctx());
        let _ : &mut Counter = &mut bag[0];
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun remove_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = object_bag::new(scenario.ctx());
        bag.remove<u64, Counter>(0).destroy();
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::object_bag::EBagNotEmpty)]
    fun destroy_non_empty() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = object_bag::new(scenario.ctx());
        let counter = new(&mut scenario);
        bag.add(0, counter);
        bag.destroy_empty();
        scenario.end();
    }

    #[test]
    fun sanity_check_contains() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = object_bag::new(scenario.ctx());
        let counter = new(&mut scenario);
        assert!(!bag.contains(0));
        bag.add(0, counter);
        assert!(bag.contains(0));
        assert!(!bag.contains(1));
        scenario.end();
        bag.remove<u64, Counter>(0).destroy();
        bag.destroy_empty()
    }

    #[test]
    fun sanity_check_contains_with_type() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = object_bag::new(scenario.ctx());
        let counter = new(&mut scenario);
        assert!(!bag.contains_with_type<u64, Counter>(0));
        assert!(!bag.contains_with_type<u64, Fake>(0));
        bag.add(0, counter);
        assert!(bag.contains_with_type<u64, Counter>(0));
        assert!(!bag.contains_with_type<u8, Counter>(0));
        assert!(!bag.contains_with_type<u8, Fake>(0));
        scenario.end();
        bag.remove<u64, Counter>(0).destroy();
        bag.destroy_empty()
    }

    #[test]
    fun sanity_check_size() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = object_bag::new(scenario.ctx());
        let counter1 = new(&mut scenario);
        let counter2 = new(&mut scenario);
        assert!(bag.is_empty());
        assert!(bag.length() == 0);
        bag.add(0, counter1);
        assert!(!bag.is_empty());
        assert!(bag.length() == 1);
        bag.add(1, counter2);
        assert!(!bag.is_empty());
        assert!(bag.length() == 2);
        scenario.end();
        bag.remove<u64, Counter>(0).destroy();
        bag.remove<u64, Counter>(1).destroy();
        bag.destroy_empty();
    }

    // transfer an object field from one "parent" to another
    #[test]
    fun transfer_object() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag1 = object_bag::new(scenario.ctx());
        let mut bag2 = object_bag::new(scenario.ctx());
        bag1.add(0, new(&mut scenario));
        assert!(bag1.contains(0));
        assert!(!bag2.contains(0));
        bump(&mut bag1[0]);
        let c = bag1.remove<u64, Counter>(0);
        bag2.add(0, c);
        assert!(!bag1.contains(0));
        assert!(bag2.contains(0));
        bump(&mut bag2[0]);
        assert!((&bag2[0] : &Counter).count() == 2);
        scenario.end();
        (bag2.remove(0) : Counter).destroy();
        bag1.destroy_empty();
        bag2.destroy_empty();
    }

    fun new(scenario: &mut test_scenario::Scenario): Counter {
        Counter { id: scenario.new_object(), count: 0 }
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
        id.delete();
        count
    }
}
