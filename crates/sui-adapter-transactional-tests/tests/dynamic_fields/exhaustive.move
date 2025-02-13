// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// similar to dynamic_field_tests but over multiple transactions, as this uses a different code path
// test remove with the wrong value type

//# init --addresses a=0x0 --accounts A

//# publish
module a::m {

use sui::dynamic_field::{add, exists_with_type, borrow, borrow_mut, remove};

public struct Obj has key {
    id: object::UID,
}

entry fun t0(ctx: &mut TxContext) {
    let id = object::new(ctx);
    sui::transfer::transfer(Obj { id }, ctx.sender())
}

entry fun t1(obj: &mut Obj) {
    let id = &mut obj.id;
    add<u64, u64>(id, 0, 0);
    add<vector<u8>, u64>(id, b"", 1);
    add<bool, u64>(id, false, 2);
}

entry fun t2(obj: &Obj) {
    let id = &obj.id;
    assert!(exists_with_type<u64, u64>(id, 0), 0);
    assert!(exists_with_type<vector<u8>, u64>(id, b""), 0);
    assert!(exists_with_type<bool, u64>(id, false), 0);
}

entry fun t3(obj: &Obj) {
    let id = &obj.id;
    assert!(*borrow(id, 0) == 0, 0);
    assert!(*borrow(id, b"") == 1, 0);
    assert!(*borrow(id, false) == 2, 0);
}

entry fun t4(obj: &mut Obj) {
    let id = &mut obj.id;
    *borrow_mut(id, 0) = 3 + *borrow(id, 0);
    *borrow_mut(id, b"") = 4 + *borrow(id, b"");
    *borrow_mut(id, false) = 5 + *borrow(id, false);
}

entry fun t5(obj: &mut Obj) {
    let id = &mut obj.id;
    assert!(*borrow(id, 0) == 3, 0);
    assert!(*borrow(id, b"") == 5, 0);
    assert!(*borrow(id, false) == 7, 0);
}

entry fun t6(obj: &mut Obj) {
    let id = &mut obj.id;
    assert!(remove(id, 0) == 3, 0);
    assert!(remove(id, b"") == 5, 0);
    // do not remove at least one
}

entry fun t7(obj: &Obj) {
    let id = &obj.id;
    assert!(!exists_with_type<u64, u64>(id, 0), 0);
    assert!(!exists_with_type<vector<u8>, u64>(id, b""), 0);
    assert!(exists_with_type<bool, u64>(id, false), 0);
}

entry fun t8(obj: Obj) {
    let Obj { id } = obj;
    object::delete(id);
}

}

//# run a::m::t0 --sender A

//# run a::m::t1 --sender A --args object(2,0)

//# run a::m::t2 --sender A --args object(2,0)

//# run a::m::t3 --sender A --args object(2,0)

//# run a::m::t4 --sender A --args object(2,0)

//# run a::m::t5 --sender A --args object(2,0)

//# run a::m::t6 --sender A --args object(2,0)

//# run a::m::t7 --sender A --args object(2,0)

//# run a::m::t8 --sender A --args object(2,0)
