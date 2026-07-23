// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Test CTURD object basics (create, transfer, update, read, delete)
module examples::object_basics;

use sui::authenticator_state::AuthenticatorState;
use sui::clock::Clock;
use sui::dynamic_object_field as ofield;
use sui::event;
use sui::random::Random;

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

public fun share(ctx: &mut TxContext) {
    transfer::public_share_object(Object { id: object::new(ctx), value: 0 })
}

public fun transfer(o: Object, recipient: address) {
    transfer::public_transfer(o, recipient)
}

public fun freeze_object(o: Object) {
    transfer::public_freeze_object(o)
}

public fun set_value(o: &mut Object, value: u64) {
    o.value = value;
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
    transfer::transfer(wrap_object(o, ctx), ctx.sender())
}

public fun unwrap(w: Wrapper, ctx: &mut TxContext) {
    let Wrapper { id, o } = w;
    id.delete();
    transfer::public_transfer(o, ctx.sender())
}

public fun wrap_object(o: Object, ctx: &mut TxContext): Wrapper {
    Wrapper { id: object::new(ctx), o }
}

public fun add_ofield(o: &mut Object, v: Object) {
    ofield::add(&mut o.id, true, v);
}

public fun remove_ofield(o: &mut Object, ctx: &mut TxContext) {
    transfer::public_transfer(
        ofield::remove<bool, Object>(&mut o.id, true),
        ctx.sender(),
    );
}

fun borrow_value_mut(o: &mut Object): &mut u64 {
    &mut o.value
}

fun borrow_value(o: &Object): &u64 {
    &o.value
}

fun get_value(o: &Object): u64 {
    o.value
}

fun get_contents(o: &Object): (ID, u64) {
    (object::id(o), o.value)
}

public fun add_field(o: &mut Object, v: Object) {
    sui::dynamic_field::add(&mut o.id, true, v);
}

public fun remove_field(o: &mut Object, ctx: &mut TxContext) {
    transfer::public_transfer(
        sui::dynamic_field::remove<bool, Object>(&mut o.id, true),
        ctx.sender(),
    );
}

public struct Name has copy, drop, store {
    name_str: std::string::String,
}

public fun add_field_with_struct_name(o: &mut Object, v: Object) {
    sui::dynamic_field::add(&mut o.id, Name { name_str: std::string::utf8(b"Test Name") }, v);
}

public fun add_ofield_with_struct_name(o: &mut Object, v: Object) {
    ofield::add(&mut o.id, Name { name_str: std::string::utf8(b"Test Name") }, v);
}

public fun add_field_with_bytearray_name(o: &mut Object, v: Object) {
    sui::dynamic_field::add(&mut o.id, b"Test Name", v);
}

public fun add_ofield_with_bytearray_name(o: &mut Object, v: Object) {
    ofield::add(&mut o.id, b"Test Name", v);
}

public fun add_field_with_address_name(o: &mut Object, v: Object, ctx: &mut TxContext) {
    sui::dynamic_field::add(&mut o.id, ctx.sender(), v);
}

public fun add_ofield_with_address_name(o: &mut Object, v: Object, ctx: &mut TxContext) {
    ofield::add(&mut o.id, ctx.sender(), v);
}

public fun generic_test<T>() {}

public fun use_clock(_clock: &Clock) {}

public fun use_auth_state(_auth_state: &AuthenticatorState) {}

public fun use_random(_random: &Random) {}
