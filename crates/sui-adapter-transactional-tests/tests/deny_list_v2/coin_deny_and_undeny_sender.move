// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test verifies the basic e2e work flow of coin deny list v2 for the sender.
// A newly created regulated coin type should come with the deny cap object.
// Coin issuer can use the deny cap to deny addresses, which will no longer be able to
// transfer the coin or use it in Move calls as a sender.
// Undeny the address will restore the original behavior.
// This behavior should be immediate and does not require epoch change.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::regulated_coin {
    use sui::coin;
    use sui::deny_list::DenyList;

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

    entry fun assert_address_deny_status(
        deny_list: &DenyList,
        addr: address,
        expected: bool,
    ) {
        let status = coin::deny_list_v2_contains_next_epoch<REGULATED_COIN>(deny_list, addr);
        assert!(status == expected, 0);
    }
}

// This is the deny cap v2 object.
//# view-object 1,3

// Transfer away the newly minted coin works normally.
//# run sui::pay::split_and_transfer --args object(1,1) 1 @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Deny account B.
//# run sui::coin::deny_list_v2_add --args object(0x403) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Assert that the address is denied.
//# run test::regulated_coin::assert_address_deny_status --args immshared(0x403) @B true --sender A

// Try transfer the coin from B. This should now be denied.
//# transfer-object 3,0 --sender B --recipient A

// Try using the coin in a Move call. This should also be denied.
//# run sui::pay::split_and_transfer --args object(3,0) 1 @A --type-args test::regulated_coin::REGULATED_COIN --sender B

// Undeny account B.
//# run sui::coin::deny_list_v2_remove --args object(0x403) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Assert that the address is no longer denied.
//# run test::regulated_coin::assert_address_deny_status --args immshared(0x403) @B false --sender A

// This time the transfer should work.
//# transfer-object 3,0 --sender B --recipient A
