// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// test valid usages of shared coin

//# init --addresses test=0x0 --accounts A --shared-object-deletion true

//# publish

module test::m1 {
    use sui::sui::SUI;
    use sui::coin;

    public fun mint_shared(ctx: &mut TxContext) {
        transfer::public_share_object(coin::zero<SUI>(ctx))
    }
}

//# run test::m1::mint_shared

//# view-object 2,0

//# programmable --sender A --inputs object(2,0)
//> MergeCoins(Gas, [Input(0)])
