// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::bag_tests {
    use sui::bag::Self;
    use sui::test_scenario;

    #[test]
    fun simple_all_functions() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = bag::new(scenario.ctx());
        // add fields
        bag.add(b"hello", 0);
        bag.add(1, 1u8);
        // check they exist
        assert!(bag.contains_with_type<vector<u8>, u64>(b"hello"));
        assert!(bag.contains_with_type<u64, u8>(1));
        // check the values
        assert!(bag[b"hello"] == 0);
        assert!(bag[1] == 1u8);
        // mutate them
        *(&mut bag[b"hello"]) = bag[b"hello"] * 2;
        *(&mut bag[1]) = bag[1] * 2u8;
        // check the new value
        assert!(bag[b"hello"] == 0);
        assert!(bag[1] == 2u8);
        // remove the value and check it
        assert!(bag.remove(b"hello") == 0);
        assert!(bag.remove(1) == 2u8);
        // verify that they are not there
        assert!(!bag.contains_with_type<vector<u8>, u64>(b"hello"));
        assert!(!bag.contains_with_type<u64, u8>(1));
        scenario.end();
        bag.destroy_empty();
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun add_duplicate() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = bag::new(scenario.ctx());
        bag.add(b"hello", 0u8);
        bag.add(b"hello", 1u8);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun add_duplicate_mismatched_type() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = bag::new(scenario.ctx());
        bag.add(b"hello", 0u128);
        bag.add(b"hello", 1u8);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let bag = bag::new(scenario.ctx());
        (&bag[0] : &u64);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
    fun borrow_wrong_type() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = bag::new(scenario.ctx());
        bag.add(0u64, 0u64);
        (&bag[0]: &u8);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_mut_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = bag::new(scenario.ctx());
        (&mut bag[0]: &mut u64);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
    fun borrow_mut_wrong_type() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = bag::new(scenario.ctx());
        bag.add(0u64, 0u64);
        (&mut bag[0]: &mut u8);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun remove_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = bag::new(scenario.ctx());
        bag.remove<u64, u64>(0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
    fun remove_wrong_type() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = bag::new(scenario.ctx());
        bag.add(0, 0);
        bag.remove<u64, u8>(0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::bag::EBagNotEmpty)]
    fun destroy_non_empty() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = bag::new(scenario.ctx());
        bag.add(0, 0);
        bag.destroy_empty();
        scenario.end();
    }

    #[test]
    fun sanity_check_contains() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = bag::new(scenario.ctx());
        assert!(!bag.contains_with_type<u64, u64>(0));
        bag.add(0, 0);
        assert!(bag.contains_with_type<u64, u64>(0));
        assert!(!bag.contains_with_type<u64, u64>(1));
        scenario.end();
        bag.remove<u64, u64>(0);
        bag.destroy_empty();
    }

    #[test]
    fun sanity_check_size() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut bag = bag::new(scenario.ctx());
        assert!(bag.is_empty());
        assert!(bag.length() == 0);
        bag.add(0, 0);
        assert!(!bag.is_empty());
        assert!(bag.length() == 1);
        bag.add(1, 0);
        assert!(!bag.is_empty());
        assert!(bag.length() == 2);
        bag.remove<u64, u64>(0);
        bag.remove<u64, u64>(1);
        scenario.end();
        bag.destroy_empty();
    }
}
