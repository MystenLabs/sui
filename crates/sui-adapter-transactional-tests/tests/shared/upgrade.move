// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that shared objects must be newly created

//# init --addresses t=0x0 --accounts A

//# publish

module t::m {
    public struct Obj has key, store {
        id: UID,
    }

    public entry fun create(ctx: &mut TxContext) {
        let mut o = Obj { id: object::new(ctx) };
        sui::dynamic_field::add(&mut o.id, 0, Obj { id: object::new(ctx) });
        sui::dynamic_object_field::add(&mut o.id, 0, Obj { id: object::new(ctx) });
        transfer::public_transfer(o, ctx.sender())
    }

    public entry fun share(o: Obj) {
        transfer::public_share_object(o)
    }

    public entry fun share_wrapped(o: &mut Obj) {
        let inner: Obj = sui::dynamic_field::remove(&mut o.id, 0);
        transfer::public_share_object(inner)
    }

    public entry fun share_child(o: &mut Obj) {
        let inner: Obj = sui::dynamic_object_field::remove(&mut o.id, 0);
        transfer::public_share_object(inner)
    }

}

//# run t::m::create --sender A

//# view-object 2,2

//# run t::m::share --args object(2,2) --sender A

//# run t::m::share_wrapped --args object(2,2) --sender A

//# run t::m::share_child --args object(2,2) --sender A
