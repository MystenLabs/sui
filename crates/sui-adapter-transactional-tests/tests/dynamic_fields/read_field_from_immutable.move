// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// similar to dynamic_field_tests but over multiple transactions, as this uses a different code path
// test duplicate add

//# init --addresses a=0x0 --accounts A

//# publish
module a::m {

use sui::dynamic_field::{add, borrow};

public struct Obj has key {
    id: object::UID,
}

entry fun add_then_freeze(ctx: &mut TxContext) {
    let mut id = object::new(ctx);
    add<u64, u64>(&mut id, 0, 0);
    sui::transfer::freeze_object(Obj { id })
}

entry fun read_from_frozen(obj: &Obj) {
    let _ = borrow<u64, u64>(&obj.id, 0);
}

}

//# run a::m::add_then_freeze --sender A

//# run a::m::read_from_frozen --sender A --args object(2,0)
