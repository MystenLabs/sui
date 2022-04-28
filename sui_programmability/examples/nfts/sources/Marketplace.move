// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module NFTs::Marketplace {
    use Sui::Bag::{Self, Bag};
    use Sui::TxContext::{Self, TxContext};
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer::{Self, ChildRef};
    use Sui::Coin::{Self, Coin};

    // For when amount paid does not match the expected.
    const EAmountIncorrect: u64 = 0;

    // For when someone tries to delist without ownership.
    const ENotOwner: u64 = 1;

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
    public(script) fun create(ctx: &mut TxContext) {
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
    public(script) fun list<T: key + store, C>(
        _marketplace: &Marketplace,
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
        _marketplace: &Marketplace,
        objects: &mut Bag,
        listing: Listing<T, C>,
        ctx: &mut TxContext
    ): T {
        let listing = Bag::remove(objects, listing);
        let Listing { id, item, ask: _, owner } = listing;

        assert!(TxContext::sender(ctx) == owner, ENotOwner);

        ID::delete(id);
        item
    }

    /// Call [`delist`] and transfer item to the sender.
    public(script) fun delist_and_take<T: key + store, C>(
        _marketplace: &Marketplace,
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
        objects: &mut Bag,
        listing: Listing<T, C>,
        paid: Coin<C>,
    ): T {
        let listing = Bag::remove(objects, listing);
        let Listing { id, item, ask, owner } = listing;

        assert!(ask == Coin::value(&paid), EAmountIncorrect);

        Transfer::transfer(paid, owner);
        ID::delete(id);
        item
    }

    /// Call [`buy`] and transfer item to the sender.
    public(script) fun buy_and_take<T: key + store, C>(
        _marketplace: &Marketplace,
        listing: Listing<T, C>,
        objects: &mut Bag,
        paid: Coin<C>,
        ctx: &mut TxContext
    ) {
        Transfer::transfer(buy(objects, listing, paid), TxContext::sender(ctx))
    }

    /// Check whether an object was listed on a Marketplace.
    public fun contains(objects: &Bag, id: &ID): bool {
        Bag::contains(objects, id)
    }

    /// Returns the size of the Marketplace.
    public fun size(objects: &Bag): u64 {
        Bag::size(objects)
    }
}

#[test_only]
module NFTs::MarketplaceTests {
    use Sui::ID::{Self, VersionedID};
    use Sui::Bag::Bag;
    use Sui::Transfer;
    use Sui::Coin::{Self, Coin};
    use Sui::SUI::SUI;
    use Sui::TxContext;
    use Sui::TestScenario::{Self, Scenario};
    use NFTs::Marketplace::{Self, Marketplace, Listing};

    use Std::Debug;

    // Simple Kitty-NFT data structure.
    struct Kitty has key, store {
        id: VersionedID,
        kitty_id: u8
    }

    const ADMIN: address = @0xA55;
    const SELLER: address = @0x00A;
    const BUYER: address = @0x00B;

    /// Create a shared [`Marketplace`].
    public(script) fun create_marketplace(scenario: &mut Scenario) {
        TestScenario::next_tx(scenario, &ADMIN);
        Marketplace::create(TestScenario::ctx(scenario));
    }

    /// Mint SUI and send it to BUYER.
    fun mint_some_coin(scenario: &mut Scenario) {
        TestScenario::next_tx(scenario, &ADMIN);
        let coin = Coin::mint_for_testing<SUI>(1000, TestScenario::ctx(scenario));
        Transfer::transfer(coin, BUYER);
    }

    /// Mint Kitty NFT and send it to SELLER.
    fun mint_kitty(scenario: &mut Scenario) {
        TestScenario::next_tx(scenario, &ADMIN);
        let nft = Kitty { id: TxContext::new_id(TestScenario::ctx(scenario)), kitty_id: 1 };
        Transfer::transfer(nft, SELLER);
    }

    // SELLER lists Kitty at the Marketplace for 100 SUI.
    public(script) fun list_kitty(scenario: &mut Scenario) {
        TestScenario::next_tx(scenario, &SELLER);
        let mkp_wrapper = TestScenario::take_shared_object<Marketplace>(scenario);
        let mkp = TestScenario::borrow_mut(&mut mkp_wrapper);
        let bag = TestScenario::take_child_object<Marketplace, Bag>(scenario, mkp);
        let nft = TestScenario::take_object<Kitty>(scenario);

        Marketplace::list<Kitty, SUI>(mkp, &mut bag, nft, 100, TestScenario::ctx(scenario));
        TestScenario::return_shared_object(scenario, mkp_wrapper);
        TestScenario::return_object(scenario, bag);
    }

    #[test]
    public(script) fun list_and_delist() {
        let scenario = &mut TestScenario::begin(&ADMIN);

        create_marketplace(scenario);
        mint_kitty(scenario);
        list_kitty(scenario);

        TestScenario::next_tx(scenario, &SELLER);
        {
            let mkp_wrapper = TestScenario::take_shared_object<Marketplace>(scenario);
            let mkp = TestScenario::borrow_mut(&mut mkp_wrapper);
            let bag = TestScenario::take_child_object<Marketplace, Bag>(scenario, mkp);
            let listing = TestScenario::take_child_object<Bag, Listing<Kitty, SUI>>(scenario, &bag);

            // Do the delist operation on a Marketplace.
            let nft = Marketplace::delist<Kitty, SUI>(mkp, &mut bag, listing, TestScenario::ctx(scenario));
            let kitty_id = burn_kitty(nft);

            assert!(kitty_id == 1, 0);

            TestScenario::return_shared_object(scenario, mkp_wrapper);
            TestScenario::return_object(scenario, bag);
        };
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    public(script) fun fail_to_delist() {
        let scenario = &mut TestScenario::begin(&ADMIN);

        create_marketplace(scenario);
        mint_some_coin(scenario);
        mint_kitty(scenario);
        list_kitty(scenario);

        // BUYER attempts to delist Kitty and he has no right to do so. :(
        TestScenario::next_tx(scenario, &BUYER);
        {
            let mkp_wrapper = TestScenario::take_shared_object<Marketplace>(scenario);
            let mkp = TestScenario::borrow_mut(&mut mkp_wrapper);
            let bag = TestScenario::take_child_object<Marketplace, Bag>(scenario, mkp);
            let listing = TestScenario::take_child_object<Bag, Listing<Kitty, SUI>>(scenario, &bag);

            // Do the delist operation on a Marketplace.
            let nft = Marketplace::delist<Kitty, SUI>(mkp, &mut bag, listing, TestScenario::ctx(scenario));
            let _ = burn_kitty(nft);

            TestScenario::return_shared_object(scenario, mkp_wrapper);
            TestScenario::return_object(scenario, bag);
        };
    }

    #[test]
    public(script) fun buy_kitty() {
        let scenario = &mut TestScenario::begin(&ADMIN);

        Debug::print(&0);
        create_marketplace(scenario);
        Debug::print(&1);
        mint_some_coin(scenario);
        Debug::print(&2);
        mint_kitty(scenario);
        Debug::print(&3);
        list_kitty(scenario);
        Debug::print(&4);

        // BUYER takes 100 SUI from his wallet and purchases Kitty.
        TestScenario::next_tx(scenario, &BUYER);
        {
            let coin = TestScenario::take_object<Coin<SUI>>(scenario);
            let mkp_wrapper = TestScenario::take_shared_object<Marketplace>(scenario);
            let mkp = TestScenario::borrow_mut(&mut mkp_wrapper);
            let bag = TestScenario::take_child_object<Marketplace, Bag>(scenario, mkp);
            let listing = TestScenario::take_child_object<Bag, Listing<Kitty, SUI>>(scenario, &bag);
            let payment = Coin::withdraw(&mut coin, 100, TestScenario::ctx(scenario));

            // Do the buy call and expect successful purchase.
            let nft = Marketplace::buy<Kitty, SUI>(&mut bag, listing, payment);
            let kitty_id = burn_kitty(nft);

            assert!(kitty_id == 1, 0);

            TestScenario::return_shared_object(scenario, mkp_wrapper);
            TestScenario::return_object(scenario, bag);
            TestScenario::return_object(scenario, coin);
        };
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    public(script) fun fail_to_buy() {
        let scenario = &mut TestScenario::begin(&ADMIN);

        create_marketplace(scenario);
        mint_some_coin(scenario);
        mint_kitty(scenario);
        list_kitty(scenario);

        // BUYER takes 100 SUI from his wallet and purchases Kitty.
        TestScenario::next_tx(scenario, &BUYER);
        {
            let coin = TestScenario::take_object<Coin<SUI>>(scenario);
            let mkp_wrapper = TestScenario::take_shared_object<Marketplace>(scenario);
            let mkp = TestScenario::borrow_mut(&mut mkp_wrapper);
            let bag = TestScenario::take_child_object<Marketplace, Bag>(scenario, mkp);
            let listing = TestScenario::take_child_object<Bag, Listing<Kitty, SUI>>(scenario, &bag);

            // AMOUNT here is 10 while expected is 100.
            let payment = Coin::withdraw(&mut coin, 10, TestScenario::ctx(scenario));

            // Attempt to buy and expect failure purchase.
            let nft = Marketplace::buy<Kitty, SUI>(&mut bag, listing, payment);
            let _ = burn_kitty(nft);

            TestScenario::return_shared_object(scenario, mkp_wrapper);
            TestScenario::return_object(scenario, bag);
            TestScenario::return_object(scenario, coin);
        };
    }

    fun burn_kitty(kitty: Kitty): u8 {
        let Kitty{ id, kitty_id } = kitty;
        ID::delete(id);
        kitty_id
    }
}
