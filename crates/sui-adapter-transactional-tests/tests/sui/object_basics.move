// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Exercise test functions that create, transfer, read, update, and delete objects

//# init --addresses test=0x0 --accounts A B

//# publish

module test::object_basics {
    use sui::event;

    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public struct Wrapper has key {
        id: UID,
        o: Object
    }

    public struct NewValueEvent has copy, drop {
        new_value: u64
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }

    public entry fun transfer_(o: Object, recipient: address) {
        transfer::public_transfer(o, recipient)
    }

    public entry fun freeze_object(o: Object) {
        transfer::public_freeze_object(o)
    }

    public entry fun set_value(o: &mut Object, value: u64) {
        o.value = value;
    }

    // test that reading o2 and updating o1 works
    public entry fun update(o1: &mut Object, o2: &Object) {
        o1.value = o2.value;
        // emit an event so the world can see the new value
        event::emit(NewValueEvent { new_value: o2.value })
    }

    public entry fun delete(o: Object) {
        let Object { id, value: _ } = o;
        object::delete(id);
    }

    public entry fun wrap(o: Object, ctx: &mut TxContext) {
        transfer::transfer(Wrapper { id: object::new(ctx), o }, tx_context::sender(ctx))
    }

    public entry fun unwrap(w: Wrapper, ctx: &mut TxContext) {
        let Wrapper { id, o } = w;
        object::delete(id);
        transfer::public_transfer(o, tx_context::sender(ctx))
    }
}

//# run test::object_basics::create --sender A --args 10 @A

//# view-object 2,0

//# run test::object_basics::transfer_ --sender A --args object(2,0) @B

//# view-object 2,0

//# run test::object_basics::create --sender B --args 20 @B

//# run test::object_basics::update --sender B --args object(2,0) object(6,0)

//# run test::object_basics::delete --sender B --args object(2,0)
