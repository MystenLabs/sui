// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests calling public transfer functions

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct Pub has key, store { id: UID }
    public fun pub(ctx: &mut TxContext): Pub { Pub { id: object::new(ctx) } }
}

//# programmable --sender A --inputs @A
//> 0: test::m1::pub();
//> sui::transfer::public_transfer<test::m1::Pub>(Result(0), Input(0));

//# view-object 2,0

//# programmable
//> 0: test::m1::pub();
//> sui::transfer::public_share_object<test::m1::Pub>(Result(0));

//# view-object 4,0

//# programmable
//> 0: test::m1::pub();
//> sui::transfer::public_freeze_object<test::m1::Pub>(Result(0));

//# view-object 6,0
