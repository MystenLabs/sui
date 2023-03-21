// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module nfts::discount_coupon_tests {
    use nfts::discount_coupon::{Self, DiscountCoupon};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::test_scenario::Self;
    use sui::transfer;
    use sui::tx_context::TxContext;

    const ISSUER_ADDRESS: address = @0xA001;
    const USER1_ADDRESS: address = @0xB001;

    // Error codes.
    // const MINT_FAILED: u64 = 0;
    // const TRANSFER_FAILED: u64 = 1;

    // Initializes the "state of the world" that mimics what should
    // be available in Sui genesis state (e.g., mints and distributes
    // coins to users).
    fun init(ctx: &mut TxContext) {
        let coin = coin::mint_for_testing<SUI>(100, ctx);
        transfer::public_transfer(coin, ISSUER_ADDRESS);
    }

    #[test]
    fun test_mint_then_transfer() {
        let scenario_val = test_scenario::begin(ISSUER_ADDRESS);
        let scenario = &mut scenario_val;
        {
            init(test_scenario::ctx(scenario));
        };

        // Mint and transfer NFT + top up recipient's address.
        test_scenario::next_tx(scenario, ISSUER_ADDRESS);
        {
            let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            discount_coupon::mint_and_topup(coin, 10, 1648820870, USER1_ADDRESS, test_scenario::ctx(scenario));
        };

        test_scenario::next_tx(scenario, USER1_ADDRESS);
        {
            assert!(
                test_scenario::has_most_recent_for_sender<DiscountCoupon>(scenario),
                0
            );
            let nft_coupon = test_scenario::take_from_sender<DiscountCoupon>(scenario); // if can remove, object exists
            assert!(discount_coupon::issuer(&nft_coupon) == ISSUER_ADDRESS, 0);
            test_scenario::return_to_sender(scenario, nft_coupon);
        };
        test_scenario::end(scenario_val);
    }
}
