// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module kiosk::royalty_rule_tests {
    use sui::coin;
    use sui::sui::SUI;
    use sui::tx_context::dummy as ctx;
    use sui::transfer_policy as policy;
    use sui::transfer_policy_tests as test;

    use kiosk::royalty_rule;

    #[test]
    fun test_default_flow_0() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // 0% royalty; min 0 MIST
        royalty_rule::add(&mut policy, &cap, 0, 0);

        let request = policy::new_request(test::fresh_id(ctx), 100_000, test::fresh_id(ctx));
        let payment = coin::mint_for_testing<SUI>(0, ctx);

        royalty_rule::pay(&mut policy, &mut request, payment);
        policy::confirm_request(&mut policy, request);

        let profits = test::wrapup(policy, cap, ctx);
        assert!(profits == 0, 1);
    }

    #[test]
    #[expected_failure(abort_code = kiosk::royalty_rule::EInsufficientAmount)]
    fun test_default_flow_0_invalid_amount_fail() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // 0% royalty; min 0 MIST
        royalty_rule::add(&mut policy, &cap, 0, 0);

        let request = policy::new_request(test::fresh_id(ctx), 100_000, test::fresh_id(ctx));
        let payment = coin::mint_for_testing<SUI>(10, ctx);

        royalty_rule::pay(&mut policy, &mut request, payment);
        policy::confirm_request(&mut policy, request);

        let profits = test::wrapup(policy, cap, ctx);
        assert!(profits == 0, 1);
    }

    #[test]
    fun test_default_flow_1() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // 1% royalty; min 0 MIST
        royalty_rule::add(&mut policy, &cap, 100, 0);

        let request = policy::new_request(test::fresh_id(ctx), 100_000, test::fresh_id(ctx));
        let payment = coin::mint_for_testing<SUI>(1000, ctx);

        royalty_rule::pay(&mut policy, &mut request, payment);
        policy::confirm_request(&mut policy, request);

        let profits = test::wrapup(policy, cap, ctx);
        assert!(profits == 1000, 1);
    }

    #[test]
    fun test_default_flow_100() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // 100% royalty; min 0 MIST
        royalty_rule::add(&mut policy, &cap, 10_000, 0);

        let request = policy::new_request(test::fresh_id(ctx), 100_000, test::fresh_id(ctx));
        let payment = coin::mint_for_testing<SUI>(100_000, ctx);

        royalty_rule::pay(&mut policy, &mut request, payment);
        policy::confirm_request(&mut policy, request);

        let profits = test::wrapup(policy, cap, ctx);
        assert!(profits == 100_000, 1);
    }

    #[test]
    fun test_default_flow_1_min_10_000() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // 1% royalty; min 10_000 MIST
        royalty_rule::add(&mut policy, &cap, 100, 10_000);

        let request = policy::new_request(test::fresh_id(ctx), 100_000, test::fresh_id(ctx));
        let payment = coin::mint_for_testing<SUI>(10_000, ctx);

        royalty_rule::pay(&mut policy, &mut request, payment);
        policy::confirm_request(&mut policy, request);
        assert!(test::wrapup(policy, cap, ctx) == 10_000, 1);
    }

    #[test]
    fun test_default_flow_10_min_10_000() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // 10% royalty; min 10_000 MIST
        royalty_rule::add(&mut policy, &cap, 1000, 10_000);

        let request = policy::new_request(test::fresh_id(ctx), 100_000, test::fresh_id(ctx));
        let payment = coin::mint_for_testing<SUI>(10_000, ctx);

        royalty_rule::pay(&mut policy, &mut request, payment);
        policy::confirm_request(&mut policy, request);
        assert!(test::wrapup(policy, cap, ctx) == 10_000, 1);
    }

    #[test]
    fun test_default_flow_20_min_10_000() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // 20% royalty; min 10_000 MIST
        royalty_rule::add(&mut policy, &cap, 20_00, 10_000);

        let request = policy::new_request(test::fresh_id(ctx), 100_000, test::fresh_id(ctx));
        let payment = coin::mint_for_testing<SUI>(20_000, ctx);

        royalty_rule::pay(&mut policy, &mut request, payment);
        policy::confirm_request(&mut policy, request);
        assert!(test::wrapup(policy, cap, ctx) == 20_000, 1);
    }

    #[test]
    #[expected_failure(abort_code = kiosk::royalty_rule::EIncorrectArgument)]
    fun test_incorrect_config() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        royalty_rule::add(&mut policy, &cap, 11_000, 0);
        test::wrapup(policy, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = kiosk::royalty_rule::EInsufficientAmount)]
    fun test_insufficient_amount() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // 1% royalty
        royalty_rule::add(&mut policy, &cap, 100, 0);

        // Requires 1_000 MIST, coin has only 999
        let request = policy::new_request(test::fresh_id(ctx), 100_000, test::fresh_id(ctx));
        let payment = coin::mint_for_testing<SUI>(999, ctx);

        royalty_rule::pay(&mut policy, &mut request, payment);
        policy::confirm_request(&mut policy, request);
        test::wrapup(policy, cap, ctx);
    }
}
