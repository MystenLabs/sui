// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests TransferObject with an object with public transfer

//# init --accounts A B --addresses test=0x0

//# publish

module test::m {
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::object::{Self, UID};

    struct S has store, key { id: UID }
    struct Cup<phantom T: store> has store, key { id: UID }

    public entry fun mint_s(ctx: &mut TxContext) {
        let id = object::new(ctx);
        transfer::transfer(S { id }, tx_context::sender(ctx))
    }

    public entry fun mint_cup<T: store>(ctx: &mut TxContext) {
        let id = object::new(ctx);
        transfer::transfer(Cup<T> { id }, tx_context::sender(ctx))
    }
}

// Mint S to A. Transfer S from A to B

//# run test::m::mint_s --sender A

//# view-object 107

//# transfer-object 107 --sender A --recipient B

//# view-object 107


// Mint Cup<S> to A. Transfer Cup<S> from A to B

//# run test::m::mint_cup --type-args test::m::S --sender A

//# view-object 110

//# transfer-object 110 --sender A --recipient B

//# view-object 110
