// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module NFTs::SharedAuctionTests {
    use Std::Vector;

    use Sui::Coin::{Self, Coin};
    use Sui::SUI::SUI;
    use Sui::ID::VersionedID;
    use Sui::TestScenario::Self;
    use Sui::TxContext::{Self, TxContext};

    use NFTs::SharedAuction;
    use NFTs::AuctionLib::Auction;


    const COIN_VALUE: u64 = 100;

    // Error codes.
    const EWRONG_ITEM_VALUE: u64 = 1;
    const EWRONG_COIN_VALUE: u64 = 2;

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
            let coin = Coin::mint_for_testing(COIN_VALUE, ctx);
            Coin::transfer<SUI>(coin, bidder);
        };
    }

    #[test]
    public(script) fun simple_auction_test() {
        let admin = @0xABBA; // needed only to initialize "state of the world"
        let owner = @0xACE;
        let bidder1 = @0xFACE;
        let bidder2 = @0xCAFE;


        let scenario = &mut TestScenario::begin(&admin);
        {
            let bidders = Vector::empty();
            Vector::push_back(&mut bidders, bidder1);
            Vector::push_back(&mut bidders, bidder2);
            init(TestScenario::ctx(scenario), bidders);
        };

        // a transaction by the item owner to put it for auction
        TestScenario::next_tx(scenario, &owner);
        let ctx = TestScenario::ctx(scenario);
        {
            let to_sell = SomeItemToSell {
                id: TxContext::new_id(ctx),
                value: 42,
            };
            SharedAuction::create_auction(to_sell, ctx);
        };

        // a transaction by the first bidder to put a bid
        TestScenario::next_tx(scenario, &bidder1);
        {
            let coin = TestScenario::take_object<Coin<SUI>>(scenario);
            let auction_wrapper = TestScenario::take_shared_object<Auction<SomeItemToSell>>(scenario);
            let auction = TestScenario::borrow_mut(&mut auction_wrapper);

            SharedAuction::bid(coin, auction, TestScenario::ctx(scenario));

            TestScenario::return_shared_object(scenario, auction_wrapper);
        };

        // a transaction by the second bidder to put a bid (a bid will
        // fail as it has the same value as that of the first
        // bidder's)
        TestScenario::next_tx(scenario, &bidder2);
        {
            let coin = TestScenario::take_object<Coin<SUI>>(scenario);
            let auction_wrapper = TestScenario::take_shared_object<Auction<SomeItemToSell>>(scenario);
            let auction = TestScenario::borrow_mut(&mut auction_wrapper);

            SharedAuction::bid(coin, auction, TestScenario::ctx(scenario));

            TestScenario::return_shared_object(scenario, auction_wrapper);
        };

        // a transaction by the second bidder to verify that the funds
        // have been returned (as a result of the failed bid).
        TestScenario::next_tx(scenario, &bidder2);
        {
            let coin = TestScenario::take_object<Coin<SUI>>(scenario);

            assert!(Coin::value(&coin) == COIN_VALUE, EWRONG_COIN_VALUE);

            TestScenario::return_object(scenario, coin);
        };

        // a transaction by the owner to end auction
        TestScenario::next_tx(scenario, &owner);
        {
            let auction_wrapper = TestScenario::take_shared_object<Auction<SomeItemToSell>>(scenario);
            let auction = TestScenario::borrow_mut(&mut auction_wrapper);

            // pass auction as mutable reference as its a shared
            // object that cannot be deleted
            SharedAuction::end_auction(auction, TestScenario::ctx(scenario));

            TestScenario::return_shared_object(scenario, auction_wrapper);
        };

        // a transaction to check if the first bidder won (as the
        // second bidder's bid was the same as that of the first one)
        TestScenario::next_tx(scenario, &bidder1);
        {
            let acquired_item = TestScenario::take_object<SomeItemToSell>(scenario);

            assert!(acquired_item.value == 42, EWRONG_ITEM_VALUE);

            TestScenario::return_object(scenario, acquired_item);
        };
    }
}
