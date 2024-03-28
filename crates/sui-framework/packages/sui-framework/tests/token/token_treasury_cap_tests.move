// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This module implements tests for the TreasuryCap-related functionality such
/// as spending, "flush"-ing, issuing new coins and performing marketplace-like
/// operations.
module sui::token_treasury_cap_tests {
    use sui::token_test_utils as test;
    use sui::token;

    #[test]
    /// Scenario: mint and spend a Token, confirm spending request with the
    /// `TreasuryCap`.
    fun test_treasury_spend_flush() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);
        let mut treasury_cap = test::get_treasury_cap(ctx);

        let token = token::mint(&mut treasury_cap, 1000, ctx);
        let request = token.spend(ctx);

        policy.allow(&cap, token::spend_action(), ctx);
        policy.confirm_request_mut(request, ctx);
        policy.flush(&mut treasury_cap, ctx);

        test::return_treasury_cap(treasury_cap);
        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: mint and spend a Token, confirm spending request with the
    /// `TreasuryCap`.
    fun test_treasury_resolve_request() {
        let ctx = &mut test::ctx(@0x0);
        let mut treasury_cap = test::get_treasury_cap(ctx);

        let token = token::mint(&mut treasury_cap, 1000, ctx);
        let request = token.spend(ctx);

        token::confirm_with_treasury_cap(&mut treasury_cap, request, ctx);
        test::return_treasury_cap(treasury_cap);
    }

    #[test]
    /// Scenario: mint and burn a Token with TreasuryCap.
    fun test_treasury_mint_burn() {
        let ctx = &mut test::ctx(@0x0);
        let mut treasury_cap = test::get_treasury_cap(ctx);

        let token = token::mint(&mut treasury_cap, 1000, ctx);
        token::burn(&mut treasury_cap, token);

        test::return_treasury_cap(treasury_cap);
    }
}
