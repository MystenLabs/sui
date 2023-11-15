// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This module implements tests for the request formation and approval in the
/// `TokenPolicy`.
module sui::token_request_tests {
    use std::string;
    use std::option::none;
    use sui::token;
    use sui::token_test_utils::{Self as test, TEST};

    struct Rule1 has drop {}
    struct Rule2 has drop {}

    #[test]
    /// Scenario: allow test action, create request, confirm request
    fun test_request_confirm() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);
        let action = string::utf8(b"test");

        token::allow(&mut policy, &cap, action, ctx);

        let request = token::new_request(action, 100, none(), none(), ctx);

        token::confirm_request(&policy, request, ctx);
        test::return_policy(policy, cap)
    }

    #[test]
    /// Scenario: issue a non-spend request, confirm with `TokenPolicyCap`
    fun test_request_confirm_with_cap() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);
        let token = test::mint(100, ctx);

        let request = token::transfer(token, @0x2, ctx);
        token::confirm_with_policy_cap(&cap, request, ctx);
        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: Policy requires only Rule1 but request gets approval from
    /// Rule2 and Rule1
    fun test_request_confirm_excessive_approvals_pass() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);
        let action = string::utf8(b"test");

        token::add_rule_for_action<TEST, Rule1>(&mut policy, &cap, action, ctx);

        let request = token::new_request(action, 100, none(), none(), ctx);

        token::add_approval(Rule1 {}, &mut request, ctx);
        token::add_approval(Rule2 {}, &mut request, ctx);

        token::confirm_request(&policy, request, ctx);
        test::return_policy(policy, cap)
    }

    #[test, expected_failure(abort_code = token::EUnknownAction)]
    /// Scenario: Policy does not allow test action, create request, try confirm
    fun test_request_confirm_unknown_action_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);
        let action = string::utf8(b"test");

        let request = token::new_request(action, 100, none(), none(), ctx);

        token::confirm_request(&policy, request, ctx);
        test::return_policy(policy, cap)
    }

    #[test, expected_failure(abort_code = token::ENotApproved)]
    /// Scenario: Policy requires Rule1 but request gets approval from Rule2
    fun test_request_confirm_not_approved_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);
        let action = string::utf8(b"test");

        token::add_rule_for_action<TEST, Rule1>(&mut policy, &cap, action, ctx);

        let request = token::new_request(action, 100, none(), none(), ctx);

        token::add_approval(Rule2 {}, &mut request, ctx);
        token::confirm_request(&policy, request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = token::ECantConsumeBalance)]
    /// Scenario: issue a Spend request, try to confirm it with `TokenPolicyCap`
    fun test_request_cant_consume_balance_with_cap() {
        let ctx = &mut test::ctx(@0x0);
        let (_policy, cap) = test::get_policy(ctx);
        let token = test::mint(100, ctx);
        let request = token::spend(token, ctx);

        token::confirm_with_policy_cap(&cap, request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = token::EUseImmutableConfirm)]
    /// Scenario: issue a transfer request, try to confirm it with `_mut`
    fun test_request_use_mutable_confirm_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);
        let token = test::mint(100, ctx);
        let request = token::transfer(token, @0x2, ctx);

        token::allow(&mut policy, &cap, token::transfer_action(), ctx);
        token::confirm_request_mut(&mut policy, request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = token::EUnknownAction)]
    /// Scenario: issue a transfer request with balance, action not allowed
    fun test_request_use_mutable_action_not_allowed_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, _cap) = test::get_policy(ctx);
        let token = test::mint(100, ctx);
        let request = token::spend(token, ctx);

        token::confirm_request_mut(&mut policy, request, ctx);

        abort 1337
    }
}
