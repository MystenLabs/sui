// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// similar to dynamic_field_tests but over multiple transactions, as this uses a different code path
// test duplicate add

//# init --addresses a=0x0 --accounts A

//# publish
module a::m {

use sui::dynamic_field::add;

public struct Obj has key {
    id: object::UID,
}

entry fun t1(ctx: &mut TxContext) {
    let mut id = object::new(ctx);
    add<u64, u64>(&mut id, 0, 0);
    sui::transfer::transfer(Obj { id }, ctx.sender())
}

entry fun t2(obj: &mut Obj) {
    add<u64, u64>(&mut obj.id, 0, 1);
}

}

//# run a::m::t1 --sender A

//# run a::m::t2 --sender A --args object(2,0)
