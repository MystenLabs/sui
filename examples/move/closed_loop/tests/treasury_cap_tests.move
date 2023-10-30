// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This module implements tests for the TreasuryCap-related functionality such
/// as spending, "flush"-ing, issuing new coins and performing marketplace-like
/// operations.
module closed_loop::treasury_cap_tests {
    use closed_loop::test_utils as test;
    use closed_loop::closed_loop as cl;

    #[test]
    /// Scenario: mint and spend a Token, confirm spending request with the
    /// `TreasuryCap`.
    fun test_treasury_spend_flush() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);
        let treasury_cap = test::get_treasury_cap(ctx);

        let token = cl::mint(&mut treasury_cap, 1000, ctx);
        let request = cl::spend(token, ctx);

        cl::allow(&mut policy, &cap, cl::spend_action(), ctx);
        cl::confirm_request(&mut policy, request, ctx);
        cl::flush(&mut policy, &mut treasury_cap, ctx);

        test::return_treasury_cap(treasury_cap);
        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: mint and spend a Token, confirm spending request with the
    /// `TreasuryCap`.
    fun test_treasury_resolve_request() {
        let ctx = &mut test::ctx();
        let treasury_cap = test::get_treasury_cap(ctx);

        let token = cl::mint(&mut treasury_cap, 1000, ctx);
        let request = cl::spend(token, ctx);

        cl::confirm_with_treasury_cap(&mut treasury_cap, request, ctx);
        test::return_treasury_cap(treasury_cap);
    }

    #[test]
    /// Scenario: mint and burn a Token with TreasuryCap.
    fun test_treasury_mint_burn() {
        let ctx = &mut test::ctx();
        let treasury_cap = test::get_treasury_cap(ctx);

        let token = cl::mint(&mut treasury_cap, 1000, ctx);
        cl::burn(&mut treasury_cap, token);

        test::return_treasury_cap(treasury_cap);
    }
}
