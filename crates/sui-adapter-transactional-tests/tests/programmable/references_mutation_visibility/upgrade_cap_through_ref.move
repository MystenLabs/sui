// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 q=0x0 q_2=0x0 p=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, f: u64 }

public fun new(ctx: &mut TxContext): Obj { Obj { id: object::new(ctx), f: 0 } }
public fun f_mut(o: &mut Obj): &mut u64 { &mut o.f }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }

//# publish --upgradeable --sender A
module q::m {
    public fun x(): u64 { 0 }
}

//# stage-package
module q_2::m {
    public fun x(): u64 { 1 }
}

//# stage-package
module p::m {
    public struct Marker has key { id: UID }
    fun init(ctx: &mut TxContext) {
        transfer::transfer(Marker { id: object::new(ctx) }, ctx.sender())
    }
}

//# programmable --sender A --inputs 7 8 @A
// VALID: references into an object result and a pure input live across a Publish whose init creates an object
//> 0: test::m::new();
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::id_mut<u64>(Input(0));
//> 3: Publish(p, [sui, std]);
//> 4: test::m::write(Result(1), Input(1));
//> 5: test::m::write(Result(2), Input(1));
//> 6: test::m::check(Result(1), Input(1));
//> 7: test::m::check(Input(0), Input(1));
//> 8: TransferObjects([Result(0), Result(3)], Input(2));

//# programmable --sender A --inputs object(2,1) 0u8 digest(q_2) 7 @A
// VALID: a reference into the UpgradeCap authorizes and commits; an object reference lives across the Upgrade
//> 0: test::m::new();
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::id_mut<sui::package::UpgradeCap>(Input(0));
//> 3: sui::package::authorize_upgrade(Result(2), Input(1), Input(2));
//> 4: Upgrade(q_2, [sui, std], q, Result(3));
//> 5: test::m::write(Result(1), Input(3));
//> 6: sui::package::commit_upgrade(Result(2), Result(4));
//> 7: test::m::check(Result(1), Input(3));
//> 8: TransferObjects([Result(0)], Input(4));
