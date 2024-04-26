// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that shared objects can have dynamic fields added
// dynamic fields can be added and removed in the same transaction

//# init --addresses a=0x0 --accounts A --shared-object-deletion true

//# publish
module a::m {
    use sui::dynamic_field::{add, remove};

    public struct Obj has key, store {
        id: object::UID,
    }

    public entry fun create(ctx: &mut TxContext) {
        transfer::public_share_object(Obj { id: object::new(ctx) })
    }

    public entry fun add_dynamic_field(mut obj: Obj) {
        add<u64, u64>(&mut obj.id, 0, 0);
        transfer::public_share_object(obj);
    }

    public entry fun add_and_remove_dynamic_field(mut obj: Obj) {
        add<u64, u64>(&mut obj.id, 0, 0);
        remove<u64, u64>(&mut obj.id, 0 );
        transfer::public_share_object(obj);
    }

}

//# run a::m::create --sender A

//# view-object 2,0

//# run a::m::add_dynamic_field --sender A --args object(2,0)

//# run a::m::create --sender A

//# view-object 5,0

//# run a::m::add_and_remove_dynamic_field --sender A --args object(5,0)
