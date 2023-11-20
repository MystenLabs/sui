// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module kiosk::collection_bidding_tests {
    use sui::coin;
    use sui::kiosk;
    use sui::tx_context::TxContext;
    use sui::kiosk_test_utils::{Self as test, Asset};
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap
    };

    use kiosk::marketplace_adapter as mkt_adapter;
    use kiosk::collection_bidding_ext::{Self as bidding};

    /// The Marketplace witness.
    struct MyMarket has drop {}

    #[test]
    fun test_simple_bid() {
        let ctx = &mut test::ctx();
        let (buyer_kiosk, buyer_cap) = test::get_kiosk(ctx);

        // install extension
        bidding::add(&mut buyer_kiosk, &buyer_cap, ctx);

        // place bids on an Asset: 100 MIST
        bidding::place_bids<Asset, MyMarket>(
            &mut buyer_kiosk,
            &buyer_cap,
            vector[ test::get_sui(100, ctx) ],
            ctx
        );

        // prepare the seller Kiosk
        let (seller_kiosk, seller_cap) = test::get_kiosk(ctx);
        let (asset, asset_id) = test::get_asset(ctx);


        // place the asset and create a MarketPurchaseCap
        bidding::add(&mut seller_kiosk, &seller_cap, ctx);
        kiosk::place(&mut seller_kiosk, &seller_cap, asset);

        let mkt_cap = mkt_adapter::new(
            &mut seller_kiosk, &seller_cap, asset_id, 100, ctx
        );

        let (asset_policy, asset_policy_cap) = get_policy<Asset>(ctx);
        let (mkt_policy, mkt_policy_cap) = get_policy<MyMarket>(ctx);

        // take the bid and perform the purchase
        let (asset_request, mkt_request) = bidding::accept_market_bid(
            &mut buyer_kiosk,
            &mut seller_kiosk,
            mkt_cap,
            &asset_policy,
            false,
            ctx
        );

        policy::confirm_request(&asset_policy, asset_request);
        policy::confirm_request(&mkt_policy, mkt_request);

        return_policy(asset_policy, asset_policy_cap, ctx);
        return_policy(mkt_policy, mkt_policy_cap, ctx);

        assert!(kiosk::has_item(&buyer_kiosk, asset_id), 0);
        assert!(!kiosk::has_item(&seller_kiosk, asset_id), 1);

        let asset = kiosk::take(&mut buyer_kiosk, &buyer_cap, asset_id);

        test::return_assets(vector[ asset ]);
        test::return_kiosk(buyer_kiosk, buyer_cap, ctx);
        let amount = test::return_kiosk(seller_kiosk, seller_cap, ctx);

        assert!(amount == 100, 2);
    }

    fun get_policy<T>(ctx: &mut TxContext): (TransferPolicy<T>, TransferPolicyCap<T>) {
        policy::new_for_testing(ctx)
    }

    fun return_policy<T>(policy: TransferPolicy<T>, policy_cap: TransferPolicyCap<T>, ctx: &mut TxContext): u64 {
        let proceeds = policy::destroy_and_withdraw(policy, policy_cap, ctx);
        coin::burn_for_testing(proceeds)
    }
}
