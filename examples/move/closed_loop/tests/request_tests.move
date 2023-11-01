// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This module implements tests for the request formation and approval in the
/// `TokenPolicy`.
module closed_loop::request_tests {
    use std::string;
    use std::option::none;
    use closed_loop::closed_loop as cl;
    use closed_loop::test_utils as test;

    struct Rule1 has drop {}
    struct Rule2 has drop {}

    #[test]
    /// Scenario: allow test action, create request, confirm request
    fun test_request_confirm() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);
        let action = string::utf8(b"test");

        cl::allow(&mut policy, &cap, action, ctx);

        let request = cl::new_request(action, 100, none(), none(), ctx);

        cl::confirm_request(&mut policy, request, ctx);
        test::return_policy(policy, cap)
    }

    #[test]
    /// Scenario: issue a non-spend request, confirm with `TokenPolicyCap`
    fun test_request_confirm_with_cap() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);
        let token = test::mint(100, ctx);

        let request = cl::transfer(token, @0x2, ctx);
        cl::confirm_with_policy_cap(&cap, request, ctx);
        test::return_policy(policy, cap);
    }

    #[test, expected_failure(abort_code = cl::EUnknownAction)]
    /// Scenario: Policy does not allow test action, create request, try confirm
    fun test_request_confirm_unknown_action_fail() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);
        let action = string::utf8(b"test");

        let request = cl::new_request(action, 100, none(), none(), ctx);

        cl::confirm_request(&mut policy, request, ctx);
        test::return_policy(policy, cap)
    }

    #[test]
    /// Scenario: Policy requires only Rule1 but request gets approval from
    /// Rule2 and Rule1
    fun test_request_confirm_excessive_approvals_pass() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);
        let action = string::utf8(b"test");

        cl::add_rule_for_action(Rule1 {}, &mut policy, &cap, action, ctx);

        let request = cl::new_request(action, 100, none(), none(), ctx);

        cl::add_approval(Rule1 {}, &mut request, ctx);
        cl::add_approval(Rule2 {}, &mut request, ctx);

        cl::confirm_request(&mut policy, request, ctx);
        test::return_policy(policy, cap)
    }

    #[test, expected_failure(abort_code = cl::ENotApproved)]
    /// Scenario: Policy requires Rule1 but request gets approval from Rule2
    fun test_request_confirm_not_approved_fail() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);
        let action = string::utf8(b"test");

        cl::add_rule_for_action(Rule1 {}, &mut policy, &cap, action, ctx);

        let request = cl::new_request(action, 100, none(), none(), ctx);

        cl::add_approval(Rule2 {}, &mut request, ctx);
        cl::confirm_request(&mut policy, request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = cl::ECantConsumeBalance)]
    /// Scenario: issue a Spend request, try to confirm it with `TokenPolicyCap`
    fun test_request_cant_consume_balance_with_cap() {
        let ctx = &mut test::ctx();
        let (_policy, cap) = test::get_policy(ctx);
        let token = test::mint(100, ctx);
        let request = cl::spend(token, ctx);

        cl::confirm_with_policy_cap(&cap, request, ctx);

        abort 1337
    }
}
