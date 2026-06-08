// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::linked_table_tests;

use sui::linked_table::{Self, LinkedTable};
use sui::test_scenario;

#[test]
fun simple_all_functions() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    check_ordering(&table, &vector[]);
    // add fields
    table.push_back(b"hello", 0u64);
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
    table.push_front(b"hello", 0u64);
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
    table.push_back(b"hello", 0u64);
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
    table.push_front(b"hello", 0u64);
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun push_back_duplicate() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"hello", 0u64);
    table.push_back(b"hello", 0);
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun push_mixed_duplicate() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"hello", 0u64);
    table.push_front(b"hello", 0);
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun borrow_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let table = linked_table::new<u64, u64>(scenario.ctx());
    &table[0];
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun borrow_mut_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new<u64, u64>(scenario.ctx());
    &mut table[0];
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun remove_missing() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new<u64, u64>(scenario.ctx());
    table.remove(0);
    abort
}

#[test]
#[expected_failure(abort_code = linked_table::ETableIsEmpty)]
fun pop_front_empty() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new<u64, u64>(scenario.ctx());
    table.pop_front();
    abort
}

#[test]
#[expected_failure(abort_code = linked_table::ETableIsEmpty)]
fun pop_back_empty() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new<u64, u64>(scenario.ctx());
    table.pop_back();
    abort
}

#[test]
#[expected_failure(abort_code = linked_table::ETableNotEmpty)]
fun destroy_non_empty() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(0u64, 0u64);
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
    table.push_back(0u64, 0u64);
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
    table.push_back(0u64, 0u64);
    assert!(!table.is_empty());
    assert!(table.length() == 1);
    table.push_back(1, 0);
    assert!(!table.is_empty());
    assert!(table.length() == 2);
    scenario.end();
    table.drop();
}

#[test]
fun insert_before_singleton() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"anchor", 1u64);
    table.insert_before(b"anchor", b"new", 0);
    check_ordering(&table, &vector[b"new", b"anchor"]);
    assert!(table[b"new"] == 0);
    assert!(table[b"anchor"] == 1);
    assert!(table.length() == 2);
    assert!(table.front().borrow() == &b"new");
    assert!(table.back().borrow() == &b"anchor");
    scenario.end();
    table.drop();
}

#[test]
fun insert_before_head() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.push_back(b"b", 1);
    table.push_back(b"c", 2);
    table.insert_before(b"a", b"z", 99);
    check_ordering(&table, &vector[b"z", b"a", b"b", b"c"]);
    assert!(table.front().borrow() == &b"z");
    assert!(table.back().borrow() == &b"c");
    assert!(table.length() == 4);
    scenario.end();
    table.drop();
}

#[test]
fun insert_before_middle() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.push_back(b"b", 1);
    table.push_back(b"c", 2);
    table.insert_before(b"b", b"m", 50);
    check_ordering(&table, &vector[b"a", b"m", b"b", b"c"]);
    assert!(table.front().borrow() == &b"a");
    assert!(table.back().borrow() == &b"c");
    scenario.end();
    table.drop();
}

#[test]
fun insert_before_tail() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.push_back(b"b", 1);
    table.push_back(b"c", 2);
    table.insert_before(b"c", b"x", 75);
    check_ordering(&table, &vector[b"a", b"b", b"x", b"c"]);
    assert!(table.front().borrow() == &b"a");
    assert!(table.back().borrow() == &b"c");
    scenario.end();
    table.drop();
}

#[test]
fun insert_after_singleton() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"anchor", 1u64);
    table.insert_after(b"anchor", b"new", 0);
    check_ordering(&table, &vector[b"anchor", b"new"]);
    assert!(table[b"new"] == 0);
    assert!(table[b"anchor"] == 1);
    assert!(table.length() == 2);
    assert!(table.front().borrow() == &b"anchor");
    assert!(table.back().borrow() == &b"new");
    scenario.end();
    table.drop();
}

#[test]
fun insert_after_head() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.push_back(b"b", 1);
    table.push_back(b"c", 2);
    table.insert_after(b"a", b"x", 25);
    check_ordering(&table, &vector[b"a", b"x", b"b", b"c"]);
    assert!(table.front().borrow() == &b"a");
    assert!(table.back().borrow() == &b"c");
    scenario.end();
    table.drop();
}

#[test]
fun insert_after_middle() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.push_back(b"b", 1);
    table.push_back(b"c", 2);
    table.insert_after(b"b", b"m", 50);
    check_ordering(&table, &vector[b"a", b"b", b"m", b"c"]);
    assert!(table.front().borrow() == &b"a");
    assert!(table.back().borrow() == &b"c");
    scenario.end();
    table.drop();
}

#[test]
fun insert_after_tail() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.push_back(b"b", 1);
    table.push_back(b"c", 2);
    table.insert_after(b"c", b"z", 99);
    check_ordering(&table, &vector[b"a", b"b", b"c", b"z"]);
    assert!(table.front().borrow() == &b"a");
    assert!(table.back().borrow() == &b"z");
    scenario.end();
    table.drop();
}

#[test]
fun push_front_after_insert_before_head() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.push_back(b"b", 1);
    table.insert_before(b"a", b"z", 99);
    check_ordering(&table, &vector[b"z", b"a", b"b"]);
    table.push_front(b"y", 100);
    check_ordering(&table, &vector[b"y", b"z", b"a", b"b"]);
    table.push_back(b"c", 2);
    check_ordering(&table, &vector[b"y", b"z", b"a", b"b", b"c"]);
    scenario.end();
    table.drop();
}

#[test]
fun push_back_after_insert_after_tail() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.push_back(b"b", 1);
    table.insert_after(b"b", b"z", 99);
    check_ordering(&table, &vector[b"a", b"b", b"z"]);
    table.push_back(b"y", 100);
    check_ordering(&table, &vector[b"a", b"b", b"z", b"y"]);
    table.push_front(b"x", 101);
    check_ordering(&table, &vector[b"x", b"a", b"b", b"z", b"y"]);
    scenario.end();
    table.drop();
}

#[test]
fun push_after_insert_at_singleton_boundary() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.insert_before(b"a", b"b", 1);
    check_ordering(&table, &vector[b"b", b"a"]);
    table.push_front(b"c", 2);
    check_ordering(&table, &vector[b"c", b"b", b"a"]);
    table.push_back(b"d", 3);
    check_ordering(&table, &vector[b"c", b"b", b"a", b"d"]);

    let mut table2 = linked_table::new(scenario.ctx());
    table2.push_back(b"a", 0u64);
    table2.insert_after(b"a", b"b", 1);
    check_ordering(&table2, &vector[b"a", b"b"]);
    table2.push_back(b"c", 2);
    check_ordering(&table2, &vector[b"a", b"b", b"c"]);
    table2.push_front(b"d", 3);
    check_ordering(&table2, &vector[b"d", b"a", b"b", b"c"]);
    scenario.end();
    table.drop();
    table2.drop();
}

#[test]
fun insert_before_repeated_at_head() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"anchor", 0u64);
    table.insert_before(b"anchor", b"a", 1);
    table.insert_before(b"a", b"b", 2);
    table.insert_before(b"b", b"c", 3);
    check_ordering(&table, &vector[b"c", b"b", b"a", b"anchor"]);
    assert!(table.length() == 4);
    scenario.end();
    table.drop();
}

#[test]
fun insert_after_repeated_at_tail() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"anchor", 0u64);
    table.insert_after(b"anchor", b"a", 1);
    table.insert_after(b"a", b"b", 2);
    table.insert_after(b"b", b"c", 3);
    check_ordering(&table, &vector[b"anchor", b"a", b"b", b"c"]);
    assert!(table.length() == 4);
    scenario.end();
    table.drop();
}

#[test]
fun insert_before_and_after_interleaved() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"m", 0u64);
    table.insert_before(b"m", b"l", 1);
    check_ordering(&table, &vector[b"l", b"m"]);
    table.insert_after(b"m", b"n", 2);
    check_ordering(&table, &vector[b"l", b"m", b"n"]);
    table.insert_before(b"l", b"k", 3);
    check_ordering(&table, &vector[b"k", b"l", b"m", b"n"]);
    table.insert_after(b"n", b"o", 4);
    check_ordering(&table, &vector[b"k", b"l", b"m", b"n", b"o"]);
    table.insert_after(b"l", b"l2", 5);
    check_ordering(&table, &vector[b"k", b"l", b"l2", b"m", b"n", b"o"]);
    table.insert_before(b"n", b"m2", 6);
    check_ordering(&table, &vector[b"k", b"l", b"l2", b"m", b"m2", b"n", b"o"]);
    assert!(table.length() == 7);
    assert!(table.front().borrow() == &b"k");
    assert!(table.back().borrow() == &b"o");
    scenario.end();
    table.drop();
}

#[test]
fun insert_then_remove_preserves_ordering() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.push_back(b"c", 2);
    table.insert_after(b"a", b"b", 1);
    check_ordering(&table, &vector[b"a", b"b", b"c"]);
    assert!(table.remove(b"b") == 1);
    check_ordering(&table, &vector[b"a", b"c"]);
    table.insert_before(b"c", b"b", 1);
    check_ordering(&table, &vector[b"a", b"b", b"c"]);
    assert!(table.remove(b"a") == 0);
    check_ordering(&table, &vector[b"b", b"c"]);
    assert!(table.remove(b"c") == 2);
    check_ordering(&table, &vector[b"b"]);
    assert!(table.remove(b"b") == 1);
    check_ordering(&table, &vector[]);
    scenario.end();
    table.destroy_empty();
}

#[test]
fun insert_before_then_pop() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"b", 1u64);
    table.insert_before(b"b", b"a", 0);
    let (k, v) = table.pop_front();
    assert!(k == b"a");
    assert!(v == 0);
    check_ordering(&table, &vector[b"b"]);
    let (k, v) = table.pop_back();
    assert!(k == b"b");
    assert!(v == 1);
    check_ordering(&table, &vector[]);
    scenario.end();
    table.destroy_empty();
}

#[test]
fun insert_after_then_pop() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.insert_after(b"a", b"b", 1);
    let (k, v) = table.pop_back();
    assert!(k == b"b");
    assert!(v == 1);
    check_ordering(&table, &vector[b"a"]);
    let (k, v) = table.pop_front();
    assert!(k == b"a");
    assert!(v == 0);
    check_ordering(&table, &vector[]);
    scenario.end();
    table.destroy_empty();
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun insert_before_missing_anchor() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new<vector<u8>, u64>(scenario.ctx());
    table.push_back(b"a", 0);
    table.insert_before(b"missing", b"new", 1);
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun insert_after_missing_anchor() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new<vector<u8>, u64>(scenario.ctx());
    table.push_back(b"a", 0);
    table.insert_after(b"missing", b"new", 1);
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun insert_before_empty_table() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new<vector<u8>, u64>(scenario.ctx());
    table.insert_before(b"anchor", b"new", 0);
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
fun insert_after_empty_table() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new<vector<u8>, u64>(scenario.ctx());
    table.insert_after(b"anchor", b"new", 0);
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun insert_before_duplicate_key() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.push_back(b"b", 1);
    table.insert_before(b"b", b"a", 99);
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun insert_after_duplicate_key() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.push_back(b"b", 1);
    table.insert_after(b"a", b"b", 99);
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun insert_before_anchor_equals_new_key() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.insert_before(b"a", b"a", 99);
    abort
}

#[test]
#[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
fun insert_after_anchor_equals_new_key() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let mut table = linked_table::new(scenario.ctx());
    table.push_back(b"a", 0u64);
    table.insert_after(b"a", b"a", 99);
    abort
}

#[test]
fun test_all_orderings() {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);
    let ctx = scenario.ctx();
    let keys = vector[b"a", b"b", b"c"];
    let values = vector[3, 2, 1u64];
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
