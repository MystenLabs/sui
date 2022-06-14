// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Test CTURD object basics (create, transfer, update, read, delete)
module sui::ObjectBasics {
    use sui::Event;
    use sui::ID::{Self, VersionedID};
    use sui::tx_context::{Self, TxContext};
    use sui::Transfer;

    struct Object has key, store {
        id: VersionedID,
        value: u64,
    }

    struct Wrapper has key {
        id: VersionedID,
        o: Object
    }

    struct NewValueEvent has copy, drop {
        new_value: u64
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        Transfer::transfer(
            Object { id: tx_context::new_id(ctx), value },
            recipient
        )
    }

    public entry fun transfer(o: Object, recipient: address) {
        Transfer::transfer(o, recipient)
    }

    public entry fun freeze_object(o: Object) {
        Transfer::freeze_object(o)
    }

    public entry fun set_value(o: &mut Object, value: u64) {
        o.value = value;
    }

    // test that reading o2 and updating o1 works
    public entry fun update(o1: &mut Object, o2: &Object) {
        o1.value = o2.value;
        // emit an event so the world can see the new value
        Event::emit(NewValueEvent { new_value: o2.value })
    }

    public entry fun delete(o: Object) {
        let Object { id, value: _ } = o;
        ID::delete(id);
    }

    public entry fun wrap(o: Object, ctx: &mut TxContext) {
        Transfer::transfer(Wrapper { id: tx_context::new_id(ctx), o }, tx_context::sender(ctx))
    }

    public entry fun unwrap(w: Wrapper, ctx: &mut TxContext) {
        let Wrapper { id, o } = w;
        ID::delete(id);
        Transfer::transfer(o, tx_context::sender(ctx))
    }
}
