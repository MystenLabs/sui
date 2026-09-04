// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 p=0x0 q=0x0 q_2=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_mut<T>(_: &mut T) {}

//# publish --upgradeable --sender A
module q::m {
    public fun x(): u64 { 0 }
}

//# stage-package
module p::m {
    public struct Marker has key { id: UID }
    fun init(ctx: &mut TxContext) {
        transfer::transfer(Marker { id: object::new(ctx) }, ctx.sender())
    }
}

//# stage-package
module q_2::m {
    public fun x(): u64 { 1 }
}

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(5,0) 7 @A
// VALID: Publish with an `init` between a reference's creation and its use
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_mut(Result(0));
//> 2: Publish(p, [sui, std]);
//> 3: test::m::write(Result(1), Input(1));
//> 4: TransferObjects([Result(2)], Input(2));

//# view-object 5,0

//# programmable --sender A --inputs object(2,1) 0u8 digest(q_2) object(5,0) 9
// VALID: Upgrade between a reference's creation and its use
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: test::m::inner_mut(Input(3));
//> 2: test::m::f_mut(Result(1));
//> 3: Upgrade(q_2, [sui, std], q, Result(0));
//> 4: test::m::write(Result(2), Input(4));
//> 5: sui::package::commit_upgrade(Input(0), Result(3));

//# view-object 5,0

//# programmable --sender A --inputs object(2,1) 0u8 digest(q_2)
// INVALID: InvalidReferenceArgument at arg 0 of command 1, the cap `&mut` while a reference into it is live
//> 0: test::m::id_mut<sui::package::UpgradeCap>(Input(0));
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: test::m::use_mut<sui::package::UpgradeCap>(Result(0));
//> 3: Upgrade(q_2, [sui, std], q, Result(1));
//> 4: sui::package::commit_upgrade(Input(0), Result(3));

//# programmable --sender A --inputs object(2,1)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, the cap by value while a reference into it is live
//> 0: test::m::id_mut<sui::package::UpgradeCap>(Input(0));
//> 1: sui::package::make_immutable(Input(0));
//> 2: test::m::use_mut<sui::package::UpgradeCap>(Result(0));
