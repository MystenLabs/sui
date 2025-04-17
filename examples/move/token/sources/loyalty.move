// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module illustrates a Closed Loop Loyalty Token. The `Token` is sent to
/// users as a reward for their loyalty by the application Admin. The `Token`
/// can be used to buy a `Gift` in the shop.
///
/// Actions:
/// - spend - spend the token in the shop
module examples::loyalty;

use sui::{coin::{Self, TreasuryCap}, token::{Self, ActionRequest, Token}};

/// Token amount does not match the `GIFT_PRICE`.
const EIncorrectAmount: u64 = 0;

/// The price for the `Gift`.
const GIFT_PRICE: u64 = 10;

/// The OTW for the Token / Coin.
public struct LOYALTY has drop {}

/// This is the Rule requirement for the `GiftShop`. The Rules don't need
/// to be separate applications, some rules make sense to be part of the
/// application itself, like this one.
public struct GiftShop has drop {}

/// The Gift object - can be purchased for 10 tokens.
public struct Gift has key, store {
    id: UID,
}

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
        ctx,
    );

    let (mut policy, policy_cap) = token::new_policy(&treasury_cap, ctx);

    // but we constrain spend by this shop:
    token::add_rule_for_action<LOYALTY, GiftShop>(
        &mut policy,
        &policy_cap,
        token::spend_action(),
        ctx,
    );

    token::share_policy(policy);

    transfer::public_freeze_object(coin_metadata);
    transfer::public_transfer(policy_cap, tx_context::sender(ctx));
    transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
}

/// Handy function to reward users. Can be called by the application admin
/// to reward users for their loyalty :)
///
/// `Mint` is available to the holder of the `TreasuryCap` by default and
/// hence does not need to be confirmed; however, the `transfer` action
/// does require a confirmation and can be confirmed with `TreasuryCap`.
public fun reward_user(
    cap: &mut TreasuryCap<LOYALTY>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    let token = token::mint(cap, amount, ctx);
    let req = token::transfer(token, recipient, ctx);

    token::confirm_with_treasury_cap(cap, req, ctx);
}

/// Buy a gift for 10 tokens. The `Gift` is received, and the `Token` is
/// spent (stored in the `ActionRequest`'s `burned_balance` field).
public fun buy_a_gift(token: Token<LOYALTY>, ctx: &mut TxContext): (Gift, ActionRequest<LOYALTY>) {
    assert!(token::value(&token) == GIFT_PRICE, EIncorrectAmount);

    let gift = Gift { id: object::new(ctx) };
    let mut req = token::spend(token, ctx);

    // only required because we've set this rule
    token::add_approval(GiftShop {}, &mut req, ctx);

    (gift, req)
}
