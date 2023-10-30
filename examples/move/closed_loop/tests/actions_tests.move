// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This testing block makes sure the protected (restricted) actions behave as
/// intended, that the request is well formed and that APIs are usable.
///
/// It also tests custom actions which can be implemented by policy owner.
module closed_loop::actions_tests {
    use std::option;
    use std::string;
    use closed_loop::closed_loop as cl;
    use closed_loop::test_utils::{Self as test};

    #[test]
    /// Scenario: perform a transfer operation, and confirm that the request
    /// is well-formed.
    fun test_transfer_action() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);

        let token = test::mint(1000, ctx);
        let request = cl::transfer(token, @0x2, ctx);

        assert!(cl::name(&request) == cl::transfer_action(), 0);
        assert!(cl::amount(&request) == 1000, 1);
        assert!(cl::sender(&request) == @0x0, 2);

        let recipient = cl::recipient(&request);

        assert!(option::is_some(&recipient), 3);
        assert!(option::borrow(&recipient) == &@0x2, 4);
        assert!(option::is_none(&cl::spent(&request)), 5);

        cl::confirm_with_policy_cap(&cap, request, ctx);
        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: spend 1000 tokens, make sure the request is well-formed, and
    /// confirm request in the policy making sure the balance is updated.
    fun test_spend_action() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);

        let token = test::mint(1000, ctx);
        let request = cl::spend(token, ctx);

        cl::allow(&mut policy, &cap, cl::spend_action(), ctx);

        assert!(cl::name(&request) == cl::spend_action(), 0);
        assert!(cl::amount(&request) == 1000, 1);
        assert!(cl::sender(&request) == @0x0, 2);
        assert!(option::is_none(&cl::recipient(&request)), 3);
        assert!(option::is_some(&cl::spent(&request)), 4);
        assert!(option::borrow(&cl::spent(&request)) == &1000, 5);

        cl::confirm_request(&mut policy, request, ctx);

        assert!(cl::spent_balance(&policy) == 1000, 6);

        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: turn 1000 tokens into Coin, make sure the request is well-formed,
    /// and perform a from_coin action to turn the Coin back into tokens.
    fun test_to_from_coin_action() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);

        let token = test::mint(1000, ctx);
        let (coin, to_request) = cl::to_coin(token, ctx);

        assert!(cl::name(&to_request) == cl::to_coin_action(), 0);
        assert!(cl::amount(&to_request) == 1000, 1);
        assert!(cl::sender(&to_request) == @0x0, 2);
        assert!(option::is_none(&cl::recipient(&to_request)), 3);
        assert!(option::is_none(&cl::spent(&to_request)), 4);

        let (token, from_request) = cl::from_coin(coin, ctx);

        assert!(cl::name(&from_request) == cl::from_coin_action(), 5);
        assert!(cl::amount(&from_request) == 1000, 6);
        assert!(cl::sender(&from_request) == @0x0, 7);
        assert!(option::is_none(&cl::recipient(&from_request)), 8);
        assert!(option::is_none(&cl::spent(&from_request)), 9);

        cl::keep(token, ctx);
        cl::confirm_with_policy_cap(&cap, to_request, ctx);
        cl::confirm_with_policy_cap(&cap, from_request, ctx);
        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: create a custom request, allow it in the policy, make sure
    /// that the request matches the expected values, and confirm it in the
    /// policy.
    fun test_custom_action() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);
        let custom_action = string::utf8(b"custom");

        cl::allow(&mut policy, &cap, custom_action, ctx);

        let request = cl::new_request(
            custom_action,
            1000,
            option::none(),
            option::none(),
            ctx
        );

        assert!(cl::name(&request) == custom_action, 0);
        assert!(cl::amount(&request) == 1000, 1);
        assert!(cl::sender(&request) == @0x0, 2);
        assert!(option::is_none(&cl::recipient(&request)), 3);
        assert!(option::is_none(&cl::spent(&request)), 4);

        cl::confirm_request(&mut policy, request, ctx);
        test::return_policy(policy, cap);
    }
}
