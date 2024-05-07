// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// similar to dynamic_object_field_tests but over multiple transactions,
// as this uses a different code path
// test remove with the wrong value type

//# init --addresses a=0x0 --accounts A

//# publish
module a::m {

use sui::dynamic_field::{add, exists_, borrow, borrow_mut};

public struct Wrapper has key {
    id: UID,
    old: UID,
}

public struct Obj has key, store {
    id: UID,
}

public struct Counter has key, store {
    id: UID,
    count: u64,
}

fun new(ctx: &mut TxContext): Counter {
    Counter { id: object::new(ctx), count: 0 }
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

entry fun t0(ctx: &mut TxContext) {
    let id = object::new(ctx);
    sui::transfer::transfer(Obj { id }, ctx.sender())
}

entry fun t1(obj: &mut Obj, ctx: &mut TxContext) {
    let id = &mut obj.id;
    add(id, 0, new(ctx));
}

entry fun t2(obj: &mut Obj) {
    let id = &mut obj.id;
    bump(borrow_mut(id, 0));
}

entry fun t3(obj: Obj, ctx: &mut TxContext) {
    let Obj { id } = obj;
    assert!(count(borrow(&id, 0)) == 1, 0);
    let wrapper = Wrapper { id: object::new(ctx), old: id };
    sui::transfer::transfer(wrapper, ctx.sender())
}

entry fun t4(wrapper: &mut Wrapper) {
    assert!(!exists_<u64>(&mut wrapper.id, 0), 0);
    assert!(count(borrow(&wrapper.old, 0)) == 1, 0);
}

entry fun t5(wrapper: Wrapper) {
    let Wrapper { id, old } = wrapper;
    object::delete(id);
    object::delete(old);
    // does not delete counter
}

}

//# run a::m::t0 --sender A

//# run a::m::t1 --sender A --args object(2,0)

//# run a::m::t2 --sender A --args object(2,0)

//# run a::m::t3 --sender A --args object(2,0)

//# view-object 3,0

//# run a::m::t4 --sender A --args object(5,0)

//# run a::m::t5 --sender A --args object(5,0)

//# view-object 3,0
