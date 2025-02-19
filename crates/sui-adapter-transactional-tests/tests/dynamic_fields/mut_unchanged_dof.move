// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test tests that objects that are borrowed mutably or added/removed, but not modified, do not
// need to be marked as mutated. Uses dynamic object fields

//# init --addresses test=0x0 --accounts A

//# publish

module test::m1;

use sui::dynamic_object_field;

public struct Object has key, store {
    id: UID,
}

public struct ObjValue has key, store {
    id: UID,
    data: vector<u8>,
}

public fun create(ctx: &mut TxContext) {
    let data = sui::address::to_bytes(ctx.sender());
    let mut o1 = Object { id: object::new(ctx) };
    let mut o2 = Object { id: object::new(ctx) };
    dynamic_object_field::add(&mut o1.id, b"obj", ObjValue { id: object::new(ctx), data });
    dynamic_object_field::add(&mut o2.id, b"obj", ObjValue { id: object::new(ctx), data });
    transfer::public_transfer(o1, ctx.sender());
    transfer::public_share_object(o2);
}

// mutably borrow but do nothing
public fun borrow_mut(obj: &mut Object) {
    let _: &mut ObjValue = dynamic_object_field::borrow_mut(&mut obj.id, b"obj");
}

// add remove the dynamic field but no change
public fun add_remove(obj: &mut Object) {
    let o: ObjValue = dynamic_object_field::remove(&mut obj.id, b"obj");
    dynamic_object_field::add(&mut obj.id, b"obj", o);
}

// write the same data back at the end
public fun write_back(obj: &mut Object, ctx: &mut TxContext) {
    let o: &mut ObjValue = dynamic_object_field::borrow_mut(&mut obj.id, b"obj");
    o.data = vector[];
    o.data = sui::address::to_bytes(ctx.sender());
}

//# run test::m1::create --sender A

//# view-object 2,4

//# view-object 2,5

// for all of these, only the inputs are mutated

//# run test::m1::borrow_mut --sender A --args object(2,4)

//# run test::m1::borrow_mut --sender A --args object(2,5)

//# run test::m1::add_remove --sender A --args object(2,4)

//# run test::m1::add_remove --sender A --args object(2,5)

//# run test::m1::write_back --sender A --args object(2,4)

//# run test::m1::write_back --sender A --args object(2,5)
