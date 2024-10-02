// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::table_tests {
    use sui::table;
    use sui::test_scenario;

    #[test]
    fun simple_all_functions() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = table::new(scenario.ctx());
        // add fields
        table.add(b"hello", 0);
        table.add(b"goodbye", 1);
        // check they exist
        assert!(table.contains(b"hello"));
        assert!(table.contains(b"goodbye"));
        // check the values
        assert!(table[b"hello"] == 0);
        assert!(table[b"goodbye"] == 1);
        // mutate them
        *&mut table[b"hello"] = table[b"hello"] * 2;
        *&mut table[b"goodbye"] = table[b"goodbye"] * 2;
        // check the new value
        assert!(table[b"hello"] == 0);
        assert!(table[b"goodbye"] == 2);
        // remove the value and check it
        assert!(table.remove(b"hello") == 0);
        assert!(table.remove(b"goodbye") == 2);
        // verify that they are not there
        assert!(!table.contains(b"hello"));
        assert!(!table.contains(b"goodbye"));
        scenario.end();
        table.destroy_empty();
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun add_duplicate() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = table::new(scenario.ctx());
        table.add(b"hello", 0);
        table.add(b"hello", 1);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let table = table::new<u64, u64>(scenario.ctx());
        &table[0];
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_mut_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = table::new<u64, u64>(scenario.ctx());
        &mut table[0];
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun remove_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = table::new<u64, u64>(scenario.ctx());
        table.remove(0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::table::ETableNotEmpty)]
    fun destroy_non_empty() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = table::new<u64, u64>(scenario.ctx());
        table.add(0, 0);
        table.destroy_empty();
        scenario.end();
    }

    #[test]
    fun sanity_check_contains() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = table::new<u64, u64>(scenario.ctx());
        assert!(!table.contains(0));
        table.add(0, 0);
        assert!(table.contains<u64, u64>(0));
        assert!(!table.contains<u64, u64>(1));
        scenario.end();
        table.drop();
    }

    #[test]
    fun sanity_check_drop() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = table::new<u64, u64>(scenario.ctx());
        table.add(0, 0);
        assert!(table.length() == 1);
        scenario.end();
        table.drop();
    }

    #[test]
    fun sanity_check_size() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = table::new<u64, u64>(scenario.ctx());
        assert!(table.is_empty());
        assert!(table.length() == 0);
        table.add(0, 0);
        assert!(!table.is_empty());
        assert!(table.length() == 1);
        table.add(1, 0);
        assert!(!table.is_empty());
        assert!(table.length() == 2);
        scenario.end();
        table.drop();
    }
}
