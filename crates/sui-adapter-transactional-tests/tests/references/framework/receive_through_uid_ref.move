// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::transfer::Receiving;

public struct Parent has key, store { id: UID, inner: Inner, child: Option<Child> }
public struct Inner has store, copy, drop { f: u64 }
public struct Child has key, store { id: UID, v: u64 }

public fun start(ctx: &mut TxContext) {
    let p = Parent { id: object::new(ctx), inner: Inner { f: 0 }, child: option::none() };
    let c = Child { id: object::new(ctx), v: 0 };
    transfer::public_transfer(c, object::id_address(&p));
    transfer::public_transfer(p, ctx.sender());
}
public fun new_child(p: &Parent, ctx: &mut TxContext) {
    transfer::public_transfer(Child { id: object::new(ctx), v: 0 }, object::id_address(p))
}
public fun uid_mut(p: &mut Parent): &mut UID { &mut p.id }
public fun uid_and_inner_mut(p: &mut Parent): (&mut UID, &mut Inner) { (&mut p.id, &mut p.inner) }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun receive_and_wrap(p: &mut Parent, r: Receiving<Child>): &mut Child {
    let c = transfer::public_receive(&mut p.id, r);
    p.child.fill(c);
    p.child.borrow_mut()
}
public fun v_mut(c: &mut Child): &mut u64 { &mut c.v }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_mut<T>(_: &mut T) {}
public fun delete_child(c: Child) { let Child { id, v: _ } = c; object::delete(id) }

//# programmable --sender A
//> 0: test::m::start();

//# programmable --sender A --inputs object(2,0) receiving(2,1) 5
// VALID: receive through the `&mut UID` sibling while the `&mut Inner` sibling is live
//> 0: test::m::uid_and_inner_mut(Input(0));
//> 1: sui::transfer::public_receive<test::m::Child>(NestedResult(0,0), Input(1));
//> 2: test::m::f_mut(NestedResult(0,1));
//> 3: test::m::write(Result(2), Input(2));
//> 4: test::m::delete_child(Result(1));

//# view-object 2,0

//# programmable --sender A --inputs object(2,0)
//> 0: test::m::new_child(Input(0));

//# programmable --sender A --inputs object(2,0) receiving(5,0) @A
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, parent transferred while its `&mut UID` is live
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::transfer::public_receive<test::m::Child>(Result(0), Input(1));
//> 2: TransferObjects([Input(0)], Input(2));
//> 3: test::m::use_mut<sui::object::UID>(Result(0));
//> 4: test::m::delete_child(Result(1));

//# programmable --sender A --inputs object(2,0) receiving(5,0) 9 @A
// VALID: the received child is wrapped into the parent, written through, and the parent transferred after
//> 0: test::m::receive_and_wrap(Input(0), Input(1));
//> 1: test::m::v_mut(Result(0));
//> 2: test::m::write(Result(1), Input(2));
//> 3: TransferObjects([Input(0)], Input(3));

//# view-object 2,0
