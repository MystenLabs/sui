// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that shared objects cannot be wrapped

//# init --addresses t1=0x0 t2=0x0 --shared-object-deletion true

//# publish

module t2::o2 {
    public struct Obj2 has key, store {
        id: UID,
    }

    public struct Wrapper has key {
        id: UID,
        o2: Obj2
    }

    public entry fun create(ctx: &mut TxContext) {
        transfer::public_share_object(Obj2 { id: object::new(ctx) })
    }

    public entry fun wrap_o2(o2: Obj2, ctx: &mut TxContext) {
        transfer::transfer(Wrapper { id: object::new(ctx), o2}, tx_context::sender(ctx))
    }
}


//# run t2::o2::create

//# view-object 2,0

//# run t2::o2::wrap_o2 --args object(2,0)
