#[test_only]
module Basics::AuctionTests {
    use Std::Vector;

    use Sui::Coin::{Self, Coin};
    use Sui::ID::{Self, VersionedID};
    use Sui::TestScenario::Self;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    use Basics::Auction::{Self, ACOIN, Auction, Bid};

    // Everything went well!
    const SUCCESS: u64 = 0;

    // Example of an object type that could be sold at an auction.
    struct SomeItemToSell has key, store {
        id: VersionedID,
        value: u64,
    }

    // Initializes the "state of the world" that mimicks what should
    // be available in Sui genesis state (e.g., mints and distributes
    // coins to users).
    fun init(ctx: &mut TxContext, bidders: vector<address>) {
        let treasury_cap = Coin::create_currency(Auction::coin_type(), ctx);

        while (!Vector::is_empty(&bidders)) {
            let bidder = Vector::pop_back(&mut bidders);
            let coin = Coin::mint(100, &mut treasury_cap, ctx);
            Coin::transfer<ACOIN>(coin, bidder);
        };

        Transfer::transfer(treasury_cap, TxContext::sender(ctx));
    }

    #[test]
    public fun simple_auction_test() {
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

        // a transaction by the first bidder to create an put a bid
        TestScenario::next_tx(scenario, &bidder1);
        {
            let coin = TestScenario::remove_object<Coin<ACOIN>>(scenario);

            Auction::bid(coin, auction_id, auctioneer, TestScenario::ctx(scenario));
        };

        // a transaction by the auctioneer to update state of the auction
        TestScenario::next_tx(scenario, &auctioneer);
        {
            let auction = TestScenario::remove_object<Auction<SomeItemToSell>>(scenario);

            let bid = TestScenario::remove_object<Bid>(scenario);
            Auction::update_auction(&mut auction, bid, TestScenario::ctx(scenario));

            TestScenario::return_object(scenario, auction);

        };
        // a transaction by the second bidder to create an put a bid (a
        // bid will fail as it has the same value as that of the first
        // bidder's)
        TestScenario::next_tx(scenario, &bidder2);
        {
            let coin = TestScenario::remove_object<Coin<ACOIN>>(scenario);

            Auction::bid(coin, auction_id, auctioneer, TestScenario::ctx(scenario));
        };

        // a transaction by the auctioneer to update state of the auction
        TestScenario::next_tx(scenario, &auctioneer);
        {
            let auction = TestScenario::remove_object<Auction<SomeItemToSell>>(scenario);

            let bid = TestScenario::remove_object<Bid>(scenario);
            Auction::update_auction(&mut auction, bid, TestScenario::ctx(scenario));

            TestScenario::return_object(scenario, auction);

        };

        // a transaction by the auctioneer to end auction
        TestScenario::next_tx(scenario, &auctioneer);
        {
            let auction = TestScenario::remove_object<Auction<SomeItemToSell>>(scenario);
            Auction::end_auction(auction, TestScenario::ctx(scenario));
        };

        // a transaction to check if the first bidder won (as the
        // second bidder's bid was the same as that of the first one)
        TestScenario::next_tx(scenario, &bidder1);
        {
            let acquired_item = TestScenario::remove_object<SomeItemToSell>(scenario);
            assert!(acquired_item.value == 42, SUCCESS);
            TestScenario::return_object(scenario, acquired_item);
        };

    }

}
