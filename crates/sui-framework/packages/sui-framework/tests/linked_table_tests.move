// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::linked_table_tests {
    use sui::linked_table::{
        Self,
        LinkedTable,
    };
    use sui::test_scenario;

    #[test]
    fun simple_all_functions() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new(scenario.ctx());
        check_ordering(&table, &vector[]);
        // add fields
        table.push_back(b"hello", 0);
        check_ordering(&table, &vector[b"hello"]);
        table.push_back(b"goodbye", 1);
        // check they exist
        assert!(table.contains(b"hello"));
        assert!(table.contains(b"goodbye"));
        assert!(!table.is_empty());
        // check the values
        assert!(table[b"hello"] == 0);
        assert!(table[b"goodbye"] == 1);
        // mutate them
        *(&mut table[b"hello"]) = table[b"hello"] * 2;
        *(&mut table[b"goodbye"]) = table[b"goodbye"] * 2;
        // check the new value
        assert!(table[b"hello"] == 0);
        assert!(table[b"goodbye"] == 2);
        // check the ordering
        check_ordering(&table, &vector[b"hello", b"goodbye"]);
        // add to the front
        table.push_front(b"!!!", 2);
        check_ordering(&table, &vector[b"!!!", b"hello", b"goodbye"]);
        // add to the back
        table.push_back(b"?", 3);
        check_ordering(&table, &vector[b"!!!", b"hello", b"goodbye", b"?"]);
        // pop front
        let (front_k, front_v) = table.pop_front();
        assert!(front_k == b"!!!");
        assert!(front_v == 2);
        check_ordering(&table, &vector[b"hello", b"goodbye", b"?"]);
        // remove middle
        assert!(table.remove(b"goodbye") == 2);
        check_ordering(&table, &vector[b"hello", b"?"]);
        // pop back
        let (back_k, back_v) = table.pop_back();
        assert!(back_k == b"?");
        assert!(back_v == 3);
        check_ordering(&table, &vector[b"hello"]);
        // remove the value and check it
        assert!(table.remove(b"hello") == 0);
        check_ordering(&table, &vector[]);
        // verify that they are not there
        assert!(!table.contains(b"!!!"));
        assert!(!table.contains(b"goodbye"));
        assert!(!table.contains(b"hello"));
        assert!(!table.contains(b"?"));
        assert!(table.is_empty());
        scenario.end();
        table.destroy_empty();
    }

    #[test]
    fun front_back_empty() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let table = linked_table::new<u64, u64>(scenario.ctx());
        assert!(table.front().is_none());
        assert!(table.back().is_none());
        scenario.end();
        table.drop()
    }

    #[test]
    fun push_front_singleton() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new(scenario.ctx());
        check_ordering(&table, &vector[]);
        table.push_front(b"hello", 0);
        assert!(table.contains(b"hello"));
        check_ordering(&table, &vector[b"hello"]);
        scenario.end();
        table.drop()
    }

    #[test]
    fun push_back_singleton() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new(scenario.ctx());
        check_ordering(&table, &vector[]);
        table.push_back(b"hello", 0);
        assert!(table.contains(b"hello"));
        check_ordering(&table, &vector[b"hello"]);
        scenario.end();
        table.drop()
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun push_front_duplicate() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new(scenario.ctx());
        table.push_front(b"hello", 0);
        table.push_front(b"hello", 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun push_back_duplicate() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new(scenario.ctx());
        table.push_back(b"hello", 0);
        table.push_back(b"hello", 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun push_mixed_duplicate() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new(scenario.ctx());
        table.push_back(b"hello", 0);
        table.push_front(b"hello", 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let table = linked_table::new<u64, u64>(scenario.ctx());
        &table[0];
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_mut_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new<u64, u64>(scenario.ctx());
        &mut table[0];
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun remove_missing() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new<u64, u64>(scenario.ctx());
        table.remove(0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = linked_table::ETableIsEmpty)]
    fun pop_front_empty() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new<u64, u64>(scenario.ctx());
        table.pop_front();
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = linked_table::ETableIsEmpty)]
    fun pop_back_empty() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new<u64, u64>(scenario.ctx());
        table.pop_back();
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = linked_table::ETableNotEmpty)]
    fun destroy_non_empty() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new(scenario.ctx());
        table.push_back(0, 0);
        table.destroy_empty();
        scenario.end();
    }

    #[test]
    fun sanity_check_contains() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new(scenario.ctx());
        assert!(!table.contains(0));
        table.push_back(0, 0);
        assert!(table.contains<u64, u64>(0));
        assert!(!table.contains<u64, u64>(1));
        scenario.end();
        table.drop();
    }

    #[test]
    fun sanity_check_drop() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new(scenario.ctx());
        table.push_back(0, 0);
        assert!(table.length() == 1);
        scenario.end();
        table.drop();
    }

    #[test]
    fun sanity_check_size() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut table = linked_table::new(scenario.ctx());
        assert!(table.is_empty());
        assert!(table.length() == 0);
        table.push_back(0, 0);
        assert!(!table.is_empty());
        assert!(table.length() == 1);
        table.push_back(1, 0);
        assert!(!table.is_empty());
        assert!(table.length() == 2);
        scenario.end();
        table.drop();
    }

    #[test]
    fun test_all_orderings() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let ctx = scenario.ctx();
        let keys = vector[b"a", b"b", b"c"];
        let values = vector[3, 2, 1];
        let all_bools = vector[
            vector[true, true, true],
            vector[true, false, true],
            vector[true, true, false],
            vector[true, false, false],
            vector[false, false, true],
            vector[false, false, false],
        ];
        let mut i = 0;
        let mut j = 0;
        let n = all_bools.length();
        // all_bools indicate possible orderings of accessing the front vs the back of the
        // table
        // test all orderings of building up and tearing down the table, while mimicking
        // the ordering in a vector, and checking the keys have the same order in the table
        while (i < n) {
            let pushes = &all_bools[i];
            while (j < n) {
                let pops = &all_bools[j];
                build_up_and_tear_down(&keys, &values, pushes, pops, ctx);
                j = j + 1;
            };
            i = i + 1;
        };
        scenario.end();
    }

    fun build_up_and_tear_down<K: copy + drop + store, V: copy + drop + store>(
        keys: &vector<K>,
        values: &vector<V>,
        // true for front, false for back
        pushes: &vector<bool>,
        // true for front, false for back
        pops: &vector<bool>,
        ctx: &mut TxContext,
    ) {
        let mut table = linked_table::new(ctx);
        let n = keys.length();
        assert!(values.length() == n);
        assert!(pushes.length() == n);
        assert!(pops.length() == n);

        let mut i = 0;
        let mut order = vector[];
        while (i < n) {
            let k = keys[i];
            let v = values[i];
            if (pushes[i]) {
                table.push_front(k, v);
                order.insert(k, 0);
            } else {
                table.push_front(k, v);
                order.push_back(k);
            };
            i = i + 1;
        };

        check_ordering(&table, &order);
        let mut i = 0;
        while (i < n) {
            let (table_k, order_k) = if (pops[i]) {
                let (table_k, _) = table.pop_front();
                (table_k, order.remove(0))
            } else {
                let (table_k, _) = table.pop_back();
                (table_k, order.pop_back())
            };
            assert!(table_k == order_k);
            check_ordering(&table, &order);
            i = i + 1;
        };
        table.destroy_empty()
    }

    fun check_ordering<K: copy + drop + store, V: store>(table: &LinkedTable<K, V>, keys: &vector<K>) {
        let n = table.length();
        assert!(n == keys.length());
        if (n == 0) {
            assert!(table.front().is_none());
            assert!(table.back().is_none());
            return
        };

        let mut i = 0;
        while (i < n) {
            let cur = keys[i];
            if (i == 0) {
                assert!(table.front().borrow() == &cur);
                assert!(table.prev(cur).is_none());
            } else {
                assert!(table.prev(cur).borrow() == &keys[i - 1]);
            };
            if (i + 1 == n) {
                assert!(table.back().borrow() == &cur);
                assert!(table.next(cur).is_none());
            } else {
                assert!(table.next(cur).borrow() == &keys[i + 1]);
            };

            i = i + 1;
        }
    }
}
