// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This module implements tests for the request formation and approval in the
/// `TokenPolicy`.
module sui::token_request_tests {
    use std::option::none;
    use sui::token;
    use sui::token_test_utils::{Self as test, TEST};

    public struct Rule1 has drop {}
    public struct Rule2 has drop {}

    #[test]
    /// Scenario: allow test action, create request, confirm request
    fun test_request_confirm() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);
        let action = b"test".to_string();

        policy.allow(&cap, action, ctx);

        let request = token::new_request(action, 100, none(), none(), ctx);

        policy.confirm_request(request, ctx);
        test::return_policy(policy, cap)
    }

    #[test]
    /// Scenario: issue a non-spend request, confirm with `TokenPolicyCap`
    fun test_request_confirm_with_cap() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);
        let token = test::mint(100, ctx);

        let request = token.transfer(@0x2, ctx);
        cap.confirm_with_policy_cap(request, ctx);
        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: Policy requires only Rule1 but request gets approval from
    /// Rule2 and Rule1
    fun test_request_confirm_excessive_approvals_pass() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);
        let action = b"test".to_string();

        policy.add_rule_for_action<TEST, Rule1>(&cap, action, ctx);

        let mut request = token::new_request(action, 100, none(), none(), ctx);

        token::add_approval(Rule1 {}, &mut request, ctx);
        token::add_approval(Rule2 {}, &mut request, ctx);

        policy.confirm_request(request, ctx);
        test::return_policy(policy, cap)
    }

    #[test, expected_failure(abort_code = token::EUnknownAction)]
    /// Scenario: Policy does not allow test action, create request, try confirm
    fun test_request_confirm_unknown_action_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);
        let action = b"test".to_string();

        let request = token::new_request(action, 100, none(), none(), ctx);

        policy.confirm_request(request, ctx);
        test::return_policy(policy, cap)
    }

    #[test, expected_failure(abort_code = token::ENotApproved)]
    /// Scenario: Policy requires Rule1 but request gets approval from Rule2
    fun test_request_confirm_not_approved_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);
        let action = b"test".to_string();

        policy.add_rule_for_action<TEST, Rule1>(&cap, action, ctx);

        let mut request = token::new_request(action, 100, none(), none(), ctx);

        token::add_approval(Rule2 {}, &mut request, ctx);
        policy.confirm_request(request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = token::ECantConsumeBalance)]
    /// Scenario: issue a Spend request, try to confirm it with `TokenPolicyCap`
    fun test_request_cant_consume_balance_with_cap() {
        let ctx = &mut test::ctx(@0x0);
        let (_policy, cap) = test::get_policy(ctx);
        let token = test::mint(100, ctx);
        let request = token.spend(ctx);

        cap.confirm_with_policy_cap(request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = token::EUseImmutableConfirm)]
    /// Scenario: issue a transfer request, try to confirm it with `_mut`
    fun test_request_use_mutable_confirm_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);
        let token = test::mint(100, ctx);
        let request = token.transfer(@0x2, ctx);

        policy.allow(&cap, token::transfer_action(), ctx);
        policy.confirm_request_mut(request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = token::EUnknownAction)]
    /// Scenario: issue a transfer request with balance, action not allowed
    fun test_request_use_mutable_action_not_allowed_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, _cap) = test::get_policy(ctx);
        let token = test::mint(100, ctx);
        let request = token.spend(ctx);

        policy.confirm_request_mut(request, ctx);

        abort 1337
    }
}
