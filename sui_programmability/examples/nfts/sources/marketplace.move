// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module nfts::marketplace {
    use sui::bag::{Self, Bag};
    use sui::tx_context::{Self, TxContext};
    use sui::id::{Self, ID, VersionedID};
    use sui::transfer::{Self, ChildRef};
    use sui::coin::{Self, Coin};

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
    public entry fun create(ctx: &mut TxContext) {
        let id = tx_context::new_id(ctx);
        let objects = bag::new(ctx);
        let (id, objects) = bag::transfer_to_object_id(objects, id);
        let market_place = Marketplace {
            id,
            objects,
        };
        transfer::share_object(market_place);
    }

    /// List an item at the Marketplace.
    public entry fun list<T: key + store, C>(
        _marketplace: &Marketplace,
        objects: &mut Bag,
        item: T,
        ask: u64,
        ctx: &mut TxContext
    ) {
        let listing = Listing<T, C> {
            item,
            ask,
            id: tx_context::new_id(ctx),
            owner: tx_context::sender(ctx),
        };
        bag::add(objects, listing)
    }

    /// Remove listing and get an item back. Only owner can do that.
    public fun delist<T: key + store, C>(
        _marketplace: &Marketplace,
        objects: &mut Bag,
        listing: Listing<T, C>,
        ctx: &mut TxContext
    ): T {
        let listing = bag::remove(objects, listing);
        let Listing { id, item, ask: _, owner } = listing;

        assert!(tx_context::sender(ctx) == owner, ENotOwner);

        id::delete(id);
        item
    }

    /// Call [`delist`] and transfer item to the sender.
    public entry fun delist_and_take<T: key + store, C>(
        _marketplace: &Marketplace,
        objects: &mut Bag,
        listing: Listing<T, C>,
        ctx: &mut TxContext
    ) {
        bag::remove_and_take(objects, listing, ctx)
    }

    /// Purchase an item using a known Listing. Payment is done in Coin<C>.
    /// Amount paid must match the requested amount. If conditions are met,
    /// owner of the item gets the payment and buyer receives their item.
    public fun buy<T: key + store, C>(
        objects: &mut Bag,
        listing: Listing<T, C>,
        paid: Coin<C>,
    ): T {
        let listing = bag::remove(objects, listing);
        let Listing { id, item, ask, owner } = listing;

        assert!(ask == coin::value(&paid), EAmountIncorrect);

        transfer::transfer(paid, owner);
        id::delete(id);
        item
    }

    /// Call [`buy`] and transfer item to the sender.
    public entry fun buy_and_take<T: key + store, C>(
        _marketplace: &Marketplace,
        listing: Listing<T, C>,
        objects: &mut Bag,
        paid: Coin<C>,
        ctx: &mut TxContext
    ) {
        transfer::transfer(buy(objects, listing, paid), tx_context::sender(ctx))
    }

    /// Check whether an object was listed on a Marketplace.
    public fun contains(objects: &Bag, id: &ID): bool {
        bag::contains(objects, id)
    }

    /// Returns the size of the Marketplace.
    public fun size(objects: &Bag): u64 {
        bag::size(objects)
    }
}

#[test_only]
module nfts::marketplaceTests {
    use sui::id::{Self, VersionedID};
    use sui::bag::Bag;
    use sui::transfer;
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::tx_context;
    use sui::test_scenario::{Self, Scenario};
    use nfts::marketplace::{Self, Marketplace, Listing};

    // Simple Kitty-NFT data structure.
    struct Kitty has key, store {
        id: VersionedID,
        kitty_id: u8
    }

    const ADMIN: address = @0xA55;
    const SELLER: address = @0x00A;
    const BUYER: address = @0x00B;

    /// Create a shared [`Marketplace`].
    fun create_marketplace(scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, &ADMIN);
        marketplace::create(test_scenario::ctx(scenario));
    }

    /// Mint SUI and send it to BUYER.
    fun mint_some_coin(scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, &ADMIN);
        let coin = coin::mint_for_testing<SUI>(1000, test_scenario::ctx(scenario));
        transfer::transfer(coin, BUYER);
    }

    /// Mint Kitty NFT and send it to SELLER.
    fun mint_kitty(scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, &ADMIN);
        let nft = Kitty { id: tx_context::new_id(test_scenario::ctx(scenario)), kitty_id: 1 };
        transfer::transfer(nft, SELLER);
    }

    // SELLER lists Kitty at the Marketplace for 100 SUI.
    fun list_kitty(scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, &SELLER);
        let mkp_wrapper = test_scenario::take_shared<Marketplace>(scenario);
        let mkp = test_scenario::borrow_mut(&mut mkp_wrapper);
        let bag = test_scenario::take_child_object<Marketplace, Bag>(scenario, mkp);
        let nft = test_scenario::take_owned<Kitty>(scenario);

        marketplace::list<Kitty, SUI>(mkp, &mut bag, nft, 100, test_scenario::ctx(scenario));
        test_scenario::return_shared(scenario, mkp_wrapper);
        test_scenario::return_owned(scenario, bag);
    }

    #[test]
    fun list_and_delist() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        create_marketplace(scenario);
        mint_kitty(scenario);
        list_kitty(scenario);

        test_scenario::next_tx(scenario, &SELLER);
        {
            let mkp_wrapper = test_scenario::take_shared<Marketplace>(scenario);
            let mkp = test_scenario::borrow_mut(&mut mkp_wrapper);
            let bag = test_scenario::take_child_object<Marketplace, Bag>(scenario, mkp);
            let listing = test_scenario::take_child_object<Bag, Listing<Kitty, SUI>>(scenario, &bag);

            // Do the delist operation on a Marketplace.
            let nft = marketplace::delist<Kitty, SUI>(mkp, &mut bag, listing, test_scenario::ctx(scenario));
            let kitty_id = burn_kitty(nft);

            assert!(kitty_id == 1, 0);

            test_scenario::return_shared(scenario, mkp_wrapper);
            test_scenario::return_owned(scenario, bag);
        };
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    fun fail_to_delist() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        create_marketplace(scenario);
        mint_some_coin(scenario);
        mint_kitty(scenario);
        list_kitty(scenario);

        // BUYER attempts to delist Kitty and he has no right to do so. :(
        test_scenario::next_tx(scenario, &BUYER);
        {
            let mkp_wrapper = test_scenario::take_shared<Marketplace>(scenario);
            let mkp = test_scenario::borrow_mut(&mut mkp_wrapper);
            let bag = test_scenario::take_child_object<Marketplace, Bag>(scenario, mkp);
            let listing = test_scenario::take_child_object<Bag, Listing<Kitty, SUI>>(scenario, &bag);

            // Do the delist operation on a Marketplace.
            let nft = marketplace::delist<Kitty, SUI>(mkp, &mut bag, listing, test_scenario::ctx(scenario));
            let _ = burn_kitty(nft);

            test_scenario::return_shared(scenario, mkp_wrapper);
            test_scenario::return_owned(scenario, bag);
        };
    }

    #[test]
    fun buy_kitty() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        create_marketplace(scenario);
        mint_some_coin(scenario);
        mint_kitty(scenario);
        list_kitty(scenario);

        // BUYER takes 100 SUI from his wallet and purchases Kitty.
        test_scenario::next_tx(scenario, &BUYER);
        {
            let coin = test_scenario::take_owned<Coin<SUI>>(scenario);
            let mkp_wrapper = test_scenario::take_shared<Marketplace>(scenario);
            let mkp = test_scenario::borrow_mut(&mut mkp_wrapper);
            let bag = test_scenario::take_child_object<Marketplace, Bag>(scenario, mkp);
            let listing = test_scenario::take_child_object<Bag, Listing<Kitty, SUI>>(scenario, &bag);
            let payment = coin::take(coin::balance_mut(&mut coin), 100, test_scenario::ctx(scenario));

            // Do the buy call and expect successful purchase.
            let nft = marketplace::buy<Kitty, SUI>(&mut bag, listing, payment);
            let kitty_id = burn_kitty(nft);

            assert!(kitty_id == 1, 0);

            test_scenario::return_shared(scenario, mkp_wrapper);
            test_scenario::return_owned(scenario, bag);
            test_scenario::return_owned(scenario, coin);
        };
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun fail_to_buy() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        create_marketplace(scenario);
        mint_some_coin(scenario);
        mint_kitty(scenario);
        list_kitty(scenario);

        // BUYER takes 100 SUI from his wallet and purchases Kitty.
        test_scenario::next_tx(scenario, &BUYER);
        {
            let coin = test_scenario::take_owned<Coin<SUI>>(scenario);
            let mkp_wrapper = test_scenario::take_shared<Marketplace>(scenario);
            let mkp = test_scenario::borrow_mut(&mut mkp_wrapper);
            let bag = test_scenario::take_child_object<Marketplace, Bag>(scenario, mkp);
            let listing = test_scenario::take_child_object<Bag, Listing<Kitty, SUI>>(scenario, &bag);

            // AMOUNT here is 10 while expected is 100.
            let payment = coin::take(coin::balance_mut(&mut coin), 10, test_scenario::ctx(scenario));

            // Attempt to buy and expect failure purchase.
            let nft = marketplace::buy<Kitty, SUI>(&mut bag, listing, payment);
            let _ = burn_kitty(nft);

            test_scenario::return_shared(scenario, mkp_wrapper);
            test_scenario::return_owned(scenario, bag);
            test_scenario::return_owned(scenario, coin);
        };
    }

    fun burn_kitty(kitty: Kitty): u8 {
        let Kitty{ id, kitty_id } = kitty;
        id::delete(id);
        kitty_id
    }
}
