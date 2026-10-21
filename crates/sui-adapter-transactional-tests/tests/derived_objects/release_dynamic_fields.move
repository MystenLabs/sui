// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Dynamic fields on a released UID persist and are reachable again after it is re-claimed. The
// field's version is above the parent's version, so the re-claimed UID must keep a root version
// that bounds the field.

//# init --addresses ex=0x0 --accounts A

//# publish
module ex::m;

use sui::derived_object;
use sui::dynamic_field as df;

public struct Parent has key { id: UID }

public struct Obj has key, store { id: UID }

public fun create_parent(ctx: &mut TxContext) {
    transfer::transfer(Parent { id: object::new(ctx) }, ctx.sender())
}

public fun claim(parent: &mut Parent): Obj {
    Obj { id: derived_object::claim(&mut parent.id, 0u64) }
}

public fun release(parent: &mut Parent, obj: Obj) {
    let Obj { id } = obj;
    derived_object::release(&mut parent.id, id)
}

public fun add_field(obj: &mut Obj, value: u64) {
    df::add(&mut obj.id, 0u64, value)
}

public fun set_field(obj: &mut Obj, value: u64) {
    *df::borrow_mut(&mut obj.id, 0u64) = value
}

public fun assert_field(obj: &Obj, value: u64) {
    assert!(*df::borrow<u64, u64>(&obj.id, 0u64) == value)
}

//# run ex::m::create_parent --sender A

//# programmable --sender A --inputs object(2,0) 0 @A
//> 0: ex::m::claim(Input(0));
//> ex::m::add_field(Result(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

// Raise the field's version above the parent's.
//# run ex::m::set_field --sender A --args object(3,0) 1

//# run ex::m::set_field --sender A --args object(3,0) 2

//# view-object 2,0

//# view-object 3,0

// Release and re-claim in one transaction, then read and modify the field.
//# programmable --sender A --inputs object(2,0) object(3,0) 2 3 @A
//> ex::m::release(Input(0), Input(1));
//> 1: ex::m::claim(Input(0));
//> ex::m::assert_field(Result(1), Input(2));
//> ex::m::set_field(Result(1), Input(3));
//> TransferObjects([Result(1)], Input(4))

//# run ex::m::set_field --sender A --args object(3,0) 4

// Release, then re-claim in a later transaction and read the field.
//# programmable --sender A --inputs object(2,0) object(3,0)
//> ex::m::release(Input(0), Input(1))

//# programmable --sender A --inputs object(2,0) 4 5 @A
//> 0: ex::m::claim(Input(0));
//> ex::m::assert_field(Result(0), Input(1));
//> ex::m::set_field(Result(0), Input(2));
//> TransferObjects([Result(0)], Input(3))

//# run ex::m::assert_field --sender A --args object(3,0) 5
