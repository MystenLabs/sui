// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::transfer::Receiving;

public struct Parent has key, store { id: UID }
public struct Child has key, store { id: UID }

public fun start(ctx: &mut TxContext) {
    let p = Parent { id: object::new(ctx) };
    let c = Child { id: object::new(ctx) };
    transfer::public_transfer(c, object::id_address(&p));
    transfer::public_transfer(p, ctx.sender());
}
public fun recv_mut(r: &mut Receiving<Child>): &mut Receiving<Child> { r }
public fun recv(r: &Receiving<Child>): &Receiving<Child> { r }
public fun receive(p: &mut Parent, r: Receiving<Child>): Child {
    transfer::public_receive(&mut p.id, r)
}
public fun r_then_v(_: &Receiving<Child>, _: Receiving<Child>) { abort 0 }
public fun v_then_r(_: Receiving<Child>, _: &Receiving<Child>) { abort 0 }
public fun use_mut<T>(_: &mut T) {}
public fun use_imm<T>(_: &T) {}
public fun delete(c: Child) { let Child { id } = c; object::delete(id) }

//# programmable --sender A
//> 0: test::m::start();

//# programmable --sender A --inputs object(2,0) receiving(2,1)
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1, received while borrowed
//> 0: test::m::recv_mut(Input(1));
//> 1: test::m::receive(Input(0), Input(1));
//> 2: test::m::use_mut<sui::transfer::Receiving<test::m::Child>>(Result(0));

//# programmable --sender A --inputs object(2,0) receiving(2,1)
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1, imm reference still live
//> 0: test::m::recv(Input(1));
//> 1: test::m::receive(Input(0), Input(1));
//> 2: test::m::use_imm<sui::transfer::Receiving<test::m::Child>>(Result(0));

//# programmable --sender A --inputs receiving(2,1)
// INVALID: CannotMoveBorrowedValue at arg 1 of command 0
//> 0: test::m::r_then_v(Input(0), Input(0));

//# programmable --sender A --inputs receiving(2,1)
// INVALID: ArgumentWithoutValue at arg 1 of command 0
//> 0: test::m::v_then_r(Input(0), Input(0));

//# programmable --sender A --inputs object(2,0) receiving(2,1)
// VALID: the receiving input is received once the reference into it is dead
//> 0: test::m::recv_mut(Input(1));
//> 1: test::m::use_mut<sui::transfer::Receiving<test::m::Child>>(Result(0));
//> 2: test::m::receive(Input(0), Input(1));
//> 3: test::m::delete(Result(2));
