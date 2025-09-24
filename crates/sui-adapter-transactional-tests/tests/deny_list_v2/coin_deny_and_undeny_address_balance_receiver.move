// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test exercises the deny-list v2 flow when regulated coins are routed through the
// address-balance accumulator APIs. We explicitly enable the accumulator feature so that
// `balance::send_to_account` is callable from PTBs, then confirm the receiver transitions through
// allowed, denied-after-epoch, and re-enabled states.

//# init --accounts A B --addresses test=0x0 --enable-accumulators --simulator

//# publish --sender A
module test::regulated_coin {
    use sui::balance;
    use sui::coin;

    public struct REGULATED_COIN has drop {}

    #[allow(deprecated_usage)]
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

    public fun split_to_balance(
        coin: &mut coin::Coin<REGULATED_COIN>,
        amount: u64,
    ): balance::Balance<REGULATED_COIN> {
        balance::split(coin::balance_mut(coin), amount)
    }
}

// Initial transfer should succeed before any deny-list action is taken.
//# programmable --sender A --inputs object(1,1) 1 @B
//> 0: test::regulated_coin::split_to_balance(Input(0), Input(1));
//> 1: sui::balance::send_to_account<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2));

// Deny account B.
//# run sui::coin::deny_list_v2_add --args object(0x403) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Deny entry is not enforced until the next epoch, so the transfer still succeeds.
//# programmable --sender A --inputs object(1,1) 1 @B
//> 0: test::regulated_coin::split_to_balance(Input(0), Input(1));
//> 1: sui::balance::send_to_account<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2));

//# advance-epoch

// After epoch change, the deny list should block the recipient.
//# programmable --sender A --inputs object(1,1) 1 @B
//> 0: test::regulated_coin::split_to_balance(Input(0), Input(1));
//> 1: sui::balance::send_to_account<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2));

// Undeny account B.
//# run sui::coin::deny_list_v2_remove --args object(0x403) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Removal only takes effect after the next epoch boundary, so this attempt still fails.
//# programmable --sender A --inputs object(1,1) 1 @B
//> 0: test::regulated_coin::split_to_balance(Input(0), Input(1));
//> 1: sui::balance::send_to_account<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2));

//# advance-epoch

// Once the following epoch begins, transfers to @B succeed again.
//# programmable --sender A --inputs object(1,1) 1 @B
//> 0: test::regulated_coin::split_to_balance(Input(0), Input(1));
//> 1: sui::balance::send_to_account<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2));
