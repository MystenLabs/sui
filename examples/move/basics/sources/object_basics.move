// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Test CTURD object basics (create, transfer, update, read, delete)
module basics::object_basics;

use sui::event;
use sui::party;

public struct Object has key, store {
    id: UID,
    value: u64,
}

public struct Wrapper has key {
    id: UID,
    o: Object,
}

public struct NewValueEvent has copy, drop {
    new_value: u64,
}

public fun create(value: u64, recipient: address, ctx: &mut TxContext) {
    transfer::public_transfer(
        Object { id: object::new(ctx), value },
        recipient,
    )
}

public fun create_party(value: u64, recipient: address, ctx: &mut TxContext) {
    let party = party::single_owner(recipient);
    transfer::public_party_transfer(
        Object { id: object::new(ctx), value },
        party,
    )
}

public fun transfer(o: Object, recipient: address) {
    transfer::public_transfer(o, recipient)
}

public fun party_transfer_single_owner(o: Object, recipient: address) {
    let party = party::single_owner(recipient);
    transfer::public_party_transfer(o, party)
}

public fun freeze_object(o: Object) {
    transfer::public_freeze_object(o)
}

public fun set_value(o: &mut Object, value: u64) {
    o.value = value;
}

public fun get_value(o: &Object): u64 {
    o.value
}

// test that reading o2 and updating o1 works
public fun update(o1: &mut Object, o2: &Object) {
    o1.value = o2.value;
    // emit an event so the world can see the new value
    event::emit(NewValueEvent { new_value: o2.value })
}

public fun delete(o: Object) {
    let Object { id, value: _ } = o;
    id.delete();
}

public fun wrap(o: Object, ctx: &mut TxContext) {
    transfer::transfer(Wrapper { id: object::new(ctx), o }, ctx.sender());
}

#[lint_allow(self_transfer)]
public fun unwrap(w: Wrapper, ctx: &TxContext) {
    let Wrapper { id, o } = w;
    id.delete();
    transfer::public_transfer(o, ctx.sender());
}
