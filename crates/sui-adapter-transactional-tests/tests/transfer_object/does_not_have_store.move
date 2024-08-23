// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests TransferObject should fail for an object _without_ public transfer

//# init --accounts A B --addresses test=0x0

//# publish

module test::m {
    public struct S has key { id: UID }
    public struct Cup<phantom T> has key { id: UID }

    public entry fun mint_s(ctx: &mut TxContext) {
        let id = object::new(ctx);
        transfer::transfer(S { id }, tx_context::sender(ctx))
    }

    public entry fun mint_cup<T>(ctx: &mut TxContext) {
        let id = object::new(ctx);
        transfer::transfer(Cup<T> { id }, tx_context::sender(ctx))
    }
}

// Mint S to A. Fail to transfer S from A to B, which should fail

//# run test::m::mint_s --sender A

//# view-object 2,0

//# transfer-object 2,0 --sender A --recipient B

//# view-object 2,0


// Mint Cup<S> to A. Fail to transfer Cup<S> from A to B

//# run test::m::mint_cup --type-args test::m::S --sender A

//# view-object 6,0

//# transfer-object 6,0 --sender A --recipient B

//# view-object 6,0
