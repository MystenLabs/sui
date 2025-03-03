// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test tests that objects that are borrowed mutably or added/removed, but not modified, do not
// need to be marked as mutated.
// Operates through a long chain of dynamic fields
// After protocol version 74, some of the child object mutations will not appear since they do not
// modify the Move value.

//# init --addresses test=0x0 --accounts A --protocol-version 74

//# publish

module test::m1;

use sui::dynamic_field;

public struct GreatGrandParent has key, store {
    id: UID,
}

public struct GrandParent has key, store {
    id: UID,
}

public struct Parent has key, store {
    id: UID,
}

public struct Value has copy, drop, store {
    data: vector<u8>,
}

public struct ObjValue has key, store {
    id: UID,
    data: vector<u8>,
}

public fun create(ctx: &mut TxContext) {
    transfer::public_transfer(create_ggp(ctx), ctx.sender());
    transfer::public_share_object(create_ggp(ctx));
}

fun create_ggp(ctx: &mut TxContext): GreatGrandParent {
    let data = sui::address::to_bytes(ctx.sender());
    let mut gpp = GreatGrandParent { id: object::new(ctx) };
    let mut gp = GrandParent { id: object::new(ctx) };
    let mut p = Parent { id: object::new(ctx) };
    dynamic_field::add(&mut p.id, b"value", Value { data });
    dynamic_field::add(&mut p.id, b"obj", ObjValue { id: object::new(ctx), data });
    dynamic_field::add(&mut gp.id, b"p", p);
    dynamic_field::add(&mut gpp.id, b"gp", gp);
    gpp
}

// mutably borrow but do nothing
public fun borrow_mut(gpp: &mut GreatGrandParent) {
    let gp: &mut GrandParent = dynamic_field::borrow_mut(&mut gpp.id, b"gp");
    let p: &mut Parent = dynamic_field::borrow_mut(&mut gp.id, b"p");
    let _: &mut Value = dynamic_field::borrow_mut(&mut p.id, b"value");
    let _: &mut ObjValue = dynamic_field::borrow_mut(&mut p.id, b"obj");
}

// add remove the dynamic field but no change
public fun add_remove(gpp: &mut GreatGrandParent) {
    let mut gp: GrandParent = dynamic_field::remove(&mut gpp.id, b"gp");
    let mut p: Parent = dynamic_field::remove(&mut gp.id, b"p");
    let v: Value = dynamic_field::remove(&mut p.id, b"value");
    let o: ObjValue = dynamic_field::remove(&mut p.id, b"obj");
    dynamic_field::add(&mut p.id, b"obj", o);
    dynamic_field::add(&mut p.id, b"value", v);
    dynamic_field::add(&mut gp.id, b"p", p);
    dynamic_field::add(&mut gpp.id, b"gp", gp);
}

// write the same data back at the end
public fun write_back(gpp: &mut GreatGrandParent, ctx: &mut TxContext) {
    let gp: &mut GrandParent = dynamic_field::borrow_mut(&mut gpp.id, b"gp");
    let p: &mut Parent = dynamic_field::borrow_mut(&mut gp.id, b"p");

    let v: &mut Value = dynamic_field::borrow_mut(&mut p.id, b"value");
    v.data = vector[];
    v.data = sui::address::to_bytes(ctx.sender());

    let o: &mut ObjValue = dynamic_field::borrow_mut(&mut p.id, b"obj");
    o.data = vector[];
    o.data = sui::address::to_bytes(ctx.sender());
}

//# run test::m1::create --sender A

//# view-object 2,8

//# view-object 2,9

// for all of these, only the inputs are mutated

//# run test::m1::borrow_mut --sender A --args object(2,8)

//# run test::m1::borrow_mut --sender A --args object(2,9)

//# run test::m1::add_remove --sender A --args object(2,8)

//# run test::m1::add_remove --sender A --args object(2,9)

//# run test::m1::write_back --sender A --args object(2,8)

//# run test::m1::write_back --sender A --args object(2,9)
