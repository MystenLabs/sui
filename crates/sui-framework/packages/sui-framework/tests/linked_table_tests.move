// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::linked_table_tests;

use std::unit_test::assert_eq;
use sui::linked_table::{Self, LinkedTable};

#[test]
fun simple_all_functions() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    check_ordering(&table, vector[], option::none());
    // add fields
    table.push_back("hello", 0);
    check_ordering(&table, vector["hello"], option::some(vector[0]));
    table.push_back("goodbye", 1);
    // check they exist
    assert!(table.contains("hello"));
    assert!(table.contains("goodbye"));
    assert!(!table.is_empty());
    // check the values
    assert_eq!(table["hello"], 0);
    assert_eq!(table["goodbye"], 1);
    // mutate them
    *(&mut table["hello"]) = table["hello"] * 2;
    *(&mut table["goodbye"]) = table["goodbye"] * 2;
    // check the new value
    assert_eq!(table["hello"], 0);
    assert_eq!(table["goodbye"], 2);
    // check the ordering
    check_ordering(&table, vector["hello", "goodbye"], option::some(vector[0, 2]));
    // add to the front
    table.push_front("!!!", 2);
    check_ordering(&table, vector["!!!", "hello", "goodbye"], option::some(vector[2, 0, 2]));
    // add to the back
    table.push_back("?", 3);
    check_ordering(
        &table,
        vector["!!!", "hello", "goodbye", "?"],
        option::some(vector[2, 0, 2, 3]),
    );
    // pop front
    let (front_k, front_v) = table.pop_front();
    assert_eq!(front_k, "!!!");
    assert_eq!(front_v, 2);
    check_ordering(&table, vector["hello", "goodbye", "?"], option::some(vector[0, 2, 3]));
    // remove middle
    assert_eq!(table.remove("goodbye"), 2);
    check_ordering(&table, vector["hello", "?"], option::some(vector[0, 3]));
    // pop back
    let (back_k, back_v) = table.pop_back();
    assert_eq!(back_k, "?");
    assert_eq!(back_v, 3);
    check_ordering(&table, vector["hello"], option::some(vector[0]));
    // remove the value and check it
    assert_eq!(table.remove("hello"), 0);
    check_ordering(&table, vector[], option::none());
    // verify that they are not there
    assert!(!table.contains("!!!"));
    assert!(!table.contains("goodbye"));
    assert!(!table.contains("hello"));
    assert!(!table.contains("?"));
    assert!(table.is_empty());
    table.destroy_empty();
}

#[test]
fun front_back_empty() {
    let ctx = &mut tx_context::dummy();
    let table = linked_table::new<u64, u64>(ctx);
    assert!(table.front().is_none());
    assert!(table.back().is_none());
    table.drop()
}

#[test]
fun push_front_singleton() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    check_ordering(&table, vector[], option::none());
    table.push_front("hello", 0u64);
    assert!(table.contains("hello"));
    check_ordering(&table, vector["hello"], option::none());
    table.drop()
}

#[test]
fun push_back_singleton() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    check_ordering(&table, vector[], option::none());
    table.push_back("hello", 0u64);
    assert!(table.contains("hello"));
    check_ordering(&table, vector["hello"], option::none());
    table.drop()
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun push_front_duplicate() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_front("hello", 0);
    table.push_front("hello", 0u64);
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun push_back_duplicate() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("hello", 0u64);
    table.push_back("hello", 0);
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun push_mixed_duplicate() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("hello", 0u64);
    table.push_front("hello", 0);
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun borrow_missing() {
    let ctx = &mut tx_context::dummy();
    let table = linked_table::new<u64, u64>(ctx);
    &table[0];
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun borrow_mut_missing() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<u64, u64>(ctx);
    &mut table[0];
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun remove_missing() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<u64, u64>(ctx);
    table.remove(0);
    abort
}

#[test, expected_failure(abort_code = linked_table::ETableIsEmpty)]
fun pop_front_empty() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<u64, u64>(ctx);
    table.pop_front();
    abort
}

#[test, expected_failure(abort_code = linked_table::ETableIsEmpty)]
fun pop_back_empty() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<u64, u64>(ctx);
    table.pop_back();
    abort
}

#[test, expected_failure(abort_code = linked_table::ETableNotEmpty)]
fun destroy_non_empty() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new(ctx);
    table.push_back(0u64, 0u64);
    table.destroy_empty();
}

#[test]
fun sanity_check_contains() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new(ctx);
    assert!(!table.contains(0));
    table.push_back(0, 0);
    assert!(table.contains<u64, u64>(0));
    assert!(!table.contains<u64, u64>(1));
    table.drop();
}

#[test]
fun sanity_check_drop() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new(ctx);
    table.push_back(0u64, 0u64);
    assert_eq!(table.length(), 1);
    table.drop();
}

#[test]
fun sanity_check_size() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new(ctx);
    assert!(table.is_empty());
    assert_eq!(table.length(), 0);
    table.push_back(0u64, 0u64);
    assert!(!table.is_empty());
    assert_eq!(table.length(), 1);
    table.push_back(1, 0);
    assert!(!table.is_empty());
    assert_eq!(table.length(), 2);
    table.drop();
}

#[test]
fun insert_before_singleton() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("anchor", 1);
    table.insert_before("anchor", "new", 0);
    check_ordering(&table, vector["new", "anchor"], option::some(vector[0, 1]));
    assert_eq!(table.length(), 2);
    assert_eq!(*table.front().borrow(), "new");
    assert_eq!(*table.back().borrow(), "anchor");
    table.drop();
}

#[test]
fun insert_before_head() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.push_back("b", 1);
    table.push_back("c", 2);
    table.insert_before("a", "z", 99);
    check_ordering(&table, vector["z", "a", "b", "c"], option::some(vector[99, 0, 1, 2]));
    assert_eq!(*table.front().borrow(), "z");
    assert_eq!(*table.back().borrow(), "c");
    assert_eq!(table.length(), 4);
    table.drop();
}

#[test]
fun insert_before_middle() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0u64);
    table.push_back("b", 1);
    table.push_back("c", 2);
    table.insert_before("b", "m", 50);
    check_ordering(&table, vector["a", "m", "b", "c"], option::some(vector[0, 50, 1, 2]));
    assert_eq!(*table.front().borrow(), "a");
    assert_eq!(*table.back().borrow(), "c");
    table.drop();
}

#[test]
fun insert_before_tail() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0u64);
    table.push_back("b", 1);
    table.push_back("c", 2);
    table.insert_before("c", "x", 75);
    check_ordering(&table, vector["a", "b", "x", "c"], option::some(vector[0, 1, 75, 2]));
    assert_eq!(*table.front().borrow(), "a");
    assert_eq!(*table.back().borrow(), "c");
    table.drop();
}

#[test]
fun insert_after_singleton() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("anchor", 1u64);
    table.insert_after("anchor", "new", 0);
    check_ordering(&table, vector["anchor", "new"], option::some(vector[1, 0]));
    assert_eq!(table.length(), 2);
    assert_eq!(*table.front().borrow(), "anchor");
    assert_eq!(*table.back().borrow(), "new");
    table.drop();
}

#[test]
fun insert_after_head() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.push_back("b", 1);
    table.push_back("c", 2);
    table.insert_after("a", "x", 25);
    check_ordering(&table, vector["a", "x", "b", "c"], option::some(vector[0, 25, 1, 2]));
    assert_eq!(*table.front().borrow(), "a");
    assert_eq!(*table.back().borrow(), "c");
    table.drop();
}

#[test]
fun insert_after_middle() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.push_back("b", 1);
    table.push_back("c", 2);
    table.insert_after("b", "m", 50);
    check_ordering(&table, vector["a", "b", "m", "c"], option::some(vector[0, 1, 50, 2]));
    assert_eq!(*table.front().borrow(), "a");
    assert_eq!(*table.back().borrow(), "c");
    table.drop();
}

#[test]
fun insert_after_tail() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0u64);
    table.push_back("b", 1);
    table.push_back("c", 2);
    table.insert_after("c", "z", 99);
    check_ordering(&table, vector["a", "b", "c", "z"], option::some(vector[0, 1, 2, 99]));
    assert_eq!(*table.front().borrow(), "a");
    assert_eq!(*table.back().borrow(), "z");
    table.drop();
}

#[test]
fun push_front_after_insert_before_head() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.push_back("b", 1);
    table.insert_before("a", "z", 99);
    check_ordering(&table, vector["z", "a", "b"], option::some(vector[99, 0, 1]));
    table.push_front("y", 100);
    check_ordering(&table, vector["y", "z", "a", "b"], option::some(vector[100, 99, 0, 1]));
    table.push_back("c", 2);
    check_ordering(&table, vector["y", "z", "a", "b", "c"], option::some(vector[100, 99, 0, 1, 2]));
    table.drop();
}

#[test]
fun push_back_after_insert_after_tail() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.push_back("b", 1);
    table.insert_after("b", "z", 99);
    check_ordering(&table, vector["a", "b", "z"], option::some(vector[0, 1, 99]));
    table.push_back("y", 100);
    check_ordering(&table, vector["a", "b", "z", "y"], option::some(vector[0, 1, 99, 100]));
    table.push_front("x", 101);
    check_ordering(
        &table,
        vector["x", "a", "b", "z", "y"],
        option::some(vector[101, 0, 1, 99, 100]),
    );
    table.drop();
}

#[test]
fun push_after_insert_at_singleton_boundary() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0u64);
    table.insert_before("a", "b", 1);
    check_ordering(&table, vector["b", "a"], option::some(vector[1, 0]));
    table.push_front("c", 2);
    check_ordering(&table, vector["c", "b", "a"], option::some(vector[2, 1, 0]));
    table.push_back("d", 3);
    check_ordering(&table, vector["c", "b", "a", "d"], option::some(vector[2, 1, 0, 3]));

    let mut table2 = linked_table::new<vector<u8>, u64>(ctx);
    table2.push_back("a", 0u64);
    table2.insert_after("a", "b", 1);
    check_ordering(&table2, vector["a", "b"], option::none());
    table2.push_back("c", 2);
    check_ordering(&table2, vector["a", "b", "c"], option::none());
    table2.push_front("d", 3);
    check_ordering(&table2, vector["d", "a", "b", "c"], option::none());
    table.drop();
    table2.drop();
}

#[test]
fun insert_before_repeated_at_head() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("anchor", 0);
    table.insert_before("anchor", "a", 1);
    table.insert_before("a", "b", 2);
    table.insert_before("b", "c", 3);
    check_ordering(&table, vector["c", "b", "a", "anchor"], option::some(vector[3, 2, 1, 0]));
    assert_eq!(table.length(), 4);
    table.drop();
}

#[test]
fun insert_after_repeated_at_tail() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("anchor", 0);
    table.insert_after("anchor", "a", 1);
    table.insert_after("a", "b", 2);
    table.insert_after("b", "c", 3);
    check_ordering(&table, vector["anchor", "a", "b", "c"], option::none());
    assert_eq!(table.length(), 4);
    table.drop();
}

#[test]
fun insert_before_and_after_interleaved() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("m", 0);
    table.insert_before("m", "l", 1);
    check_ordering(&table, vector["l", "m"], option::none());
    table.insert_after("m", "n", 2);
    check_ordering(&table, vector["l", "m", "n"], option::none());
    table.insert_before("l", "k", 3);
    check_ordering(&table, vector["k", "l", "m", "n"], option::none());
    table.insert_after("n", "o", 4);
    check_ordering(&table, vector["k", "l", "m", "n", "o"], option::none());
    table.insert_after("l", "l2", 5);
    check_ordering(&table, vector["k", "l", "l2", "m", "n", "o"], option::none());
    table.insert_before("n", "m2", 6);
    check_ordering(&table, vector["k", "l", "l2", "m", "m2", "n", "o"], option::none());
    assert_eq!(table.length(), 7);
    assert_eq!(*table.front().borrow(), "k");
    assert_eq!(*table.back().borrow(), "o");
    table.drop();
}

#[test]
fun insert_then_remove_preserves_ordering() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.push_back("c", 2);
    table.insert_after("a", "b", 1);
    check_ordering(&table, vector["a", "b", "c"], option::none());
    assert_eq!(table.remove("b"), 1);
    check_ordering(&table, vector["a", "c"], option::none());
    table.insert_before("c", "b", 1);
    check_ordering(&table, vector["a", "b", "c"], option::none());
    assert_eq!(table.remove("a"), 0);
    check_ordering(&table, vector["b", "c"], option::none());
    assert_eq!(table.remove("c"), 2);
    check_ordering(&table, vector["b"], option::none());
    assert_eq!(table.remove("b"), 1);
    check_ordering(&table, vector[], option::none());
    table.destroy_empty();
}

#[test]
fun insert_before_then_pop() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("b", 1u64);
    table.insert_before("b", "a", 0);
    let (k, v) = table.pop_front();
    assert_eq!(k, "a");
    assert_eq!(v, 0);
    check_ordering(&table, vector["b"], option::none());
    let (k, v) = table.pop_back();
    assert_eq!(k, "b");
    assert_eq!(v, 1);
    check_ordering(&table, vector[], option::none());
    table.destroy_empty();
}

#[test]
fun insert_after_then_pop() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.insert_after("a", "b", 1);
    let (k, v) = table.pop_back();
    assert_eq!(k, "b");
    assert_eq!(v, 1);
    check_ordering(&table, vector["a"], option::some(vector[0]));
    let (k, v) = table.pop_front();
    assert_eq!(k, "a");
    assert_eq!(v, 0);
    check_ordering(&table, vector[], option::none());
    table.destroy_empty();
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun insert_before_missing_anchor() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.insert_before("missing", "new", 1);
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun insert_after_missing_anchor() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.insert_after("missing", "new", 1);
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun insert_before_empty_table() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.insert_before("anchor", "new", 0);
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun insert_after_empty_table() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.insert_after("anchor", "new", 0);
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun insert_before_duplicate_key() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.push_back("b", 1);
    table.insert_before("b", "a", 99);
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun insert_after_duplicate_key() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.push_back("b", 1);
    table.insert_after("a", "b", 99);
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun insert_before_anchor_equals_new_key() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.insert_before("a", "a", 99);
    abort
}

#[test, expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun insert_after_anchor_equals_new_key() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<vector<u8>, u64>(ctx);
    table.push_back("a", 0);
    table.insert_after("a", "a", 99);
    abort
}

#[test]
fun test_all_orderings() {
    let ctx = &mut tx_context::dummy();
    let keys = vector<vector<u8>>["a", "b", "c"];
    let values = vector[3, 2, 1u64];
    let all_bools = vector[
        vector[true, true, true],
        vector[true, false, true],
        vector[true, true, false],
        vector[true, false, false],
        vector[false, false, true],
        vector[false, false, false],
    ];
    // all_bools indicate possible orderings of accessing the front vs the back of the
    // table
    // test all orderings of building up and tearing down the table, while mimicking
    // the ordering in a vector, and checking the keys have the same order in the table
    all_bools.length().do!(|i| {
        let pushes = &all_bools[i];
        all_bools.length().do!(|j| {
            let pops = &all_bools[j];
            build_up_and_tear_down(&keys, &values, pushes, pops, ctx);
        });
    });
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
    assert_eq!(values.length(), n);
    assert_eq!(pushes.length(), n);
    assert_eq!(pops.length(), n);

    let mut order = vector[];
    n.do!(|i| {
        let k = keys[i];
        let v = values[i];
        if (pushes[i]) {
            table.push_front(k, v);
            order.insert(k, 0);
        } else {
            table.push_back(k, v);
            order.push_back(k);
        };
    });

    check_ordering(&table, order, option::none());

    n.do!(|i| {
        let (table_k, order_k) = if (pops[i]) {
            let (table_k, _) = table.pop_front();
            (table_k, order.remove(0))
        } else {
            let (table_k, _) = table.pop_back();
            (table_k, order.pop_back())
        };
        assert_eq!(table_k, order_k);
        check_ordering(&table, order, option::none());
    });
    table.destroy_empty()
}

fun check_ordering<K: copy + drop + store, V: copy + store + drop>(
    table: &LinkedTable<K, V>,
    keys: vector<K>,
    values: Option<vector<V>>,
) {
    let n = table.length();
    assert_eq!(n, keys.length());
    if (n == 0) {
        assert!(table.front().is_none());
        assert!(table.back().is_none());
        return
    };

    let mut i = 0;
    while (i < n) {
        let cur = keys[i];
        values.do_ref!(|values| {
            assert_eq!(table[cur], values[i]);
        });

        if (i == 0) {
            assert_eq!(*table.front().borrow(), cur);
            assert!(table.prev(cur).is_none());
        } else {
            assert_eq!(*table.prev(cur).borrow(), keys[i - 1]);
        };
        if (i + 1 == n) {
            assert_eq!(*table.back().borrow(), cur);
            assert!(table.next(cur).is_none());
        } else {
            assert_eq!(*table.next(cur).borrow(), keys[i + 1]);
        };
        i = i + 1;
    }
}
