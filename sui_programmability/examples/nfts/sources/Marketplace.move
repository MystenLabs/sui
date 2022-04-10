// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module NFTs::Marketplace {
    use Sui::Bag::{Self, Bag};
    use Sui::TxContext::{Self, TxContext};
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer::{Self, ChildRef};
    use Sui::Coin::{Self, Coin};

    // For when amount paid does not match the expected.
    const EAMOUNT_INCORRECT: u64 = 0;

    // For when someone tries to delist without ownership.
    const ENOT_OWNER: u64 = 1;

    // For when trying to remove object that's not on the Marketplace.
    const EOBJECT_NOT_FOUND: u64 = 2;

    /// Adding the same object to the markeplace twice is not allowed.
    const EOBJECT_DOUBLE_ADD: u64 = 3;

    struct Marketplace has key {
        id: VersionedID,
        objects: ChildRef<Bag>,
    }

    /// A single listing which contains the listed item and its price in [`Coin<C>`].
    struct Listing<T: key + store, phantom C> has key, store {
        id: VersionedID,
        item: T,
        ask: u64, // Coin<C>
        owner: address,
    }

    /// Create a new shared Marketplace.
    public fun create(ctx: &mut TxContext) {
        let id = TxContext::new_id(ctx);
        let objects = Bag::new(ctx);
        let (id, objects) = Transfer::transfer_to_object_id(objects, id);
        let market_place = Marketplace {
            id,
            objects,
        };
        Transfer::share_object(market_place);
    }

    /// List an item at the Marketplace.
    public fun list<T: key + store, C>(
        _marketplace: &mut Marketplace,
        objects: &mut Bag,
        item: T,
        ask: u64,
        ctx: &mut TxContext
    ) {
        let listing = Listing<T, C> {
            item,
            ask,
            id: TxContext::new_id(ctx),
            owner: TxContext::sender(ctx),
        };
        Bag::add(objects, listing)
    }

    /// Remove listing and get an item back. Only owner can do that.
    public fun delist<T: key + store, C>(
        _marketplace: &mut Marketplace,
        objects: &mut Bag,
        listing: Listing<T, C>,
        ctx: &mut TxContext
    ): T {
        let listing = Bag::remove(objects, listing);
        let Listing { id, item, ask: _, owner } = listing;

        assert!(TxContext::sender(ctx) == owner, ENOT_OWNER);

        ID::delete(id);
        item
    }

    /// Call [`delist`] and transfer item to the sender.
    public fun delist_and_take<T: key + store, C>(
        _marketplace: &mut Marketplace,
        objects: &mut Bag,
        listing: Listing<T, C>,
        ctx: &mut TxContext
    ) {
        Bag::remove_and_take(objects, listing, ctx)
    }

    /// Purchase an item using a known Listing. Payment is done in Coin<C>.
    /// Amount paid must match the requested amount. If conditions are met,
    /// owner of the item gets the payment and buyer receives their item.
    public fun buy<T: key + store, C>(
        _marketplace: &mut Marketplace,
        objects: &mut Bag,
        listing: Listing<T, C>,
        paid: Coin<C>,
    ): T {
        let listing = Bag::remove(objects, listing);
        let Listing { id, item, ask, owner } = listing;

        assert!(ask == Coin::value(&paid), EAMOUNT_INCORRECT);

        Transfer::transfer(paid, owner);
        ID::delete(id);
        item
    }

    /// Call [`buy`] and transfer item to the sender.
    public fun buy_and_take<T: key + store, C>(
        marketplace: &mut Marketplace,
        listing: Listing<T, C>,
        objects: &mut Bag,
        paid: Coin<C>,
        ctx: &mut TxContext
    ) {
        Transfer::transfer(buy(marketplace, objects, listing, paid), TxContext::sender(ctx))
    }

    /// Check whether an object was listed on a Marketplace.
    public fun contains(_marketplace: &Marketplace, objects: &Bag, id: &ID): bool {
        Bag::contains(objects, id)
    }

    /// Returns the size of the Marketplace.
    public fun size(_marketplace: &Marketplace, objects: &Bag): u64 {
        Bag::size(objects)
    }
}

#[test_only]
module NFTs::MarketplaceTests {
    use Sui::Transfer;
    use Sui::NFT::{Self, NFT};
    use Sui::Coin::{Self, Coin};
    use Sui::SUI::SUI;
    use Sui::TestScenario::{Self, Scenario};
    use NFTs::Marketplace::{Self, Marketplace, Listing};

    // Simple KITTY-NFT data structure.
    struct KITTY has store, drop {
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

    /// Mint SUI and send it to BUYER.
    fun mint_some_coin(scenario: &mut Scenario) {
        TestScenario::next_tx(scenario, &ADMIN);
        let coin = Coin::mint_for_testing<SUI>(1000, TestScenario::ctx(scenario));
        Transfer::transfer(coin, BUYER);
    }

    /// Mint KITTY NFT and send it to SELLER.
    fun mint_kitty(scenario: &mut Scenario) {
        TestScenario::next_tx(scenario, &ADMIN);
        let nft = NFT::mint(KITTY { id: 1 }, TestScenario::ctx(scenario));
        NFT::transfer(nft, SELLER);
    }

    // SELLER lists KITTY at the Marketplace for 100 SUI.
    fun list_kitty(scenario: &mut Scenario) {
        TestScenario::next_tx(scenario, &SELLER);
        let mkp = TestScenario::remove_object<Marketplace>(scenario);
        let nft = TestScenario::remove_object<NFT<KITTY>>(scenario);

        Marketplace::list<NFT<KITTY>, SUI>(&mut mkp, nft, 100, TestScenario::ctx(scenario));
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
            let listing = TestScenario::remove_nested_object<Marketplace, Listing<NFT<KITTY>, SUI>>(scenario, &mkp);

            // Do the delist operation on a Marketplace.
            let nft = Marketplace::delist<NFT<KITTY>, SUI>(&mut mkp, listing, TestScenario::ctx(scenario));
            let kitten = NFT::burn<KITTY>(nft);

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

        // BUYER attempts to delist KITTY and he has no right to do so. :(
        TestScenario::next_tx(scenario, &BUYER);
        {
            let mkp = TestScenario::remove_object<Marketplace>(scenario);
            let listing = TestScenario::remove_nested_object<Marketplace, Listing<NFT<KITTY>, SUI>>(scenario, &mkp);

            // Do the delist operation on a Marketplace.
            let nft = Marketplace::delist<NFT<KITTY>, SUI>(&mut mkp, listing, TestScenario::ctx(scenario));
            let _ = NFT::burn<KITTY>(nft);

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

        // BUYER takes 100 SUI from his wallet and purchases KITTY.
        TestScenario::next_tx(scenario, &BUYER);
        {
            let coin = TestScenario::remove_object<Coin<SUI>>(scenario);
            let mkp = TestScenario::remove_object<Marketplace>(scenario);
            let listing = TestScenario::remove_nested_object<Marketplace, Listing<NFT<KITTY>, SUI>>(scenario, &mkp);
            let payment = Coin::withdraw(&mut coin, 100, TestScenario::ctx(scenario));

            // Do the buy call and expect successful purchase.
            let nft = Marketplace::buy<NFT<KITTY>, SUI>(&mut mkp, listing, payment);
            let kitten = NFT::burn<KITTY>(nft);

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

        // BUYER takes 100 SUI from his wallet and purchases KITTY.
        TestScenario::next_tx(scenario, &BUYER);
        {
            let coin = TestScenario::remove_object<Coin<SUI>>(scenario);
            let mkp = TestScenario::remove_object<Marketplace>(scenario);
            let listing = TestScenario::remove_nested_object<Marketplace, Listing<NFT<KITTY>, SUI>>(scenario, &mkp);

            // AMOUNT here is 10 while expected is 100.
            let payment = Coin::withdraw(&mut coin, 10, TestScenario::ctx(scenario));

            // Attempt to buy and expect failure purchase.
            let nft = Marketplace::buy<NFT<KITTY>, SUI>(&mut mkp, listing, payment);
            let _ = NFT::burn<KITTY>(nft);

            TestScenario::return_object(scenario, mkp);
            TestScenario::return_object(scenario, coin);
        };
    }
}
