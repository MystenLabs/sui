// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Tests for the marketplace adapter.
module kiosk::marketplace_adapter_tests {
    use sui::coin;
    use sui::kiosk;
    use sui::object;
    use sui::sui::SUI;
    use sui::transfer_policy as policy;
    use sui::kiosk_test_utils::{Self as test, Asset};

    use kiosk::marketplace_adapter as mkt;

    /// The Marketplace witness.
    struct MyMarket has drop {}

    /// The witness to use in tests.
    struct OTW has drop {}

    // Performs a test of the `new` and `return_cap` functions. Not supposed to
    // abort, and there's only so many scenarios where it can fail due to strict
    // type requirements.
    #[test] fun test_new_return_flow() {
        let ctx = &mut test::ctx();
        let (kiosk, kiosk_cap) = test::get_kiosk(ctx);
        let (asset, asset_id) = test::get_asset(ctx);

        kiosk::place(&mut kiosk, &kiosk_cap, asset);

        let mkt_cap = mkt::new<Asset, MyMarket>(
            &mut kiosk, &kiosk_cap, asset_id, 100000, ctx
        );

        assert!(mkt::item(&mkt_cap) == asset_id, 0);
        assert!(mkt::min_price(&mkt_cap) == 100000, 1);
        assert!(mkt::kiosk(&mkt_cap) == object::id(&kiosk), 2);

        mkt::return_cap(&mut kiosk, mkt_cap, ctx);

        let asset = kiosk::take(&mut kiosk, &kiosk_cap, asset_id);
        test::return_kiosk(kiosk, kiosk_cap, ctx);
        test::return_assets(vector[ asset ]);
    }

    // Perform a `purchase` using the `MarketPurchaseCap`. Type constraints make
    // it impossible to cheat and pass another Cap. So the number of potential
    // fail scenarios is limited and already covered by the base Kiosk
    #[test] fun test_new_purchase_flow() {
        let ctx = &mut test::ctx();
        let (kiosk, kiosk_cap) = test::get_kiosk(ctx);
        let (asset, asset_id) = test::get_asset(ctx);

        kiosk::place(&mut kiosk, &kiosk_cap, asset);

        // Lock an item in the Marketplace
        let mkt_cap = mkt::new<Asset, MyMarket>(
            &mut kiosk, &kiosk_cap, asset_id, 100000, ctx
        );

        // Mint a Coin and make a purchase
        let coin = coin::mint_for_testing<SUI>(100000, ctx);
        let (item, req, mkt_req) = mkt::purchase<Asset, MyMarket>(
            &mut kiosk, mkt_cap, coin, ctx
        );

        // Get Policy for the Asset, use it and clean up.
        let (policy, policy_cap) = test::get_policy(ctx);
        policy::confirm_request(&policy, req);
        test::return_policy(policy, policy_cap, ctx);

        // Get Policy for the Marketplace, use it and clean up.
        let (policy, policy_cap) = policy::new_for_testing<MyMarket>(ctx);
        policy::confirm_request(&policy, mkt_req);
        let proceeds = policy::destroy_and_withdraw(policy, policy_cap, ctx);
        coin::destroy_zero<SUI>(proceeds);

        // Now deal with the item and with the Kiosk.
        test::return_assets(vector[ item ]);
        test::return_kiosk(kiosk, kiosk_cap, ctx);
    }
}
