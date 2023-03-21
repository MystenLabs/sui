// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module nfts::shared_auction_tests {
    use std::vector;

    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::object::{Self, UID};
    use sui::test_scenario::Self;
    use sui::transfer;
    use sui::tx_context::TxContext;

    use nfts::shared_auction;
    use nfts::auction_lib::Auction;


    const COIN_VALUE: u64 = 100;

    // Error codes.
    const EWRONG_ITEM_VALUE: u64 = 1;
    const EWRONG_COIN_VALUE: u64 = 2;

    // Example of an object type that could be sold at an auction.
    struct SomeItemToSell has key, store {
        id: UID,
        value: u64,
    }

    // Initializes the "state of the world" that mimics what should
    // be available in Sui genesis state (e.g., mints and distributes
    // coins to users).
    fun init_bidders(ctx: &mut TxContext, bidders: vector<address>) {
        while (!vector::is_empty(&bidders)) {
            let bidder = vector::pop_back(&mut bidders);
            let coin = coin::mint_for_testing<SUI>(COIN_VALUE, ctx);
            transfer::public_transfer(coin, bidder);
        };
    }

    #[test]
    fun simple_auction_test() {
        let admin = @0xABBA; // needed only to initialize "state of the world"
        let owner = @0xACE;
        let bidder1 = @0xFACE;
        let bidder2 = @0xCAFE;

        let scenario_val = test_scenario::begin(admin);
        let scenario = &mut scenario_val;
        {
            let bidders = vector::empty();
            vector::push_back(&mut bidders, bidder1);
            vector::push_back(&mut bidders, bidder2);
            init_bidders(test_scenario::ctx(scenario), bidders);
        };

        // a transaction by the item owner to put it for auction
        test_scenario::next_tx(scenario, owner);
        let ctx = test_scenario::ctx(scenario);
        {
            let to_sell = SomeItemToSell {
                id: object::new(ctx),
                value: 42,
            };
            shared_auction::create_auction(to_sell, ctx);
        };

        // a transaction by the first bidder to put a bid
        test_scenario::next_tx(scenario, bidder1);
        {
            let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let auction_val = test_scenario::take_shared<Auction<SomeItemToSell>>(scenario);
            let auction = &mut auction_val;

            shared_auction::bid(coin, auction, test_scenario::ctx(scenario));

            test_scenario::return_shared(auction_val);
        };

        // a transaction by the second bidder to put a bid (a bid will
        // fail as it has the same value as that of the first
        // bidder's)
        test_scenario::next_tx(scenario, bidder2);
        {
            let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let auction_val = test_scenario::take_shared<Auction<SomeItemToSell>>(scenario);
            let auction = &mut auction_val;

            shared_auction::bid(coin, auction, test_scenario::ctx(scenario));

            test_scenario::return_shared(auction_val);
        };

        // a transaction by the second bidder to verify that the funds
        // have been returned (as a result of the failed bid).
        test_scenario::next_tx(scenario, bidder2);
        {
            let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);

            assert!(coin::value(&coin) == COIN_VALUE, EWRONG_COIN_VALUE);

            test_scenario::return_to_sender(scenario, coin);
        };

        // a transaction by the owner to end auction
        test_scenario::next_tx(scenario, owner);
        {
            let auction_val = test_scenario::take_shared<Auction<SomeItemToSell>>(scenario);
            let auction = &mut auction_val;

            // pass auction as mutable reference as its a shared
            // object that cannot be deleted
            shared_auction::end_auction(auction, test_scenario::ctx(scenario));

            test_scenario::return_shared(auction_val);
        };

        // a transaction to check if the first bidder won (as the
        // second bidder's bid was the same as that of the first one)
        test_scenario::next_tx(scenario, bidder1);
        {
            let acquired_item = test_scenario::take_from_sender<SomeItemToSell>(scenario);

            assert!(acquired_item.value == 42, EWRONG_ITEM_VALUE);

            test_scenario::return_to_sender(scenario, acquired_item);
        };
        test_scenario::end(scenario_val);
    }
}
