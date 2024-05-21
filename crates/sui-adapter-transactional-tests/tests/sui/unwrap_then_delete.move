// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Exercise test functions that wrap an object and subsequently unwrap and delete it.
// There are two cases:
// 1. In the first case, we first create the object, wrap it in a separate transaction (so that there will be a wrapped
//    tombstone), and then unwrap and delete it in a third transaction. In this case we expect the object to show up
//    in the unwrapped_then_deleted in the effects.
// 2. In the second case, we create and wrap the object in a single transaction, and then unwrap and delete it in a
//    second transaction. In this case we expect the object to not show up in the unwrapped_then_deleted in the effects.

//# init --addresses test=0x0 --accounts A

//# publish

module test::object_basics {
    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public struct Wrapper has key {
        id: UID,
        o: Object
    }

    public entry fun create(value: u64, ctx: &mut TxContext) {
        transfer::transfer(
            Object { id: object::new(ctx), value },
            tx_context::sender(ctx)
        )
    }

    public entry fun wrap(o: Object, ctx: &mut TxContext) {
        transfer::transfer(
            Wrapper { id: object::new(ctx), o },
            tx_context::sender(ctx)
        )
    }

    public entry fun create_and_wrap(value: u64, ctx: &mut TxContext) {
        let o = Object { id: object::new(ctx), value };
        transfer::transfer(
            Wrapper { id: object::new(ctx), o },
            tx_context::sender(ctx)
        )
    }

    public entry fun unwrap(w: Wrapper, ctx: &mut TxContext) {
        let Wrapper { id, o } = w;
        object::delete(id);
        transfer::transfer(o, tx_context::sender(ctx))
    }

    public entry fun unwrap_and_delete(w: Wrapper) {
        let Wrapper { id, o } = w;
        object::delete(id);
        let Object { id, value: _ } = o;
        object::delete(id);
    }
}

//# run test::object_basics::create --args 10

//# run test::object_basics::wrap --args object(2,0)

//# run test::object_basics::unwrap_and_delete --args object(3,0)

//# run test::object_basics::create_and_wrap --args 10

//# run test::object_basics::unwrap_and_delete --args object(5,0)
