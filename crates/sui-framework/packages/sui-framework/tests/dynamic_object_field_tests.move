// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::dynamic_object_field_tests {
    use sui::dynamic_object_field::{
        add,
        borrow,
        borrow_mut,
        exists_,
        exists_with_type,
        remove,
        id as field_id,
    };
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
        let mut id = scenario.new_object();
        let counter1 = new(&mut scenario);
        let id1 = object::id(&counter1);
        let counter2 = new(&mut scenario);
        let id2 = object::id(&counter2);
        let counter3 = new(&mut scenario);
        let id3 = object::id(&counter3);
        // add fields
        add(&mut id, 0, counter1);
        add(&mut id, b"", counter2);
        add(&mut id, false, counter3);
        // check they exist
        assert!(exists_(&id, 0));
        assert!(exists_(&id, b""));
        assert!(exists_(&id, false));
        // check the IDs
        assert!(field_id(&id, 0).borrow() == &id1);
        assert!(field_id(&id, b"").borrow() == &id2);
        assert!(field_id(&id, false).borrow() == &id3);
        // check the values
        assert!(count(borrow(&id, 0)) == 0);
        assert!(count(borrow(&id, b"")) == 0);
        assert!(count(borrow(&id, false)) == 0);
        // mutate them
        bump(borrow_mut(&mut id, 0));
        bump(bump(borrow_mut(&mut id, b"")));
        bump(bump(bump(borrow_mut(&mut id, false))));
        // check the new value
        assert!(count(borrow(&id, 0)) == 1);
        assert!(count(borrow(&id, b"")) == 2);
        assert!(count(borrow(&id, false)) == 3);
        // remove the value and check it
        assert!(destroy(remove(&mut id, 0)) == 1);
        assert!(destroy(remove(&mut id, b"")) == 2);
        assert!(destroy(remove(&mut id, false)) == 3);
        // verify that they are not there
        assert!(!exists_(&id, 0));
        assert!(!exists_(&id, b""));
        assert!(!exists_(&id, false));
        scenario.end();
        id.delete();
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun add_duplicate() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut id = scenario.new_object();
        add<u64, Counter>(&mut id, 0, new(&mut scenario));
        add<u64, Counter>(&mut id, 0, new(&mut scenario));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun add_duplicate_mismatched_type() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut id = scenario.new_object();
        add<u64, Counter>(&mut id, 0, new(&mut scenario));
        add<u64, Fake>(&mut id, 0, Fake { id: scenario.new_object() });
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let id = scenario.new_object();
        borrow<u64, Counter>(&id, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
    fun borrow_wrong_type() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut id = scenario.new_object();
        add(&mut id, 0, new(&mut scenario));
        borrow<u64, Fake>(&id, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_mut_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut id = scenario.new_object();
        borrow_mut<u64, Counter>(&mut id, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
    fun borrow_mut_wrong_type() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut id = scenario.new_object();
        add(&mut id, 0, new(&mut scenario));
        borrow_mut<u64, Fake>(&mut id, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun remove_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut id = scenario.new_object();
        destroy(remove<u64, Counter>(&mut id, 0));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
    fun remove_wrong_type() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut id = scenario.new_object();
        add(&mut id, 0, new(&mut scenario));
        let Fake { id } = remove<u64, Fake>(&mut id, 0);
        id.delete();
        abort 42
    }

    #[test]
    fun sanity_check_exists() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut id = scenario.new_object();
        assert!(!exists_<u64>(&id, 0));
        add(&mut id, 0, new(&mut scenario));
        assert!(exists_<u64>(&id, 0));
        assert!(!exists_<u8>(&id, 0));
        scenario.end();
        id.delete();
    }

    #[test]
    fun sanity_check_exists_with_type() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut id = scenario.new_object();
        assert!(!exists_with_type<u64, Counter>(&id, 0));
        assert!(!exists_with_type<u64, Fake>(&id, 0));
        add(&mut id, 0, new(&mut scenario));
        assert!(exists_with_type<u64, Counter>(&id, 0));
        assert!(!exists_with_type<u8, Counter>(&id, 0));
        assert!(!exists_with_type<u8, Fake>(&id, 0));
        scenario.end();
        id.delete();
    }

    // should be able to do delete a UID even though it has a dynamic field
    #[test]
    fun delete_uid_with_fields() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut id = scenario.new_object();
        add(&mut id, 0, new(&mut scenario));
        assert!(exists_<u64>(&id, 0));
        scenario.end();
        id.delete();
    }

    // transfer an object field from one "parent" to another
    #[test]
    fun transfer_object() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut id1 = scenario.new_object();
        let mut id2 = scenario.new_object();
        add(&mut id1, 0, new(&mut scenario));
        assert!(exists_<u64>(&id1, 0));
        assert!(!exists_<u64>(&id2, 0));
        bump(borrow_mut(&mut id1, 0));
        let c: Counter = remove(&mut id1, 0);
        add(&mut id2, 0, c);
        assert!(!exists_<u64>(&id1, 0));
        assert!(exists_<u64>(&id2, 0));
        bump(borrow_mut(&mut id2, 0));
        assert!(count(borrow(&id2, 0)) == 2);
        scenario.end();
        id1.delete();
        id2.delete();
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
