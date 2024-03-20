// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module kiosk::floor_price_rule_tests {
    use sui::tx_context::dummy as ctx;
    use sui::transfer_policy as policy;
    use sui::transfer_policy_tests as test;

    use kiosk::floor_price_rule;

    #[test]
    fun test_floor_price_rule_default() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // Minimum sale price: 1 SUI.
        floor_price_rule::add(&mut policy, &cap, 1_000_000_000);

        let request = policy::new_request(test::fresh_id(ctx), 1_000_000_000, test::fresh_id(ctx));

        floor_price_rule::prove(&mut policy, &mut request);
        policy::confirm_request(&mut policy, request);

        test::wrapup(policy, cap, ctx);
    }

    #[test]
    fun test_floor_price_rule_high_price() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // Minimum sale price: 1 SUI.
        floor_price_rule::add(&mut policy, &cap, 1_000_000_000);

        let request = policy::new_request(test::fresh_id(ctx), 100_000_000_000, test::fresh_id(ctx));

        floor_price_rule::prove(&mut policy, &mut request);
        policy::confirm_request(&mut policy, request);

        test::wrapup(policy, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = floor_price_rule::EPriceTooSmall)]
    fun fail_test_smaller_price() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // Minimum sale price: 1 SUI.
        floor_price_rule::add(&mut policy, &cap, 1_000_000_000);

        // Attemps a failed purchase with .99 SUI.
        let request = policy::new_request(test::fresh_id(ctx), 999_999_999, test::fresh_id(ctx));

        floor_price_rule::prove(&mut policy, &mut request);
        policy::confirm_request(&mut policy, request);

        test::wrapup(policy, cap, ctx);
    }
}
