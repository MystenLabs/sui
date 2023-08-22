// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Tests for the marketplace `marketplace_trading_ext`.
module kiosk::marketplace_trading_ext_tests {
    use sui::coin;
    use sui::object::ID;
    use sui::kiosk_extension;
    use sui::tx_context::TxContext;
    use sui::transfer_policy as policy;
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::kiosk_test_utils::{Self as test, Asset};
    use kiosk::marketplace_trading_ext as ext;

    const PRICE: u64 = 100_000;

    /// Marketplace type.
    struct MyMarket has drop {}

    #[test] fun test_list_and_delist() {
        let ctx = &mut test::ctx();
        let (kiosk, kiosk_cap, asset_id) = prepare(ctx);

        ext::list<Asset, MyMarket>(&mut kiosk, &kiosk_cap, asset_id, PRICE, ctx);

        assert!(ext::is_listed<Asset, MyMarket>(&kiosk, asset_id), 0);
        assert!(ext::price<Asset, MyMarket>(&kiosk, asset_id) == PRICE, 1);

        ext::delist<Asset, MyMarket>(&mut kiosk, &kiosk_cap, asset_id, ctx);

        let asset = kiosk::take(&mut kiosk, &kiosk_cap, asset_id);
        test::return_assets(vector[ asset ]);
        wrapup(kiosk, kiosk_cap, ctx);
    }

    #[test] fun test_list_and_purchase() {
        let ctx = &mut test::ctx();
        let (kiosk, kiosk_cap, asset_id) = prepare(ctx);

        ext::list<Asset, MyMarket>(&mut kiosk, &kiosk_cap, asset_id, PRICE, ctx);

        let coin = test::get_sui(PRICE, ctx);
        let (item, req, mkt_req) = ext::purchase<Asset, MyMarket>(
            &mut kiosk, asset_id, coin, ctx
        );

        // Resolve creator's Policy
        let (policy, policy_cap) = test::get_policy(ctx);
        policy::confirm_request(&policy, req);
        test::return_policy(policy, policy_cap, ctx);

        // Resolve marketplace's Policy
        let (policy, policy_cap) = policy::new_for_testing<MyMarket>(ctx);
        policy::confirm_request(&policy, mkt_req);
        let proceeds = policy::destroy_and_withdraw(policy, policy_cap, ctx);
        coin::destroy_zero(proceeds);

        // Deal with the Asset + Kiosk, KioskOwnerCap
        test::return_assets(vector[ item ]);
        wrapup(kiosk, kiosk_cap, ctx);
    }

    /// Prepare a Kiosk with:
    /// - extension installed
    /// - an asset inside
    fun prepare(ctx: &mut TxContext): (Kiosk, KioskOwnerCap, ID) {
        let (kiosk, kiosk_cap) = test::get_kiosk(ctx);
        let (asset, asset_id) = test::get_asset(ctx);

        kiosk::place(&mut kiosk, &kiosk_cap, asset);
        ext::add(&mut kiosk, &kiosk_cap, ctx);
        (kiosk, kiosk_cap, asset_id)
    }

    /// Wrap everything up; remove the extension and the asset.
    fun wrapup(kiosk: Kiosk, cap: KioskOwnerCap, ctx: &mut TxContext) {
        kiosk_extension::remove<ext::Extension>(&mut kiosk, &cap);
        test::return_kiosk(kiosk, cap, ctx);
    }
}
