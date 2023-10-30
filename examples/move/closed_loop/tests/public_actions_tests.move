// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This module tests `join`, `split`, `zero` and `destroy_zero` functions
module closed_loop::public_actions_tests {
    use closed_loop::test_utils::{Self as test, TEST};
    use closed_loop::closed_loop as cl;

    #[test]
    /// Scenario: mint a Token, split it, merge back, then issue a zero and
    /// destroy it.
    fun test_public_split_join_zero_destroy() {
        let ctx = &mut test::ctx();
        let token = test::mint(100, ctx);

        let split = cl::split(&mut token, 50, ctx);
        let zero = cl::zero<TEST>(ctx);

        cl::join(&mut token, split);
        cl::join(&mut token, zero);

        let zero = cl::split(&mut token, 0, ctx);
        cl::destroy_zero(zero);
        cl::keep(token, ctx);
    }

    #[test, expected_failure(abort_code = cl::ENotZero)]
    /// Scenario: try to destroy a non-zero Token.
    fun test_public_destroy_non_zero_fail() {
        let ctx = &mut test::ctx();
        let token = test::mint(100, ctx);

        cl::destroy_zero(token)
    }

    #[test, expected_failure(abort_code = cl::EBalanceTooLow)]
    /// Scenario: try to split more than in the Token.
    fun test_split_excessive_fail() {
        let ctx = &mut test::ctx();
        let token = test::mint(0, ctx);

        let _t = cl::split(&mut token, 100, ctx);

        abort 1337
    }
}
