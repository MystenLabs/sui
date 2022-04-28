// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module NFTs::AuctionTests {
    use Std::Vector;

    use Sui::Coin::{Self, Coin};
    use Sui::SUI::SUI;
    use Sui::ID::{Self, VersionedID};
    use Sui::TestScenario::Self;
    use Sui::TxContext::{Self, TxContext};

    use NFTs::Auction::{Self, Bid};
    use NFTs::AuctionLib::Auction;

    // Error codes.
    const EWRONG_ITEM_VALUE: u64 = 1;

    // Example of an object type that could be sold at an auction.
    struct SomeItemToSell has key, store {
        id: VersionedID,
        value: u64,
    }

    // Initializes the "state of the world" that mimics what should
    // be available in Sui genesis state (e.g., mints and distributes
    // coins to users).
    fun init(ctx: &mut TxContext, bidders: vector<address>) {
        while (!Vector::is_empty(&bidders)) {
            let bidder = Vector::pop_back(&mut bidders);
            let coin = Coin::mint_for_testing(100, ctx);
            Coin::transfer<SUI>(coin, bidder);
        };
    }

    #[test]
    public(script) fun simple_auction_test() {
        let auctioneer = @0xABBA;
        let owner = @0xACE;
        let bidder1 = @0xFACE;
        let bidder2 = @0xCAFE;


        let scenario = &mut TestScenario::begin(&auctioneer);
        {
            let bidders = Vector::empty();
            Vector::push_back(&mut bidders, bidder1);
            Vector::push_back(&mut bidders, bidder2);
            init(TestScenario::ctx(scenario), bidders);
        };

        // a transaction by the item owner to put it for auction
        TestScenario::next_tx(scenario, &owner);
        let ctx = TestScenario::ctx(scenario);
        let to_sell = SomeItemToSell {
            id: TxContext::new_id(ctx),
            value: 42,
        };
        // generate unique auction ID (it would be more natural to
        // generate one in crate_auction and return it, but we cannot
        // do this at the moment)
        let id = TxContext::new_id(ctx);
        // we need to dereference (copy) right here rather wherever
        // auction_id is used - otherwise id would still be considered
        // borrowed and could not be passed argument to a function
        // consuming it
        let auction_id = *ID::inner(&id);
        Auction::create_auction(to_sell, id, auctioneer, ctx);

        // a transaction by the first bidder to create and put a bid
        TestScenario::next_tx(scenario, &bidder1);
        {
            let coin = TestScenario::take_owned<Coin<SUI>>(scenario);

            Auction::bid(coin, auction_id, auctioneer, TestScenario::ctx(scenario));
        };

        // a transaction by the auctioneer to update state of the auction
        TestScenario::next_tx(scenario, &auctioneer);
        {
            let auction = TestScenario::take_owned<Auction<SomeItemToSell>>(scenario);

            let bid = TestScenario::take_owned<Bid>(scenario);
            Auction::update_auction(&mut auction, bid, TestScenario::ctx(scenario));

            TestScenario::return_owned(scenario, auction);
        };
        // a transaction by the second bidder to create and put a bid (a
        // bid will fail as it has the same value as that of the first
        // bidder's)
        TestScenario::next_tx(scenario, &bidder2);
        {
            let coin = TestScenario::take_owned<Coin<SUI>>(scenario);

            Auction::bid(coin, auction_id, auctioneer, TestScenario::ctx(scenario));
        };

        // a transaction by the auctioneer to update state of the auction
        TestScenario::next_tx(scenario, &auctioneer);
        {
            let auction = TestScenario::take_owned<Auction<SomeItemToSell>>(scenario);

            let bid = TestScenario::take_owned<Bid>(scenario);
            Auction::update_auction(&mut auction, bid, TestScenario::ctx(scenario));

            TestScenario::return_owned(scenario, auction);
        };

        // a transaction by the auctioneer to end auction
        TestScenario::next_tx(scenario, &auctioneer);
        {
            let auction = TestScenario::take_owned<Auction<SomeItemToSell>>(scenario);

            Auction::end_auction(auction, TestScenario::ctx(scenario));
        };

        // a transaction to check if the first bidder won (as the
        // second bidder's bid was the same as that of the first one)
        TestScenario::next_tx(scenario, &bidder1);
        {
            let acquired_item = TestScenario::take_owned<SomeItemToSell>(scenario);

            assert!(acquired_item.value == 42, EWRONG_ITEM_VALUE);

            TestScenario::return_owned(scenario, acquired_item);
        };
    }
}
