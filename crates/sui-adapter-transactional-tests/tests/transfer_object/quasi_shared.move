// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests TransferObject should fail for a quasi-shared object

//# init --accounts A B --addresses test=0x0

//# publish

module test::m {
    use sui::transfer;
    use sui::tx_context::TxContext;
    use sui::object::{Self, Info};

    struct S has key { info: Info }
    struct Child has key { info: Info }

    public entry fun mint_s(ctx: &mut TxContext) {
        let info = object::new(ctx);
        transfer::share_object(S { info })
    }

    public entry fun mint_child(s: &mut S, ctx: &mut TxContext) {
        let info = object::new(ctx);
        transfer::transfer_to_object(Child { info }, s);
    }
}

//# run test::m::mint_s

//# run test::m::mint_child --args object(107)

//# view-object 109

//# transfer-object 109 --sender A --recipient B

//# view-object 109
