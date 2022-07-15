// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests TransferObject should fail for an immutable object

//# init --accounts A B --addresses test=0x0

//# publish

module test::m {
    use sui::transfer;
    use sui::tx_context::TxContext;
    use sui::object::{Self, Info};

    struct S has store, key { info: Info }
    struct Cup<phantom T: store> has store, key { info: Info }

    public entry fun mint_s(ctx: &mut TxContext) {
        let info = object::new(ctx);
        transfer::freeze_object(S { info })
    }
}

//# run test::m::mint_s --sender A

//# view-object 107

//# transfer-object 107 --sender A --recipient B

//# view-object 107
