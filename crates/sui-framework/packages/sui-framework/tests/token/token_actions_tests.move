// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This testing block makes sure the protected (restricted) actions behave as
/// intended, that the request is well formed and that APIs are usable.
///
/// It also tests custom actions which can be implemented by policy owner.
module sui::token_actions_tests {
    use std::option;
    use std::string;
    use sui::token;
    use sui::token_test_utils as test;

    #[test]
    /// Scenario: perform a transfer operation, and confirm that the request
    /// is well-formed.
    fun test_transfer_action() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);

        let token = test::mint(1000, ctx);
        let request = token::transfer(token, @0x2, ctx);

        assert!(token::action(&request) == token::transfer_action(), 0);
        assert!(token::amount(&request) == 1000, 1);
        assert!(token::sender(&request) == @0x0, 2);

        let recipient = token::recipient(&request);

        assert!(option::is_some(&recipient), 3);
        assert!(option::borrow(&recipient) == &@0x2, 4);
        assert!(option::is_none(&token::spent(&request)), 5);

        token::confirm_with_policy_cap(&cap, request, ctx);
        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: spend 1000 tokens, make sure the request is well-formed, and
    /// confirm request in the policy making sure the balance is updated.
    fun test_spend_action() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);

        let token = test::mint(1000, ctx);
        let request = token::spend(token, ctx);

        token::allow(&mut policy, &cap, token::spend_action(), ctx);

        assert!(token::action(&request) == token::spend_action(), 0);
        assert!(token::amount(&request) == 1000, 1);
        assert!(token::sender(&request) == @0x0, 2);
        assert!(option::is_none(&token::recipient(&request)), 3);
        assert!(option::is_some(&token::spent(&request)), 4);
        assert!(option::borrow(&token::spent(&request)) == &1000, 5);

        token::confirm_request_mut(&mut policy, request, ctx);

        assert!(token::spent_balance(&policy) == 1000, 6);

        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: turn 1000 tokens into Coin, make sure the request is well-formed,
    /// and perform a from_coin action to turn the Coin back into tokens.
    fun test_to_from_coin_action() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);

        let token = test::mint(1000, ctx);
        let (coin, to_request) = token::to_coin(token, ctx);

        assert!(token::action(&to_request) == token::to_coin_action(), 0);
        assert!(token::amount(&to_request) == 1000, 1);
        assert!(token::sender(&to_request) == @0x0, 2);
        assert!(option::is_none(&token::recipient(&to_request)), 3);
        assert!(option::is_none(&token::spent(&to_request)), 4);

        let (token, from_request) = token::from_coin(coin, ctx);

        assert!(token::action(&from_request) == token::from_coin_action(), 5);
        assert!(token::amount(&from_request) == 1000, 6);
        assert!(token::sender(&from_request) == @0x0, 7);
        assert!(option::is_none(&token::recipient(&from_request)), 8);
        assert!(option::is_none(&token::spent(&from_request)), 9);

        token::keep(token, ctx);
        token::confirm_with_policy_cap(&cap, to_request, ctx);
        token::confirm_with_policy_cap(&cap, from_request, ctx);
        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: create a custom request, allow it in the policy, make sure
    /// that the request matches the expected values, and confirm it in the
    /// policy.
    fun test_custom_action() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);
        let custom_action = string::utf8(b"custom");

        token::allow(&mut policy, &cap, custom_action, ctx);

        let request = token::new_request(
            custom_action,
            1000,
            option::none(),
            option::none(),
            ctx
        );

        assert!(token::action(&request) == custom_action, 0);
        assert!(token::amount(&request) == 1000, 1);
        assert!(token::sender(&request) == @0x0, 2);
        assert!(option::is_none(&token::recipient(&request)), 3);
        assert!(option::is_none(&token::spent(&request)), 4);

        token::confirm_request(&policy, request, ctx);
        test::return_policy(policy, cap);
    }
}
