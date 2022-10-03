// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::dynamic_object_field_tests {

use std::option;
use sui::dynamic_object_field::{add, exists_, borrow, borrow_mut, remove, id as field_id};
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
    let id = ts::new_object(&mut scenario);
    let uid1 = ts::new_object(&mut scenario);
    let id1 = object::uid_to_inner(&uid1);
    let uid2 = ts::new_object(&mut scenario);
    let id2 = object::uid_to_inner(&uid2);
    let uid3 = ts::new_object(&mut scenario);
    let id3 = object::uid_to_inner(&uid3);
    // add fields
    add(&mut id, 0, new(uid1));
    add(&mut id, b"", new(uid2));
    add(&mut id, false, new(uid3));
    // check they exist
    assert!(exists_(&id, 0), 0);
    assert!(exists_(&id, b""), 0);
    assert!(exists_(&id, false), 0);
    // check the IDs
    assert!(option::borrow(&field_id(&id, 0)) == &id1, 0);
    assert!(option::borrow(&field_id(&id, b"")) == &id2, 0);
    assert!(option::borrow(&field_id(&id, false)) == &id3, 0);
    // check the values
    assert!(count(borrow(&id, 0)) == 0, 0);
    assert!(count(borrow(&id, b"")) == 0, 0);
    assert!(count(borrow(&id, false)) == 0, 0);
    // mutate them
    bump(borrow_mut(&mut id, 0));
    bump(bump(borrow_mut(&mut id, b"")));
    bump(bump(bump(borrow_mut(&mut id, false))));
    // check the new value
    assert!(count(borrow(&id, 0)) == 1, 0);
    assert!(count(borrow(&id, b"")) == 2, 0);
    assert!(count(borrow(&id, false)) == 3, 0);
    // remove the value and check it
    assert!(destroy(remove(&mut id, 0)) == 1, 0);
    assert!(destroy(remove(&mut id, b"")) == 2, 0);
    assert!(destroy(remove(&mut id, false)) == 3, 0);
    // verify that they are not there
    assert!(!exists_(&id, 0), 0);
    assert!(!exists_(&id, b""), 0);
    assert!(!exists_(&id, false), 0);
    ts::end(scenario);
    object::delete(id);
}

fun new(id: UID): Counter {
    Counter { id, count: 0 }
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
