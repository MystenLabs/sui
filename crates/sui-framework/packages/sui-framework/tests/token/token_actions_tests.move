// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This testing block makes sure the protected (restricted) actions behave as
/// intended, that the request is well formed and that APIs are usable.
///
/// It also tests custom actions which can be implemented by policy owner.
module sui::token_actions_tests {
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

        assert!(request.action() == token::transfer_action());
        assert!(request.amount() == 1000);
        assert!(request.sender() == @0x0);

        let recipient = request.recipient();

        assert!(recipient.is_some());
        assert!(recipient.borrow() == &@0x2);
        assert!(request.spent().is_none());

        cap.confirm_with_policy_cap(request, ctx);
        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: spend 1000 tokens, make sure the request is well-formed, and
    /// confirm request in the policy making sure the balance is updated.
    fun test_spend_action() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);

        let token = test::mint(1000, ctx);
        let request = token::spend(token, ctx);

        policy.allow(&cap, token::spend_action(), ctx);

        assert!(request.action() == token::spend_action());
        assert!(request.amount() == 1000);
        assert!(request.sender() == @0x0);
        assert!(request.recipient().is_none());
        assert!(request.spent().is_some());
        assert!(request.spent().borrow() == &1000);

        token::confirm_request_mut(&mut policy, request, ctx);

        assert!(token::spent_balance(&policy) == 1000);

        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: turn 1000 tokens into Coin, make sure the request is well-formed,
    /// and perform a from_coin action to turn the Coin back into tokens.
    fun test_to_from_coin_action() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, cap) = test::get_policy(ctx);

        let token = test::mint(1000, ctx);
        let (coin, to_request) = token.to_coin(ctx);

        assert!(to_request.action() == token::to_coin_action());
        assert!(to_request.amount() == 1000);
        assert!(to_request.sender() == @0x0);
        assert!(to_request.recipient().is_none());
        assert!(to_request.spent().is_none());

        let (token, from_request) = token::from_coin(coin, ctx);

        assert!(from_request.action() == token::from_coin_action());
        assert!(from_request.amount() == 1000);
        assert!(from_request.sender() == @0x0);
        assert!(from_request.recipient().is_none());
        assert!(from_request.spent().is_none());

        token.keep(ctx);
        cap.confirm_with_policy_cap(to_request, ctx);
        cap.confirm_with_policy_cap(from_request, ctx);
        test::return_policy(policy, cap);
    }

    #[test]
    /// Scenario: create a custom request, allow it in the policy, make sure
    /// that the request matches the expected values, and confirm it in the
    /// policy.
    fun test_custom_action() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);
        let custom_action = b"custom".to_string();

        policy.allow(&cap, custom_action, ctx);

        let request = token::new_request(
            custom_action,
            1000,
            option::none(),
            option::none(),
            ctx
        );

        assert!(request.action() == custom_action);
        assert!(request.amount() == 1000);
        assert!(request.sender() == @0x0);
        assert!(request.recipient().is_none());
        assert!(request.spent().is_none());

        policy.confirm_request(request, ctx);
        test::return_policy(policy, cap);
    }
}
