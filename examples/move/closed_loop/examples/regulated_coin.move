// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This example demonstrates how to use Closed Loop to create a regulated coin
/// that follows different regulatory requirements for actions:
///
/// 1. A new Token can only be minted by admin (out of scope)
/// 2. Tokens can only be transferred between KYC-d (approved) addresses
/// 3. A single transfer can't exceed 3000.00 REG
/// 4. A single "withdraw" operation `to_coin` can't exceed 1000.00 REG
/// 5. All actions are regulated by a denylist rule
///
/// With this set of rules new accounts can either be created by admin (with a
/// mint and transfer operation) or if the account is KYC-d it can be created
/// with a transfer operation from an existing account. Similarly, an account
/// that has "Coin<REG>" can only convert it to `Token<REG>` if it's KYC-d.
///
/// Notes:
///
/// - best analogy for regulated account (Token) and unregulated account (Coin)
/// is a Bank account and Cash. Bank account is regulated and requires KYC to
/// open, Cash is unregulated and can be used by anyone and passed freely.
/// However should someone decide to put Cash into a Bank account, the Bank will
/// require KYC.
///
/// - KYC in this example is represented by an allowlist rule
module examples::regulated_coin {
    use std::option;
    use sui::transfer;
    use sui::coin::{Self, TreasuryCap};
    use sui::tx_context::{sender, TxContext};

    use closed_loop::closed_loop::{Self as cl, TokenPolicy, TokenPolicyCap};

    // import rules and use them for this app
    use examples::allowlist_rule as allowlist;
    use examples::denylist_rule as denylist;
    use examples::limiter_rule as limiter;

    /// OTW and the type for the Token.
    struct REGULATED_COIN has drop {}

    // Most of the magic happens in the initializer for the demonstration
    // purposes; however half of what's happening here could be implemented as
    // a single / set of PTBs.
    fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
        let treasury_cap = create_currency(otw, ctx);
        let (policy, cap) = cl::new(&treasury_cap, ctx);

        set_rules(&mut policy, &cap, ctx);

        transfer::public_transfer(treasury_cap, sender(ctx));
        transfer::public_transfer(cap, sender(ctx));
        cl::share_policy(policy);
    }

    /// Internal: not necessary, but moving this call to a separate function for
    /// better visibility of the Closed Loop setup in `init` and easier testing.
    public(friend) fun set_rules<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        ctx: &mut TxContext
    ) {
        // Create a denylist rule and add it to every action
        // Now all actions are allowed but require a denylist
        denylist::add_for(policy, cap, cl::spend_action(), ctx);
        denylist::add_for(policy, cap, cl::to_coin_action(), ctx);
        denylist::add_for(policy, cap, cl::transfer_action(), ctx);
        denylist::add_for(policy, cap, cl::from_coin_action(), ctx);

        // Set limits for each action:
        // transfer - 3000.00 REG, to_coin - 1000.00 REG
        limiter::add_for(policy, cap, cl::transfer_action(), 3000_000000, ctx);
        limiter::add_for(policy, cap, cl::to_coin_action(), 1000_000000, ctx);

        // Using allowlist to mock a KYC process; transfer and from_coin can
        // only be performed by KYC-d (allowed) addresses. Just like a Bank
        // account.
        allowlist::add_for(policy, cap, cl::from_coin_action(), ctx);
        allowlist::add_for(policy, cap, cl::transfer_action(), ctx);
    }

    /// Internal: not necessary, but moving this call to a separate function for
    /// better visibility of the Closed Loop setup in `init`.
    fun create_currency<T: drop>(
        otw: T,
        ctx: &mut TxContext
    ): TreasuryCap<T> {
        let (treasury_cap, metadata) = coin::create_currency(
            otw, 6,
            b"REG",
            b"Regulated Coin",
            b"Coin that illustrates different regulatory requirements",
            option::none(),
            ctx
        );

        transfer::public_freeze_object(metadata);
        treasury_cap
    }

    #[test_only] friend examples::regulated_coin_tests;
}

#[test_only]
/// Implements tests for most common scenarios for the regulated coin example.
/// We don't test the currency itself but rather use the same set of regulations
/// on a test currency.
module examples::regulated_coin_tests {
    use sui::coin;
    use sui::tx_context::TxContext;

    use closed_loop::closed_loop::{Self as cl, TokenPolicy, TokenPolicyCap};
    use closed_loop::test_utils::{Self as test, TEST};

    use examples::regulated_coin::set_rules;

    use examples::allowlist_rule as allowlist;
    use examples::denylist_rule as denylist;
    use examples::limiter_rule as limiter;

    const ALICE: address = @0x0;
    const BOB: address = @0x1;


    // === Limiter Tests ===

    #[test]
    /// Transfer 3000 REG to self
    fun test_limiter_transfer_allowed_pass() {
        let ctx = &mut test::ctx();
        let (policy, cap) = policy_with_allowlist(ctx);

        let token = test::mint(3000_000000, ctx);
        let request = cl::transfer(token, ALICE, ctx);

        limiter::verify(&policy, &mut request, ctx);
        denylist::verify(&policy, &mut request, ctx);
        allowlist::verify(&policy, &mut request, ctx);

        cl::confirm_request(&mut policy, request, ctx);
        test::return_policy(policy, cap);
    }

    #[test, expected_failure(abort_code = limiter::ELimitExceeded)]
    /// Try to transfer more than 3000.00 REG.
    fun test_limiter_transfer_to_not_allowed_fail() {
        let ctx = &mut test::ctx();
        let (policy, _cap) = policy_with_allowlist(ctx);

        let token = test::mint(3001_000000, ctx);
        let request = cl::transfer(token, ALICE, ctx);

        limiter::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test]
    /// Turn 1000 REG into Coin from.
    fun test_limiter_to_coin_allowed_pass() {
        let ctx = &mut test::ctx();
        let (policy, cap) = policy_with_allowlist(ctx);

        let token = test::mint(1000_000000, ctx);
        let (coin, request) = cl::to_coin(token, ctx);

        limiter::verify(&policy, &mut request, ctx);
        denylist::verify(&policy, &mut request, ctx);
        allowlist::verify(&policy, &mut request, ctx);

        cl::confirm_request(&mut policy, request, ctx);
        test::return_policy(policy, cap);
        coin::burn_for_testing(coin);
    }

    #[test, expected_failure(abort_code = limiter::ELimitExceeded)]
    /// Try to convert more than 1000.00 REG in a single operation.
    fun test_limiter_to_coin_exceeded_fail() {
        let ctx = &mut test::ctx();
        let (policy, _cap) = policy_with_allowlist(ctx);

        let token = test::mint(1001_000000, ctx);
        let (_coin, request) = cl::to_coin(token, ctx);

        limiter::verify(&policy, &mut request, ctx);

        abort 1337
    }

    // === Allowlist Tests ===

    // Test from allowed account is already covered in the
    // `test_limiter_transfer_allowed_pass`

    #[test, expected_failure(abort_code = allowlist::ENotAllowed)]
    /// Try to `transfer` to a not allowed account.
    fun test_allowlist_transfer_to_not_allowed_fail() {
        let ctx = &mut test::ctx();
        let (policy, _cap) = policy_with_allowlist(ctx);

        let token = test::mint(1000_000000, ctx);
        let request = cl::transfer(token, BOB, ctx);

        allowlist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = allowlist::ENotAllowed)]
    /// Try to `from_coin` from a not allowed account.
    fun test_allowlist_from_coin_not_allowed_fail() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);

        set_rules(&mut policy, &cap, ctx);

        let coin = coin::mint_for_testing(1000_000000, ctx);
        let (_token, request) = cl::from_coin(coin, ctx);

        allowlist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    // === Denylist Tests ===

    #[test, expected_failure(abort_code = denylist::EUserBlocked)]
    /// Try to `transfer` from a blocked account.
    fun test_denylist_transfer_fail() {
        let ctx = &mut test::ctx();
        let (policy, _cap) = policy_with_denylist(ctx);

        let token = test::mint(1000_000000, ctx);
        let request = cl::transfer(token, BOB, ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = denylist::EUserBlocked)]
    /// Try to `transfer` to a blocked account.
    fun test_denylist_transfer_to_recipient_fail() {
        let ctx = &mut test::ctx();
        let (policy, _cap) = policy_with_denylist(ctx);

        let token = test::mint(1000_000000, ctx);
        let request = cl::transfer(token, BOB, ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = denylist::EUserBlocked)]
    /// Try to `spend` from a blocked account.
    fun test_denylist_spend_fail() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);

        set_rules(&mut policy, &cap, ctx);
        denylist::add_records(&mut policy, &cap, vector[ BOB ]);

        let token = test::mint(1000_000000, ctx);
        let request = cl::transfer(token, BOB, ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = denylist::EUserBlocked)]
    /// Try to `to_coin` from a blocked account.
    fun test_denylist_to_coin_fail() {
        let ctx = &mut test::ctx();
        let (policy, _cap) = policy_with_denylist(ctx);

        let token = test::mint(1000_000000, ctx);
        let (_coin, request) = cl::to_coin(token, ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = denylist::EUserBlocked)]
    /// Try to `from_coin` from a blocked account.
    fun test_denylist_from_coin_fail() {
        let ctx = &mut test::ctx();
        let (policy, _cap) = policy_with_denylist(ctx);

        let coin = coin::mint_for_testing(1000_000000, ctx);
        let (_token, request) = cl::from_coin(coin, ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    /// Internal: prepare a policy with a denylist rule where sender is banned;
    fun policy_with_denylist(ctx: &mut TxContext): (TokenPolicy<TEST>, TokenPolicyCap<TEST>) {
        let (policy, cap) = test::get_policy(ctx);
        set_rules(&mut policy, &cap, ctx);

        denylist::add_records(&mut policy, &cap, vector[ ALICE ]);
        (policy, cap)
    }

    /// Internal: prepare a policy with an allowlist rule where sender is allowed;
    fun policy_with_allowlist(ctx: &mut TxContext): (TokenPolicy<TEST>, TokenPolicyCap<TEST>) {
        let (policy, cap) = test::get_policy(ctx);
        set_rules(&mut policy, &cap, ctx);

        allowlist::add_records(&mut policy, &cap, vector[ ALICE ]);
        (policy, cap)
    }
}
