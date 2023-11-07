// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::linked_table_tests {
    use std::option;
    use std::vector;
    use sui::linked_table::{
        Self,
        LinkedTable,
        front,
        back,
        push_front,
        push_back,
        borrow,
        borrow_mut,
        prev,
        next,
        remove,
        pop_front,
        pop_back,
        contains,
        length,
        is_empty,
        destroy_empty,
        drop,
    };
    use sui::test_scenario as ts;
    use sui::tx_context::TxContext;

    #[test]
    fun simple_all_functions() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new(ts::ctx(&mut scenario));
        check_ordering(&table, &vector[]);
        // add fields
        push_back(&mut table, b"hello", 0);
        check_ordering(&table, &vector[b"hello"]);
        push_back(&mut table, b"goodbye", 1);
        // check they exist
        assert!(contains(&table, b"hello"), 0);
        assert!(contains(&table, b"goodbye"), 0);
        assert!(!is_empty(&table), 0);
        // check the values
        assert!(*borrow(&table, b"hello") == 0, 0);
        assert!(*borrow(&table, b"goodbye") == 1, 0);
        // mutate them
        *borrow_mut(&mut table, b"hello") = *borrow(&table, b"hello") * 2;
        *borrow_mut(&mut table, b"goodbye") = *borrow(&table, b"goodbye") * 2;
        // check the new value
        assert!(*borrow(&table, b"hello") == 0, 0);
        assert!(*borrow(&table, b"goodbye") == 2, 0);
        // check the ordering
        check_ordering(&table, &vector[b"hello", b"goodbye"]);
        // add to the front
        push_front(&mut table, b"!!!", 2);
        check_ordering(&table, &vector[b"!!!", b"hello", b"goodbye"]);
        // add to the back
        push_back(&mut table, b"?", 3);
        check_ordering(&table, &vector[b"!!!", b"hello", b"goodbye", b"?"]);
        // pop front
        let (front_k, front_v) = pop_front(&mut table);
        assert!(front_k == b"!!!", 0);
        assert!(front_v == 2, 0);
        check_ordering(&table, &vector[b"hello", b"goodbye", b"?"]);
        // remove middle
        assert!(remove(&mut table, b"goodbye") == 2, 0);
        check_ordering(&table, &vector[b"hello", b"?"]);
        // pop back
        let (back_k, back_v) = pop_back(&mut table);
        assert!(back_k == b"?", 0);
        assert!(back_v == 3, 0);
        check_ordering(&table, &vector[b"hello"]);
        // remove the value and check it
        assert!(remove(&mut table, b"hello") == 0, 0);
        check_ordering(&table, &vector[]);
        // verify that they are not there
        assert!(!contains(&table, b"!!!"), 0);
        assert!(!contains(&table, b"goodbye"), 0);
        assert!(!contains(&table, b"hello"), 0);
        assert!(!contains(&table, b"?"), 0);
        assert!(is_empty(&table), 0);
        ts::end(scenario);
        destroy_empty(table);
    }

    #[test]
    fun front_back_empty() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new<u64, u64>(ts::ctx(&mut scenario));
        assert!(option::is_none(front(&table)), 0);
        assert!(option::is_none(back(&table)), 0);
        ts::end(scenario);
        drop(table)
    }

    #[test]
    fun push_front_singleton() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new(ts::ctx(&mut scenario));
        check_ordering(&table, &vector[]);
        push_front(&mut table, b"hello", 0);
        assert!(contains(&table, b"hello"), 0);
        check_ordering(&table, &vector[b"hello"]);
        ts::end(scenario);
        drop(table)
    }

    #[test]
    fun push_back_singleton() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new(ts::ctx(&mut scenario));
        check_ordering(&table, &vector[]);
        push_back(&mut table, b"hello", 0);
        assert!(contains(&table, b"hello"), 0);
        check_ordering(&table, &vector[b"hello"]);
        ts::end(scenario);
        drop(table)
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun push_front_duplicate() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new(ts::ctx(&mut scenario));
        push_front(&mut table, b"hello", 0);
        push_front(&mut table, b"hello", 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun push_back_duplicate() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new(ts::ctx(&mut scenario));
        push_back(&mut table, b"hello", 0);
        push_back(&mut table, b"hello", 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun push_mixed_duplicate() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new(ts::ctx(&mut scenario));
        push_back(&mut table, b"hello", 0);
        push_front(&mut table, b"hello", 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new<u64, u64>(ts::ctx(&mut scenario));
        borrow(&table, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_mut_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new<u64, u64>(ts::ctx(&mut scenario));
        borrow_mut(&mut table, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun remove_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new<u64, u64>(ts::ctx(&mut scenario));
        remove(&mut table, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = linked_table::ETableIsEmpty)]
    fun pop_front_empty() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new<u64, u64>(ts::ctx(&mut scenario));
        pop_front(&mut table);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = linked_table::ETableIsEmpty)]
    fun pop_back_empty() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new<u64, u64>(ts::ctx(&mut scenario));
        pop_back(&mut table);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = linked_table::ETableNotEmpty)]
    fun destroy_non_empty() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new(ts::ctx(&mut scenario));
        push_back(&mut table, 0, 0);
        destroy_empty(table);
        ts::end(scenario);
    }

    #[test]
    fun sanity_check_contains() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new(ts::ctx(&mut scenario));
        assert!(!contains(&table, 0), 0);
        push_back(&mut table, 0, 0);
        assert!(contains<u64, u64>(&table, 0), 0);
        assert!(!contains<u64, u64>(&table, 1), 0);
        ts::end(scenario);
        drop(table);
    }

    #[test]
    fun sanity_check_drop() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new(ts::ctx(&mut scenario));
        push_back(&mut table, 0, 0);
        assert!(length(&table) == 1, 0);
        ts::end(scenario);
        drop(table);
    }

    #[test]
    fun sanity_check_size() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let table = linked_table::new(ts::ctx(&mut scenario));
        assert!(is_empty(&table), 0);
        assert!(length(&table) == 0, 0);
        push_back(&mut table, 0, 0);
        assert!(!is_empty(&table), 0);
        assert!(length(&table) == 1, 0);
        push_back(&mut table, 1, 0);
        assert!(!is_empty(&table), 0);
        assert!(length(&table) == 2, 0);
        ts::end(scenario);
        drop(table);
    }

    #[test]
    fun test_all_orderings() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let ctx = ts::ctx(&mut scenario);
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
        let i = 0;
        let j = 0;
        let n = vector::length(&all_bools);
        // all_bools indicate possible orderings of accessing the front vs the back of the
        // table
        // test all orderings of building up and tearing down the table, while mimicing
        // the ordering in a vector, and checking the keys have the same order in the table
        while (i < n) {
            let pushes = vector::borrow(&all_bools, i);
            while (j < n) {
                let pops = vector::borrow(&all_bools, j);
                build_up_and_tear_down(&keys, &values, pushes, pops, ctx);
                j = j + 1;
            };
            i = i + 1;
        };
        ts::end(scenario);
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
        let table = linked_table::new(ctx);
        let n = vector::length(keys);
        assert!(vector::length(values) == n, 0);
        assert!(vector::length(pushes) == n, 0);
        assert!(vector::length(pops) == n, 0);

        let i = 0;
        let order = vector[];
        while (i < n) {
            let k = *vector::borrow(keys, i);
            let v = *vector::borrow(values, i);
            if (*vector::borrow(pushes, i)) {
                push_front(&mut table, k, v);
                vector::insert(&mut order, k, 0);
            } else {
                push_front(&mut table, k, v);
                vector::push_back(&mut order, k);
            };
            i = i + 1;
        };

        check_ordering(&table, &order);
        let i = 0;
        while (i < n) {
            let (table_k, order_k) = if (*vector::borrow(pops, i)) {
                let (table_k, _) = pop_front(&mut table);
                (table_k, vector::remove(&mut order, 0))
            } else {
                let (table_k, _) = pop_back(&mut table);
                (table_k, vector::pop_back(&mut order))
            };
            assert!(table_k == order_k, 0);
            check_ordering(&table, &order);
            i = i + 1;
        };
        destroy_empty(table)
    }

    fun check_ordering<K: copy + drop + store, V: store>(table: &LinkedTable<K, V>, keys: &vector<K>) {
        let n = length(table);
        assert!(n == vector::length(keys), 0);
        if (n == 0) {
            assert!(option::is_none(front(table)), 0);
            assert!(option::is_none(back(table)), 0);
            return
        };

        let i = 0;
        while (i < n) {
            let cur = *vector::borrow(keys, i);
            if (i == 0) {
                assert!(option::borrow(front(table)) == &cur, 0);
                assert!(option::is_none(prev(table, cur)), 0);
            } else {
                assert!(option::borrow(prev(table, cur)) == vector::borrow(keys, i - 1), 0);
            };
            if (i + 1 == n) {
                assert!(option::borrow(back(table)) == &cur, 0);
                assert!(option::is_none(next(table, cur)), 0);
            } else {
                assert!(option::borrow(next(table, cur)) == vector::borrow(keys, i + 1), 0);
            };

            i = i + 1;
        }
    }
}
