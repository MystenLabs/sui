// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This module tests `join`, `split`, `zero` and `destroy_zero` functions
module sui::token_public_actions_tests {
    use sui::token_test_utils::{Self as test, TEST};
    use sui::token;

    #[test]
    /// Scenario: mint a Token, split it, merge back, then issue a zero and
    /// destroy it.
    fun test_public_split_join_zero_destroy() {
        let ctx = &mut test::ctx(@0x0);
        let token = test::mint(100, ctx);

        let split = token::split(&mut token, 50, ctx);
        let zero = token::zero<TEST>(ctx);

        token::join(&mut token, split);
        token::join(&mut token, zero);

        let zero = token::split(&mut token, 0, ctx);
        token::destroy_zero(zero);
        token::keep(token, ctx);
    }

    #[test, expected_failure(abort_code = token::ENotZero)]
    /// Scenario: try to destroy a non-zero Token.
    fun test_public_destroy_non_zero_fail() {
        let ctx = &mut test::ctx(@0x0);
        let token = test::mint(100, ctx);

        token::destroy_zero(token)
    }

    #[test, expected_failure(abort_code = token::EBalanceTooLow)]
    /// Scenario: try to split more than in the Token.
    fun test_split_excessive_fail() {
        let ctx = &mut test::ctx(@0x0);
        let token = test::mint(0, ctx);

        let _t = token::split(&mut token, 100, ctx);

        abort 1337
    }
}
