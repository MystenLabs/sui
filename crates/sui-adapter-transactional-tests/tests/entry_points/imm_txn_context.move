// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --accounts A

//# publish
module Test::M {
    public struct Obj has key {
        id: sui::object::UID,
        value: u64
    }

    public entry fun mint(ctx: &mut TxContext) {
        sui::transfer::transfer(
            Obj { id: sui::object::new(ctx), value: 0 },
            tx_context::sender(ctx),
        )
    }

    public entry fun set_to_epoch(obj: &mut Obj, ctx: &TxContext) {
        obj.value = tx_context::epoch(ctx)
    }

    public entry fun check_is_epoch(obj: &Obj, ctx: &TxContext) {
        assert!(obj.value == tx_context::epoch(ctx), 0)
    }
}

//# run Test::M::mint --sender A

//# run Test::M::set_to_epoch --sender A --args object(2,0)

//# run Test::M::check_is_epoch --sender A --args object(2,0)
