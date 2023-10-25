// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module illustrates a Closed Loop Loyalty Token. The `Token` is sent to
/// users as a reward for their loyalty by the application Admin. The `Token`
/// can be used to buy a gift in the shop.
///
/// Actions:
/// - spend - spend the token in the shop
module 0x0::loyalty {
    use std::option;
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::coin::{Self, TreasuryCap};
    use sui::tx_context::{Self, TxContext};

    use closed_loop::closed_loop::{Self as cl, ActionRequest, Token};

    /// Token amount does not match the `GIFT_PRICE`.
    const EIncorrectAmount: u64 = 0;

    /// The price for the `Gift`.
    const GIFT_PRICE: u64 = 10;

    /// The OTW for the Token / Coin.
    struct LOYALTY has drop {}

    /// The Gift object - can be purchased for 10 tokens.
    struct Gift has key, store { id: UID }

    // Create a new LOYALTY currency, create a `TokenPolicy` for it and allow
    // everyone to spend `Token`s if they were `reward`ed.
    fun init(otw: LOYALTY, ctx: &mut TxContext) {
        let (treasury_cap, coin_metadata) = coin::create_currency(
            otw,
            0, // no decimals
            b"LOY", // symbol
            b"Loyalty Token", // name
            b"Token for Loyalty", // description
            option::none(), // url
            ctx
        );

        // create and share the `TokenPolicy`, use the cap to initialize the
        let (policy, policy_cap) = cl::new(&mut treasury_cap, ctx);

        // we allow spending the balance in the shop
        // how about a handy alias: cl::allow(policy_cap, cl::spend_name())
        cl::allow(&mut policy, &policy_cap, cl::spend_name(), ctx);
        cl::share_policy(policy);

        transfer::public_freeze_object(coin_metadata);
        transfer::public_transfer(policy_cap, tx_context::sender(ctx));
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
    }

    /// Handy function to reward users. Can be called by the application admin
    /// to reward users for their loyalty :)
    ///
    /// Uses two actions: `from_coin` to mint the
    public fun reward_user(
        cap: &mut TreasuryCap<LOYALTY>,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext
    ) {
        let coin = coin::mint(cap, amount, ctx);
        let (token, req_1) = cl::from_coin(coin, ctx);
        let req_2 = cl::transfer(token, recipient, ctx);

        cl::confirm_with_treasury_cap(cap, req_1, ctx);
        cl::confirm_with_treasury_cap(cap, req_2, ctx);
    }

    /// Buy a gift for 10 tokens.
    public fun buy_a_gift(
        token: Token<LOYALTY>,
        ctx: &mut TxContext
    ): (Gift, ActionRequest<LOYALTY>) {
        assert!(cl::value(&token) == GIFT_PRICE, EIncorrectAmount);

        let gift = Gift { id: object::new(ctx) };
        let req = cl::spend(token, ctx);

        (gift, req)
    }
}
