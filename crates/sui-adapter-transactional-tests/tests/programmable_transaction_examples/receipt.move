// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 A=0x42

//# publish
module test::m1 {
    public struct PrologueReceipt {}
    public struct Witness has key { id: UID }

    public fun prologue(): PrologueReceipt {
        PrologueReceipt {}
    }

    public fun execute(_: &PrologueReceipt, ctx: &mut TxContext) {
        transfer::transfer(Witness { id: object::new(ctx) }, tx_context::sender(ctx))
    }

    public fun epilogue(r: PrologueReceipt) {
        let PrologueReceipt {} = r;
    }

}

//# programmable

//> 0: test::m1::prologue();
//> test::m1::execute(Result(0));
//> test::m1::epilogue(Result(0));

//# view-object 2,0
