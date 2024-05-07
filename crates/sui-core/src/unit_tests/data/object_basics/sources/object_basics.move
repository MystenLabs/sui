// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Test CTURD object basics (create, transfer, update, read, delete)
module examples::object_basics {
    use sui::clock::Clock;
    use sui::authenticator_state::AuthenticatorState;
    use sui::random::Random;
    use sui::dynamic_object_field as ofield;
    use sui::event;
    use sui::object::{Self, UID, ID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

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

    public entry fun share(ctx: &mut TxContext) {
        transfer::public_share_object(Object { id: object::new(ctx), value: 0 })
    }

    public entry fun transfer(o: Object, recipient: address) {
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
        transfer::transfer(wrap_object(o, ctx), tx_context::sender(ctx))
    }

    public entry fun unwrap(w: Wrapper, ctx: &mut TxContext) {
        let Wrapper { id, o } = w;
        object::delete(id);
        transfer::public_transfer(o, tx_context::sender(ctx))
    }

    public fun wrap_object(o: Object, ctx: &mut TxContext): Wrapper {
        Wrapper { id: object::new(ctx), o }
    }

    public entry fun add_ofield(o: &mut Object, v: Object) {
        ofield::add(&mut o.id, true, v);
    }

    public entry fun remove_ofield(o: &mut Object, ctx: &mut TxContext) {
        transfer::public_transfer(
            ofield::remove<bool, Object>(&mut o.id, true),
            tx_context::sender(ctx),
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

    public entry fun add_field(o: &mut Object, v: Object) {
        sui::dynamic_field::add(&mut o.id, true, v);
    }

    public entry fun remove_field(o: &mut Object, ctx: &mut TxContext) {
        transfer::public_transfer(
            sui::dynamic_field::remove<bool, Object>(&mut o.id, true),
            tx_context::sender(ctx),
        );
    }

    public struct Name has copy, drop, store {
        name_str: std::string::String
    }

    public entry fun add_field_with_struct_name(o: &mut Object, v: Object) {
        sui::dynamic_field::add(&mut o.id, Name {name_str: std::string::utf8(b"Test Name")}, v);
    }

    public entry fun add_ofield_with_struct_name(o: &mut Object, v: Object) {
        ofield::add(&mut o.id, Name {name_str: std::string::utf8(b"Test Name")}, v);
    }

    public entry fun add_field_with_bytearray_name(o: &mut Object, v: Object) {
        sui::dynamic_field::add(&mut o.id,b"Test Name", v);
    }

    public entry fun add_ofield_with_bytearray_name(o: &mut Object, v: Object) {
        ofield::add(&mut o.id,b"Test Name", v);
    }

    public entry fun add_field_with_address_name(o: &mut Object, v: Object,  ctx: &mut TxContext) {
        sui::dynamic_field::add(&mut o.id,tx_context::sender(ctx), v);
    }

    public entry fun add_ofield_with_address_name(o: &mut Object, v: Object,  ctx: &mut TxContext) {
        ofield::add(&mut o.id,tx_context::sender(ctx), v);
    }

    public entry fun generic_test<T>() {}

    public entry fun use_clock(_clock: &Clock) {}

    public entry fun use_auth_state(_auth_state: &AuthenticatorState) {}

    public entry fun use_random(_random: &Random) {}
}
