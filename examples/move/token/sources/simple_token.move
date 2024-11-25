// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Create a simple Token with Denylist for every action; all four default
/// actions are allowed as long as the user is not on the denylist.
module examples::simple_token {
    use examples::denylist_rule::Denylist;
    use sui::{
        coin::{Self, TreasuryCap},
        token::{Self, TokenPolicy, TokenPolicyCap},
        tx_context::sender
    };

    /// OTW and the type for the Token.
    public struct SIMPLE_TOKEN has drop {}

    // Most of the magic happens in the initializer for the demonstration
    // purposes; however half of what's happening here could be implemented as
    // a single / set of PTBs.
    fun init(otw: SIMPLE_TOKEN, ctx: &mut TxContext) {
        let treasury_cap = create_currency(otw, ctx);
        let (mut policy, cap) = token::new_policy(&treasury_cap, ctx);

        set_rules(&mut policy, &cap, ctx);

        transfer::public_transfer(treasury_cap, ctx.sender());
        transfer::public_transfer(cap, ctx.sender());
        token::share_policy(policy);
    }

    /// Internal: not necessary, but moving this call to a separate function for
    /// better visibility of the Closed Loop setup in `init` and easier testing.
    public(package) fun set_rules<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        ctx: &mut TxContext,
    ) {
        // Create a denylist rule and add it to every action
        // Now all actions are allowed but require a denylist
        token::add_rule_for_action<T, Denylist>(policy, cap, token::spend_action(), ctx);
        token::add_rule_for_action<T, Denylist>(policy, cap, token::to_coin_action(), ctx);
        token::add_rule_for_action<T, Denylist>(policy, cap, token::transfer_action(), ctx);
        token::add_rule_for_action<T, Denylist>(policy, cap, token::from_coin_action(), ctx);
    }

    /// Internal: not necessary, but moving this call to a separate function for
    /// better visibility of the Closed Loop setup in `init`.
    fun create_currency<T: drop>(otw: T, ctx: &mut TxContext): TreasuryCap<T> {
        let (treasury_cap, metadata) = coin::create_currency(
            otw,
            6,
            b"SMPL",
            b"Simple Token",
            b"Token that showcases denylist",
            option::none(),
            ctx,
        );

        transfer::public_freeze_object(metadata);
        treasury_cap
    }
}

#[test_only]
/// Implements tests for most common scenarios for the regulated coin example.
/// We don't test the currency itself but rather use the same set of regulations
/// on a test currency.
module examples::simple_token_tests {
    use examples::{denylist_rule as denylist, simple_token::set_rules};
    use sui::{
        coin,
        token::{Self, TokenPolicy, TokenPolicyCap},
        token_test_utils::{Self as test, TEST}
    };

    const ALICE: address = @0x0;
    const BOB: address = @0x1;

    // === Denylist Tests ===

    #[test, expected_failure(abort_code = denylist::EUserBlocked)]
    /// Try to `transfer` from a blocked account.
    fun test_denylist_transfer_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, _cap) = policy_with_denylist(ctx);

        let token = test::mint(1000_000000, ctx);
        let mut request = token::transfer(token, BOB, ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = denylist::EUserBlocked)]
    /// Try to `transfer` to a blocked account.
    fun test_denylist_transfer_to_recipient_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, _cap) = policy_with_denylist(ctx);

        let token = test::mint(1000_000000, ctx);
        let mut request = token::transfer(token, BOB, ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = denylist::EUserBlocked)]
    /// Try to `spend` from a blocked account.
    fun test_denylist_spend_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);

        set_rules(&mut policy, &cap, ctx);
        denylist::add_records(&mut policy, &cap, vector[BOB], ctx);

        let token = test::mint(1000_000000, ctx);
        let mut request = token::transfer(token, BOB, ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = denylist::EUserBlocked)]
    /// Try to `to_coin` from a blocked account.
    fun test_denylist_to_coin_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, _cap) = policy_with_denylist(ctx);

        let token = test::mint(1000_000000, ctx);
        let (_coin, mut request) = token::to_coin(token, ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = denylist::EUserBlocked)]
    /// Try to `from_coin` from a blocked account.
    fun test_denylist_from_coin_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, _cap) = policy_with_denylist(ctx);

        let coin = coin::mint_for_testing(1000_000000, ctx);
        let (_token, mut request) = token::from_coin(coin, ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    /// Internal: prepare a policy with a denylist rule where sender is banned;
    fun policy_with_denylist(ctx: &mut TxContext): (TokenPolicy<TEST>, TokenPolicyCap<TEST>) {
        let (mut policy, cap) = test::get_policy(ctx);
        set_rules(&mut policy, &cap, ctx);

        denylist::add_records(&mut policy, &cap, vector[ALICE], ctx);
        (policy, cap)
    }
}
