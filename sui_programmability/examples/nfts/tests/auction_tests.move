// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module nfts::auction_tests {
    use std::vector;

    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::object::{Self, UID};
    use sui::test_scenario::Self;
    use sui::transfer;
    use sui::tx_context::TxContext;

    use nfts::auction::{Self, Bid};
    use nfts::auction_lib::Auction;

    // Error codes.
    const EWRONG_ITEM_VALUE: u64 = 1;

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
            let coin = coin::mint_for_testing<SUI>(100, ctx);
            transfer::public_transfer(coin, bidder);
        };
    }

    #[test]
    fun simple_auction_test() {
        let auctioneer = @0xABBA;
        let owner = @0xACE;
        let bidder1 = @0xFACE;
        let bidder2 = @0xCAFE;

        let scenario_val = test_scenario::begin(auctioneer);
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
        let to_sell = SomeItemToSell {
            id: object::new(ctx),
            value: 42,
        };
        // create the auction
        let auction_id = auction::create_auction(to_sell, auctioneer, ctx);

        // a transaction by the first bidder to create and put a bid
        test_scenario::next_tx(scenario, bidder1);
        {
            let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);

            auction::bid(coin, auction_id, auctioneer, test_scenario::ctx(scenario));
        };

        // a transaction by the auctioneer to update state of the auction
        test_scenario::next_tx(scenario, auctioneer);
        {
            let auction = test_scenario::take_from_sender<Auction<SomeItemToSell>>(scenario);

            let bid = test_scenario::take_from_sender<Bid>(scenario);
            auction::update_auction(&mut auction, bid, test_scenario::ctx(scenario));

            test_scenario::return_to_sender(scenario, auction);
        };
        // a transaction by the second bidder to create and put a bid (a
        // bid will fail as it has the same value as that of the first
        // bidder's)
        test_scenario::next_tx(scenario, bidder2);
        {
            let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);

            auction::bid(coin, auction_id, auctioneer, test_scenario::ctx(scenario));
        };

        // a transaction by the auctioneer to update state of the auction
        test_scenario::next_tx(scenario, auctioneer);
        {
            let auction = test_scenario::take_from_sender<Auction<SomeItemToSell>>(scenario);

            let bid = test_scenario::take_from_sender<Bid>(scenario);
            auction::update_auction(&mut auction, bid, test_scenario::ctx(scenario));

            test_scenario::return_to_sender(scenario, auction);
        };

        // a transaction by the auctioneer to end auction
        test_scenario::next_tx(scenario, auctioneer);
        {
            let auction = test_scenario::take_from_sender<Auction<SomeItemToSell>>(scenario);

            auction::end_auction(auction, test_scenario::ctx(scenario));
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
