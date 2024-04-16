// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module regulated_token::reg {
    use std::option;
    use sui::tx_context::{sender, TxContext};
    use sui::transfer;
    use sui::coin::{Self, TreasuryCap};
    use sui::token::{Self, Token, TokenPolicy};

    use regulated_token::denylist_rule::{Self as denylist, Denylist};

    /// The OTW and the type for the Token
    public struct REG has drop {}

    // Create a TreasuryCap in the module initializer.
    // Also create a `TokenPolicy` (while this action can be performed offchain).
    // The policy does not allow any action by default!
    fun init(otw: REG, ctx: &mut TxContext) {
        let (treasury_cap, coin_metadata) = coin::create_currency(
            otw, 6, b"REG", b"Regulated Token", b"Example of a regulated token",
            option::none(), ctx
        );
        let (mut policy, policy_cap) = token::new_policy(&treasury_cap, ctx);

        // Allow transfer and spend by default
        token::allow(&mut policy, &policy_cap, token::transfer_action(), ctx);
        token::allow(&mut policy, &policy_cap, token::spend_action(), ctx);

        token::add_rule_for_action<REG, Denylist>(
            &mut policy, &policy_cap, token::transfer_action(), ctx
        );

        token::add_rule_for_action<REG, Denylist>(
            &mut policy, &policy_cap, token::spend_action(), ctx
        );

        transfer::public_transfer(treasury_cap, ctx.sender());
        transfer::public_transfer(policy_cap, ctx.sender());
        transfer::public_freeze_object(coin_metadata);
        token::share_policy(policy);
    }

    // === Admin entry functions (required for CLI) ===

    /// Mint `Token` with `amount` and transfer it to the `recipient`.
    entry fun mint_and_transfer(
        cap: &mut TreasuryCap<REG>,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext
    ) {
        let token = token::mint(cap, amount, ctx);
        let request = token::transfer(token, recipient, ctx);
        token::confirm_with_treasury_cap(cap, request, ctx);
    }

    // === User-related entry functions ===

    /// Split `amount` from `token` transfer it to the `recipient`.
    entry fun split_and_transfer(
        self: &TokenPolicy<REG>,
        token: &mut Token<REG>,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext
    ) {
        let to_send = token::split(token, amount, ctx);
        let mut request = token::transfer(to_send, recipient, ctx);
        denylist::verify(self, &mut request, ctx);
        token::confirm_request(self, request, ctx);
    }

    /// Transfer `token` to the `recipient`.
    entry fun transfer(
        self: &TokenPolicy<REG>,
        token: Token<REG>,
        recipient: address,
        ctx: &mut TxContext
    ) {
        let mut request = token::transfer(token, recipient, ctx);
        denylist::verify(self, &mut request, ctx);
        token::confirm_request(self, request, ctx);
    }

    /// Spend a given `amount` of `Token`.
    entry fun spend(
        self: &mut TokenPolicy<REG>,
        token: &mut Token<REG>,
        amount: u64,
        ctx: &mut TxContext
    ) {
        let to_spend = token::split(token, amount, ctx);
        let mut request = token::spend(to_spend, ctx);
        denylist::verify(self, &mut request, ctx);
        token::confirm_request_mut(self, request, ctx);
    }
}
