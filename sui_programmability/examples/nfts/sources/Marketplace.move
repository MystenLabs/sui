module NFTs::Marketplace {
    use Sui::TxContext::{Self, TxContext};
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer::{Self};
    use Sui::Bag::{Self, Bag};
    use Sui::NFT::NFT;
    use Sui::Coin::{Self, Coin};

    // For when amount paid does not match the expected.
    const EAMOUNT_INCORRECT: u64 = 0;

    // For when someone tries to delist without ownership.
    const ENOT_OWNER: u64 = 1;

    struct Marketplace has key {
        id: VersionedID,
        listings: Bag,
        owner: address,
    }  

    /// A single listing which contains the listed NFT and its price in [`Coin<C>`].
    struct Listing<T: store, phantom C> has key, store {
        id: VersionedID,
        nft: NFT<T>,
        ask: u64, // Coin<C>
        owner: address,
    }

    /// Get a reference to the [`Bag`] with listings of this marketplace.
    public fun listings(market: &Marketplace): &Bag {
        &market.listings
    }

    /// Create a new shared Marketplace.
    public fun create(ctx: &mut TxContext) {
        Transfer::share_object(Marketplace {
            id: TxContext::new_id(ctx),
            listings: Bag::new(ctx),
            owner: TxContext::sender(ctx),
        });
    }

    /// List an NFT at the Marketplace.
    public fun list<T: store, C>(
        marketplace: &mut Marketplace,
        nft: NFT<T>,
        ask: u64,
        ctx: &mut TxContext
    ) {
        Bag::add(&mut marketplace.listings, Listing<T, C> {
            nft,
            ask,
            id: TxContext::new_id(ctx),
            owner: TxContext::sender(ctx),
        })
    }

    /// Remove listing and get an NFT back. Only owner can do that.
    public fun delist<T: store, C>(
        market: &mut Marketplace,
        listing: Listing<T, C>,
        ctx: &mut TxContext
    ): NFT<T> {
        let listing = Bag::remove(&mut market.listings, listing);
        let Listing { id, nft, ask: _, owner } = listing;

        assert!(TxContext::sender(ctx) == owner, ENOT_OWNER);

        ID::delete(id);
        nft
    }

    /// Call [`delist`] and transfer NFT to the sender.
    public fun delist_and_take<T: store, C>(
        market: &mut Marketplace,
        listing: Listing<T, C>,
        ctx: &mut TxContext
    ) {
        Transfer::transfer(delist(market, listing, ctx), TxContext::sender(ctx))
    }

    /// Purchase an NFT using a known Listing. Payment is done in Coin<C>.
    /// Amount paid must match the requested amount. If conditions are met,
    /// owner of the NFT gets the payment and buyer receives their NFT.
    public fun buy<T: store, C>(
        market: &mut Marketplace,
        listing: Listing<T, C>,
        paid: Coin<C>,
    ): NFT<T> {
        let listing = Bag::remove(&mut market.listings, listing);
        let Listing { id, nft, ask, owner } = listing;

        assert!(ask == Coin::value(&paid), EAMOUNT_INCORRECT); 

        Transfer::transfer(paid, owner);
        ID::delete(id);
        nft
    }

    /// Call [`buy`] and transfer NFT to the sender.
    public fun buy_and_take<T: store, C>(
        market: &mut Marketplace,
        listing: Listing<T, C>,
        paid: Coin<C>,
        ctx: &mut TxContext
    ) {
        Transfer::transfer(buy(market, listing, paid), TxContext::sender(ctx))
    }
}

#[test_only]
module NFTs::MarketplaceTests {
    use Sui::Bag::Bag;
    use Sui::Transfer;
    use Sui::NFT::{Self, NFT};
    use Sui::Coin::{Self, Coin};
    use Sui::TestScenario::{Self, Scenario};
    use NFTs::Marketplace::{Self, Marketplace, Listing};

    // The coin required to buy a Kitty.
    struct KittyCoin {}

    // Simple Kitty-NFT data structure.
    struct Kitty has store, drop {
        id: u8
    }

    const ADMIN: address = @0xA55;
    const SELLER: address = @0x00A;
    const BUYER: address = @0x00B;

    /// Create a shared [`Marketplace`].
    fun create_marketplace(scenario: &mut Scenario) {
        TestScenario::next_tx(scenario, &ADMIN);
        Marketplace::create(TestScenario::ctx(scenario));
    }

    /// Mint KittyCoin and send it to BUYER.
    fun mint_some_coin(scenario: &mut Scenario) {
        TestScenario::next_tx(scenario, &ADMIN);
        let coin = Coin::mint_for_testing<KittyCoin>(1000, TestScenario::ctx(scenario));
        Transfer::transfer(coin, BUYER);
    }

    /// Mint Kitty NFT and send it to SELLER.
    fun mint_kitty(scenario: &mut Scenario) {
        TestScenario::next_tx(scenario, &ADMIN);
        let nft = NFT::mint(Kitty { id: 1 }, TestScenario::ctx(scenario));
        NFT::transfer(nft, SELLER);
    }

    // SELLER lists Kitty at the Marketplace for 100 KittyCoin.
    fun list_kitty(scenario: &mut Scenario) {
        TestScenario::next_tx(scenario, &SELLER);  
        let mkp = TestScenario::remove_object<Marketplace>(scenario);
        let nft = TestScenario::remove_object<NFT<Kitty>>(scenario);
            
        Marketplace::list<Kitty, KittyCoin>(&mut mkp, nft, 100, TestScenario::ctx(scenario));
        TestScenario::return_object(scenario, mkp);
    }

    #[test]
    fun list_and_delist() {
        let scenario = &mut TestScenario::begin(&ADMIN);

        create_marketplace(scenario);
        mint_kitty(scenario);
        list_kitty(scenario);
        
        TestScenario::next_tx(scenario, &SELLER);
        {
            let mkp = TestScenario::remove_object<Marketplace>(scenario);
            let listing = TestScenario::remove_nested_object<Bag, Listing<Kitty, KittyCoin>>(scenario, Marketplace::listings(&mkp));

            // Do the delist operation on a Marketplace.
            let nft = Marketplace::delist<Kitty, KittyCoin>(&mut mkp, listing, TestScenario::ctx(scenario));
            let kitten = NFT::burn<Kitty>(nft);
        
            assert!(kitten.id == 1, 0);

            TestScenario::return_object(scenario, mkp);
        };
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    fun fail_to_delist() {
        let scenario = &mut TestScenario::begin(&ADMIN);

        create_marketplace(scenario);
        mint_some_coin(scenario);
        mint_kitty(scenario);
        list_kitty(scenario);

        // BUYER attempts to delist Kitty and he has no right to do so. :(
        TestScenario::next_tx(scenario, &BUYER);
        {
            let mkp = TestScenario::remove_object<Marketplace>(scenario);
            let listing = TestScenario::remove_nested_object<Bag, Listing<Kitty, KittyCoin>>(scenario, Marketplace::listings(&mkp));
            
            // Do the delist operation on a Marketplace.
            let nft = Marketplace::delist<Kitty, KittyCoin>(&mut mkp, listing, TestScenario::ctx(scenario));
            let _ = NFT::burn<Kitty>(nft);

            TestScenario::return_object(scenario, mkp);
        };
    }

    #[test]
    fun buy_kitty() {
        let scenario = &mut TestScenario::begin(&ADMIN);

        create_marketplace(scenario);
        mint_some_coin(scenario);
        mint_kitty(scenario);
        list_kitty(scenario);

        // BUYER takes 100 KittyCoin from his wallet and purchases Kitty.
        TestScenario::next_tx(scenario, &BUYER);
        {
            let coin = TestScenario::remove_object<Coin<KittyCoin>>(scenario);
            let mkp = TestScenario::remove_object<Marketplace>(scenario);
            let listing = TestScenario::remove_nested_object<Bag, Listing<Kitty, KittyCoin>>(scenario, Marketplace::listings(&mkp));
            let payment = Coin::withdraw(&mut coin, 100, TestScenario::ctx(scenario));

            // Do the buy call and expect successful purchase.
            let nft = Marketplace::buy<Kitty, KittyCoin>(&mut mkp, listing, payment);
            let kitten = NFT::burn<Kitty>(nft);
            
            assert!(kitten.id == 1, 0);

            TestScenario::return_object(scenario, mkp);
            TestScenario::return_object(scenario, coin);
        };
    }
    
    #[test]
    #[expected_failure(abort_code = 0)]
    fun fail_to_buy() {
        let scenario = &mut TestScenario::begin(&ADMIN);

        create_marketplace(scenario);
        mint_some_coin(scenario);
        mint_kitty(scenario);
        list_kitty(scenario);

        // BUYER takes 100 KittyCoin from his wallet and purchases Kitty.
        TestScenario::next_tx(scenario, &BUYER);
        {
            let coin = TestScenario::remove_object<Coin<KittyCoin>>(scenario);
            let mkp = TestScenario::remove_object<Marketplace>(scenario);
            let listing = TestScenario::remove_nested_object<Bag, Listing<Kitty, KittyCoin>>(scenario, Marketplace::listings(&mkp));
            
            // AMOUNT here is 10 while expected is 100.
            let payment = Coin::withdraw(&mut coin, 10, TestScenario::ctx(scenario));

            // Attempt to buy and expect failure purchase.
            let nft = Marketplace::buy<Kitty, KittyCoin>(&mut mkp, listing, payment);
            let _ = NFT::burn<Kitty>(nft);
            
            TestScenario::return_object(scenario, mkp);
            TestScenario::return_object(scenario, coin);
        };
    }
}
