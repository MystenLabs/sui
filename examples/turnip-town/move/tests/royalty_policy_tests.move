// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module turnip_town::royalty_policy_tests {
    use sui::coin;
    use sui::sui::SUI;
    use sui::test_scenario as ts;
    use sui::transfer_policy as policy;
    use sui::transfer_policy_tests as test;
    use turnip_town::royalty_policy as royalty;

    const ALICE: address = @0xA;

    #[test]
    fun normal_flow() {
        let mut ts = ts::begin(ALICE);

        let (mut policy, cap) = test::prepare(ts.ctx());
        royalty::set(&mut policy, &cap);

        let mut request = policy::new_request(
            test::fresh_id(ts.ctx()),
            100_000,
            test::fresh_id(ts.ctx()),
        );

        // Commission is 1%, so the coin needs to contain at least 1_000 MIST.
        let mut coin = coin::mint_for_testing<SUI>(1_500, ts.ctx());
        royalty::pay(&mut policy, &mut request, &mut coin, ts.ctx());
        policy::confirm_request(&policy, request);

        let remainder = coin::burn_for_testing(coin);
        let profits = test::wrapup(policy, cap, ts.ctx());

        assert!(remainder == 500);
        assert!(profits == 1_000);
        ts.end();
    }

    #[test]
    fun minimum_royalty() {
        let mut ts = ts::begin(ALICE);

        let (mut policy, cap) = test::prepare(ts.ctx());
        royalty::set(&mut policy, &cap);

        let mut request = policy::new_request(
            test::fresh_id(ts.ctx()),
            99,
            test::fresh_id(ts.ctx()),
        );

        // Commission is 1%, which would usually round down to 0, but the policy
        // also has a minimum royalty of 1.
        let mut coin = coin::mint_for_testing<SUI>(10, ts.ctx());
        royalty::pay(&mut policy, &mut request, &mut coin, ts.ctx());
        policy::confirm_request(&policy, request);

        let remainder = coin::burn_for_testing(coin);
        let profits = test::wrapup(policy, cap, ts.ctx());

        assert!(remainder == 9);
        assert!(profits == 1);
        ts.end();
    }

    #[test]
    #[expected_failure(abort_code = royalty::EInsufficientAmount)]
    fun insufficient_amount() {
        let mut ts = ts::begin(ALICE);

        let (mut policy, cap) = test::prepare(ts.ctx());
        royalty::set(&mut policy, &cap);

        let mut request = policy::new_request(
            test::fresh_id(ts.ctx()),
            100_000,
            test::fresh_id(ts.ctx()),
        );

        // Commission is 1%, so the coin needs to contain at least 1_000 MIST.
        let mut coin = coin::mint_for_testing<SUI>(999, ts.ctx());
        royalty::pay(&mut policy, &mut request, &mut coin, ts.ctx());
        abort 0
    }
}
