// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test verifies the basic e2e work flow of coin global pause.
// Regulated coin issuer can enable global pause, which will deny all transfers of such coin.
// Receiving such coin should also be denied at newer epoch.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::regulated_coin {
    use sui::coin::{Self, Coin};
    use sui::deny_list::DenyList;

    public struct REGULATED_COIN has drop {}

    public struct Wrapper has key {
        id: UID,
        coin: Coin<REGULATED_COIN>,
    }

    fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
        let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
            otw,
            9,
            b"RC",
            b"REGULATED_COIN",
            b"A new regulated coin",
            option::none(),
            true,
            ctx
        );
        let coin = coin::mint(&mut treasury_cap, 10000, ctx);
        transfer::public_transfer(coin, tx_context::sender(ctx));
        transfer::public_transfer(deny_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }

    entry fun partial_wrap(coin: &mut Coin<REGULATED_COIN>, ctx: &mut TxContext) {
        let new_coin = coin::split(coin, 1, ctx);
        full_wrap(new_coin, ctx);
    }

    entry fun full_wrap(coin: Coin<REGULATED_COIN>, ctx: &mut TxContext) {
        let wrapper = Wrapper {
            id: object::new(ctx),
            coin,
        };
        transfer::transfer(wrapper, tx_context::sender(ctx));
    }

    entry fun unwrap(wrapper: Wrapper, ctx: &TxContext) {
        let Wrapper {
          id,
          coin
        } = wrapper;
        transfer::public_transfer(coin, tx_context::sender(ctx));
        object::delete(id);
    }

    entry fun assert_global_pause_status(
        deny_list: &DenyList,
        expected: bool,
    ) {
        let status =
            coin::deny_list_v2_is_global_pause_enabled_next_epoch<REGULATED_COIN>(deny_list);
        assert!(status == expected, 0);
    }
}

// Transfer the newly minted coin works normally.
//# run sui::pay::split_and_transfer --args object(1,1) 1 @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Wrap part of the coin in a wrapper object. We will need this later.
//# run test::regulated_coin::partial_wrap --args object(1,1) --sender A

// Create another wrapper. We will need this later.
//# run test::regulated_coin::partial_wrap --args object(1,1) --sender A

// Enable global pause.
//# run sui::coin::deny_list_v2_enable_global_pause --args object(0x403) object(1,3) --type-args test::regulated_coin::REGULATED_COIN --sender A

// Assert that global pause is enabled.
//# run test::regulated_coin::assert_global_pause_status --args immshared(0x403) true --sender A

// Transfer the regulated coin from A no longer works.
//# run sui::pay::split_and_transfer --args object(1,1) 1 @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Transfer the coin from B also no longer works.
//# transfer-object 2,0 --sender B --recipient A

// Try using the coin in a Move call. This should also be denied.
//# run sui::pay::split_and_transfer --args object(2,0) 1 @A --type-args test::regulated_coin::REGULATED_COIN --sender B

// Unwrap the wrapper. This still works and A will receive the coin, since global pause check
// for receiving will only take effect at the new epoch.
//# run test::regulated_coin::unwrap --args object(3,0) --sender A

//# advance-epoch

// Unwrap the other wrapper. This should be denied now since global pause
// will also apply to execution in the new epoch. A cannot receive the coin.
//# run test::regulated_coin::unwrap --args object(4,0) --sender A

// Transfer still doesn't work as the previous epoch.
//# run sui::pay::split_and_transfer --args object(1,1) 1 @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Assert that global pause is still enabled.
//# run test::regulated_coin::assert_global_pause_status --args immshared(0x403) true --sender A

// Disable global pause.
//# run sui::coin::deny_list_v2_disable_global_pause --args object(0x403) object(1,3) --type-args test::regulated_coin::REGULATED_COIN --sender A

// Assert that global pause is disabled.
//# run test::regulated_coin::assert_global_pause_status --args immshared(0x403) false --sender A

// Transfer the regulated coin from A works again.
//# run sui::pay::split_and_transfer --args object(1,1) 1 @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Create a new wrapper. This works now since spending coin is allowed again.
//# run test::regulated_coin::full_wrap --args object(1,1) --sender A

// However unwrapping still does not work since it involves receiving coin.
// This is still disabled until the next epoch.
//# run test::regulated_coin::unwrap --args object(18,0) --sender A

//# advance-epoch

// Unwrap the wrapper. This works now since receiving coin is allowed again.
//# run test::regulated_coin::unwrap --args object(18,0) --sender A
