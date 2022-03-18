module DeFi::Marketplace {
    use Sui::TxContext::{Self, TxContext};
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer::{Self};
    use Sui::Bag::{Self, Bag};

    struct Marketplace has key {
        id: VersionedID,
        listings: Bag,
        owner: address,
    }

    struct Listing<T: key + store, phantom C> has key, store {
        id: VersionedID,
        for: T,
        ask: u64, // Coin<C>
    }

    public fun listings(market: &Marketplace): &Bag {
        &market.listings
    }

    public fun create(ctx: &mut TxContext) {
        Transfer::share_object(Marketplace {
            id: TxContext::new_id(ctx),
            listings: Bag::new(ctx),
            owner: TxContext::sender(ctx),
        });
    }

    public fun list<T: key + store, C>(
        market: &mut Marketplace,
        nft: T,
        ask: u64,
        ctx: &mut TxContext
    ) {
        Bag::add(&mut market.listings, Listing<T, C> {
            id: TxContext::new_id(ctx),
            for: nft,
            ask,
        })
    }

    public fun buy<T: key + store, C>(
        market: &mut Marketplace,
        listing: Listing<T, C>,
        pay: u64,
    ): T {
        let listing = Bag::remove(&mut market.listings, listing);
        let Listing { id, for: nft, ask } = listing;

        assert!(ask == pay, 0); // TODO: EAMOUNT_INCORRECT

        ID::delete(id);
        nft
    }
}

#[test_only]
module DeFi::MarketplaceTests {

    // use Sui::Transfer;
    use Sui::TestScenario;
    use Sui::TxContext;
    use Sui::ID::{Self, VersionedID};
    use Sui::Bag::Bag;

    use DeFi::Marketplace::{Self, Marketplace, Listing};

    struct NFT has key, store {
        id: VersionedID
    }

    #[test]
    fun make_my_bag() {
        let user = &@0xA55;
        let seller = &@0x00A;
        let buyer = &@0x00B;
        let scenario = &mut TestScenario::begin(user);
        
        // Someone creates a Marketplace - anyone can do that.
        TestScenario::next_tx(scenario, user);
        {
            let ctx = TestScenario::ctx(scenario);
            Marketplace::create(ctx);
        };

        // Anyone can list anything on this marketplace.
        TestScenario::next_tx(scenario, seller);  
        {
            let mkp = TestScenario::remove_object<Marketplace>(scenario);
            let ctx = TestScenario::ctx(scenario);
            let nft = NFT { id: TxContext::new_id(ctx) };
            
            Marketplace::list<NFT, u64>(&mut mkp, nft, 100, ctx);

            TestScenario::return_object(scenario, mkp);
        };

        // Anyone can buy any listing.
        TestScenario::next_tx(scenario, buyer);
        {
            let mkp = TestScenario::remove_object<Marketplace>(scenario);
            let listing = TestScenario::remove_nested_object<Bag, Listing<NFT, u64>>(scenario, Marketplace::listings(&mkp));

            let nft = Marketplace::buy<NFT, u64>(&mut mkp, listing, 100);
            let NFT { id } = nft;
            ID::delete(id);

            TestScenario::return_object(scenario, mkp);
        };
    }
}
