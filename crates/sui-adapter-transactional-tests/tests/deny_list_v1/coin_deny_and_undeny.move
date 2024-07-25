// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test verifies the basic e2e work flow of coin deny list.
// A newly created regulated coin type should come with the deny cap object.
// Coin isser can use the deny cap to deny addresses, which will no longer be able to
// transfer the coin or use it in Move calls.
// Undeny the address will restore the original behavior.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
#[allow(deprecated_usage)]
module test::regulated_coin {
    use sui::coin;

    public struct REGULATED_COIN has drop {}

    fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
        let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency(
            otw,
            9,
            b"RC",
            b"REGULATED_COIN",
            b"A new regulated coin",
            option::none(),
            ctx
        );
        let coin = coin::mint(&mut treasury_cap, 10000, ctx);
        transfer::public_transfer(coin, tx_context::sender(ctx));
        transfer::public_transfer(deny_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }
}

//# view-object 1,0

//# view-object 1,1

//# view-object 1,2

//# view-object 1,3

//# view-object 1,4

//# view-object 1,5

// Transfer away the newly minted coin works normally.
//# run sui::pay::split_and_transfer --args object(1,1) 1 @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Deny account B.
//# run sui::coin::deny_list_add --args object(0x403) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Try transfer the coin from B. This should now be denied.
//# transfer-object 8,0 --sender B --recipient A

// Try using the coin in a Move call. This should also be denied.
//# run sui::pay::split_and_transfer --args object(8,0) 1 @A --type-args test::regulated_coin::REGULATED_COIN --sender B

// Undeny account B.
//# run sui::coin::deny_list_remove --args object(0x403) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// This time the transfer should work.
//# transfer-object 8,0 --sender B --recipient A
