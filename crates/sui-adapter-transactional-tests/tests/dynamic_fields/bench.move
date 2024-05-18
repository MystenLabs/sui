// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// similar to dynamic_field_tests but over multiple transactions, as this uses a different code path
// test remove with the wrong value type

//# init --addresses a=0x0 --accounts A

//# publish
module a::m {

use sui::dynamic_field::{add, exists_with_type, borrow};

public struct Obj has key {
    id: object::UID,
}

entry fun t0(ctx: &mut TxContext) {
    let id = object::new(ctx);
    sui::transfer::transfer(Obj { id }, ctx.sender())
}

entry fun t1(obj: &mut Obj) {
    let id = &mut obj.id;

    let mut i = 0;
    while (i < 500) {
        add<u64, u64>(id, i, i);
        i = i + 1;
    }
}

entry fun t2(obj: &Obj) {
    let id = &obj.id;
    let mut i = 0;
    while (i < 500) {
        assert!(exists_with_type<u64, u64>(id, i), 0);
        i = i + 1;
    }
}

entry fun t3(obj: &Obj) {
    let id = &obj.id;
    let mut i = 0;
    while (i < 500) {
        assert!(!exists_with_type<u64, bool>(id, i), 0);
        i = i + 1;
    }
}

entry fun t4(obj: &Obj) {
    let id = &obj.id;
    let mut i = 0;
    while (i < 500) {
        assert!(*borrow(id, i) == i, 0);
        i = i + 1;
    }
}

}

//# run a::m::t0 --sender A

//# bench a::m::t1 --sender A --args object(2,0)

//# bench a::m::t2 --sender A --args object(2,0)

//# bench a::m::t3 --sender A --args object(2,0)

//# bench a::m::t4 --sender A --args object(2,0)
