// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test verifies the basic e2e work flow of coin deny list v2 for receiver of regulated coins.
// Regulated coin issuer can use the deny cap to deny addresses, which will no longer be able to
// receive the coin during execution.
// Undeny the address will restore the original behavior.
// This behavior only gets triggered after an epoch change.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::regulated_coin {
    use sui::coin;

    public struct REGULATED_COIN has drop {}

    fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
        let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
            otw,
            9,
            b"RC",
            b"REGULATED_COIN",
            b"A new regulated coin",
            option::none(),
            false,
            ctx
        );
        let coin = coin::mint(&mut treasury_cap, 10000, ctx);
        transfer::public_transfer(coin, tx_context::sender(ctx));
        transfer::public_transfer(deny_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }
}

// Transfer the regulated coin to @B works normally.
//# run sui::pay::split_and_transfer --args object(1,1) 1 @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Deny account B.
//# run sui::coin::deny_list_v2_add --args object(0x403) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Transfer the regulated coin to @B still works normally.
//# run sui::pay::split_and_transfer --args object(1,1) 1 @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# advance-epoch

// Transfer the regulated coin to @B no longer works.
//# run sui::pay::split_and_transfer --args object(1,1) 1 @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Undeny account B.
//# run sui::coin::deny_list_v2_remove --args object(0x403) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Transfer the regulated coin to @B still does not work.
//# run sui::pay::split_and_transfer --args object(1,1) 1 @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# advance-epoch

// Transfer the regulated coin to @B works now.
//# run sui::pay::split_and_transfer --args object(1,1) 1 @B --type-args test::regulated_coin::REGULATED_COIN --sender A
