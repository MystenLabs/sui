// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Basic `Marketplace` implementation. Supports listing of any assets,
/// and does not have constraints.
///
/// Makes use of `sui::dynamic_object_field` module by attaching `Listing`
/// objects as fields to the `Marketplace` object; as well as stores and
/// merges user profits as dynamic object fields (ofield).
///
/// Rough illustration of the dynamic field architecture for listings:
/// ```
///             /--->Listing--->Item
/// (Marketplace)--->Listing--->Item
///             \--->Listing--->Item
/// ```
///
/// Profits storage is also attached to the `Marketplace` (indexed by `address`):
/// ```
///                   /--->Coin<COIN>
/// (Marketplace<COIN>)--->Coin<COIN>
///                   \--->Coin<COIN>
/// ```
module nfts::marketplace {
    use sui::dynamic_object_field as ofield;
    use sui::tx_context::{Self, TxContext};
    use sui::object::{Self, ID, UID};
    use sui::coin::{Self, Coin};
    use sui::transfer;

    /// For when amount paid does not match the expected.
    const EAmountIncorrect: u64 = 0;
    /// For when someone tries to delist without ownership.
    const ENotOwner: u64 = 1;

    /// A shared `Marketplace`. Can be created by anyone using the
    /// `create` function. One instance of `Marketplace` accepts
    /// only one type of Coin - `COIN` for all its listings.
    struct Marketplace<phantom COIN> has key {
        id: UID,
    }

    /// A single listing which contains the listed item and its
    /// price in [`Coin<COIN>`].
    struct Listing has key, store {
        id: UID,
        ask: u64,
        owner: address,
    }

    /// Create a new shared Marketplace.
    public entry fun create<COIN>(ctx: &mut TxContext) {
        let id = object::new(ctx);
        transfer::share_object(Marketplace<COIN> { id })
    }

    /// List an item at the Marketplace.
    public entry fun list<T: key + store, COIN>(
        marketplace: &mut Marketplace<COIN>,
        item: T,
        ask: u64,
        ctx: &mut TxContext
    ) {
        let item_id = object::id(&item);
        let listing = Listing {
            ask,
            id: object::new(ctx),
            owner: tx_context::sender(ctx),
        };

        ofield::add(&mut listing.id, true, item);
        ofield::add(&mut marketplace.id, item_id, listing)
    }

    /// Remove listing and get an item back. Only owner can do that.
    public fun delist<T: key + store, COIN>(
        marketplace: &mut Marketplace<COIN>,
        item_id: ID,
        ctx: &mut TxContext
    ): T {
        let Listing {
            id,
            owner,
            ask: _,
        } = ofield::remove(&mut marketplace.id, item_id);

        assert!(tx_context::sender(ctx) == owner, ENotOwner);

        let item = ofield::remove(&mut id, true);
        object::delete(id);
        item
    }

    /// Call [`delist`] and transfer item to the sender.
    public entry fun delist_and_take<T: key + store, COIN>(
        marketplace: &mut Marketplace<COIN>,
        item_id: ID,
        ctx: &mut TxContext
    ) {
        let item = delist<T, COIN>(marketplace, item_id, ctx);
        transfer::public_transfer(item, tx_context::sender(ctx));
    }

    /// Purchase an item using a known Listing. Payment is done in Coin<C>.
    /// Amount paid must match the requested amount. If conditions are met,
    /// owner of the item gets the payment and buyer receives their item.
    public fun buy<T: key + store, COIN>(
        marketplace: &mut Marketplace<COIN>,
        item_id: ID,
        paid: Coin<COIN>,
    ): T {
        let Listing {
            id,
            ask,
            owner
        } = ofield::remove(&mut marketplace.id, item_id);

        assert!(ask == coin::value(&paid), EAmountIncorrect);

        // Check if there's already a Coin hanging and merge `paid` with it.
        // Otherwise attach `paid` to the `Marketplace` under owner's `address`.
        if (ofield::exists_<address>(&marketplace.id, owner)) {
            coin::join(
                ofield::borrow_mut<address, Coin<COIN>>(&mut marketplace.id, owner),
                paid
            )
        } else {
            ofield::add(&mut marketplace.id, owner, paid)
        };

        let item = ofield::remove(&mut id, true);
        object::delete(id);
        item
    }

    /// Call [`buy`] and transfer item to the sender.
    public entry fun buy_and_take<T: key + store, COIN>(
        marketplace: &mut Marketplace<COIN>,
        item_id: ID,
        paid: Coin<COIN>,
        ctx: &mut TxContext
    ) {
        transfer::public_transfer(
            buy<T, COIN>(marketplace, item_id, paid),
            tx_context::sender(ctx)
        )
    }

    /// Take profits from selling items on the `Marketplace`.
    public fun take_profits<COIN>(
        marketplace: &mut Marketplace<COIN>,
        ctx: &mut TxContext
    ): Coin<COIN> {
        ofield::remove<address, Coin<COIN>>(&mut marketplace.id, tx_context::sender(ctx))
    }

    /// Call [`take_profits`] and transfer Coin to the sender.
    public entry fun take_profits_and_keep<COIN>(
        marketplace: &mut Marketplace<COIN>,
        ctx: &mut TxContext
    ) {
        transfer::public_transfer(
            take_profits(marketplace, ctx),
            tx_context::sender(ctx)
        )
    }
}

#[test_only]
module nfts::marketplaceTests {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::coin;
    use sui::sui::SUI;
    use sui::test_scenario::{Self, Scenario};
    use nfts::marketplace;

    // Simple Kitty-NFT data structure.
    struct Kitty has key, store {
        id: UID,
        kitty_id: u8
    }

    const ADMIN: address = @0xA55;
    const SELLER: address = @0x00A;
    const BUYER: address = @0x00B;

    /// Create a shared [`Marketplace`].
    fun create_marketplace(scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, ADMIN);
        marketplace::create<SUI>(test_scenario::ctx(scenario));
    }

    /// Mint SUI and send it to BUYER.
    fun mint_some_coin(scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, ADMIN);
        let coin = coin::mint_for_testing<SUI>(1000, test_scenario::ctx(scenario));
        transfer::public_transfer(coin, BUYER);
    }

    /// Mint Kitty NFT and send it to SELLER.
    fun mint_kitty(scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, ADMIN);
        let nft = Kitty { id: object::new(test_scenario::ctx(scenario)), kitty_id: 1 };
        transfer::public_transfer(nft, SELLER);
    }

    // TODO(dyn-child) redo test with dynamic child object loading
    // // SELLER lists Kitty at the Marketplace for 100 SUI.
    // fun list_kitty(scenario: &mut Scenario) {
    //     test_scenario::next_tx(scenario, SELLER);
    //     let mkp_val = test_scenario::take_shared<Marketplace>(scenario);
    //     let mkp = &mut mkp_val;
    //     let bag = test_scenario::take_child_object<Marketplace, Bag>(scenario, mkp);
    //     let nft = test_scenario::take_from_sender<Kitty>(scenario);

    //     marketplace::list<Kitty, SUI>(mkp, &mut bag, nft, 100, test_scenario::ctx(scenario));
    //     test_scenario::return_shared(mkp_val);
    //     test_scenario::return_to_sender(scenario, bag);
    // }

    // TODO(dyn-child) redo test with dynamic child object loading
    // #[test]
    // fun list_and_delist() {
    //     let scenario = &mut test_scenario::begin(ADMIN);

    //     create_marketplace(scenario);
    //     mint_kitty(scenario);
    //     list_kitty(scenario);

    //     test_scenario::next_tx(scenario, SELLER);
    //     {
    //         let mkp_val = test_scenario::take_shared<Marketplace>(scenario);
    //         let mkp = &mut mkp_val;
    //         let bag = test_scenario::take_child_object<Marketplace, Bag>(scenario, mkp);
    //         let listing = test_scenario::take_child_object<Bag, bag::Item<Listing<Kitty, SUI>>>(scenario, &bag);

    //         // Do the delist operation on a Marketplace.
    //         let nft = marketplace::delist<Kitty, SUI>(mkp, &mut bag, listing, test_scenario::ctx(scenario));
    //         let kitty_id = burn_kitty(nft);

    //         assert!(kitty_id == 1, 0);

    //         test_scenario::return_shared(mkp_val);
    //         test_scenario::return_to_sender(scenario, bag);
    //     };
    // }

    // TODO(dyn-child) redo test with dynamic child object loading
    // #[test]
    // #[expected_failure(abort_code = 1)]
    // fun fail_to_delist() {
    //     let scenario = &mut test_scenario::begin(ADMIN);

    //     create_marketplace(scenario);
    //     mint_some_coin(scenario);
    //     mint_kitty(scenario);
    //     list_kitty(scenario);

    //     // BUYER attempts to delist Kitty and he has no right to do so. :(
    //     test_scenario::next_tx(scenario, BUYER);
    //     {
    //         let mkp_val = test_scenario::take_shared<Marketplace>(scenario);
    //         let mkp = &mut mkp_val;
    //         let bag = test_scenario::take_child_object<Marketplace, Bag>(scenario, mkp);
    //         let listing = test_scenario::take_child_object<Bag, bag::Item<Listing<Kitty, SUI>>>(scenario, &bag);

    //         // Do the delist operation on a Marketplace.
    //         let nft = marketplace::delist<Kitty, SUI>(mkp, &mut bag, listing, test_scenario::ctx(scenario));
    //         let _ = burn_kitty(nft);

    //         test_scenario::return_shared(mkp_val);
    //         test_scenario::return_to_sender(scenario, bag);
    //     };
    // }

    // TODO(dyn-child) redo test with dynamic child object loading
    // #[test]
    // fun buy_kitty() {
    //     let scenario = &mut test_scenario::begin(ADMIN);

    //     create_marketplace(scenario);
    //     mint_some_coin(scenario);
    //     mint_kitty(scenario);
    //     list_kitty(scenario);

    //     // BUYER takes 100 SUI from his wallet and purchases Kitty.
    //     test_scenario::next_tx(scenario, BUYER);
    //     {
    //         let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
    //         let mkp_val = test_scenario::take_shared<Marketplace>(scenario);
    //         let mkp = &mut mkp_val;
    //         let bag = test_scenario::take_child_object<Marketplace, Bag>(scenario, mkp);
    //         let listing = test_scenario::take_child_object<Bag, bag::Item<Listing<Kitty, SUI>>>(scenario, &bag);
    //         let payment = coin::take(coin::balance_mut(&mut coin), 100, test_scenario::ctx(scenario));

    //         // Do the buy call and expect successful purchase.
    //         let nft = marketplace::buy<Kitty, SUI>(&mut bag, listing, payment);
    //         let kitty_id = burn_kitty(nft);

    //         assert!(kitty_id == 1, 0);

    //         test_scenario::return_shared(mkp_val);
    //         test_scenario::return_to_sender(scenario, bag);
    //         test_scenario::return_to_sender(scenario, coin);
    //     };
    // }

    // TODO(dyn-child) redo test with dynamic child object loading
    // #[test]
    // #[expected_failure(abort_code = 0)]
    // fun fail_to_buy() {
    //     let scenario = &mut test_scenario::begin(ADMIN);

    //     create_marketplace(scenario);
    //     mint_some_coin(scenario);
    //     mint_kitty(scenario);
    //     list_kitty(scenario);

    //     // BUYER takes 100 SUI from his wallet and purchases Kitty.
    //     test_scenario::next_tx(scenario, BUYER);
    //     {
    //         let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
    //         let mkp_val = test_scenario::take_shared<Marketplace>(scenario);
    //         let mkp = &mut mkp_val;
    //         let bag = test_scenario::take_child_object<Marketplace, Bag>(scenario, mkp);
    //         let listing = test_scenario::take_child_object<Bag, bag::Item<Listing<Kitty, SUI>>>(scenario, &bag);

    //         // AMOUNT here is 10 while expected is 100.
    //         let payment = coin::take(coin::balance_mut(&mut coin), 10, test_scenario::ctx(scenario));

    //         // Attempt to buy and expect failure purchase.
    //         let nft = marketplace::buy<Kitty, SUI>(&mut bag, listing, payment);
    //         let _ = burn_kitty(nft);

    //         test_scenario::return_shared(mkp_val);
    //         test_scenario::return_to_sender(scenario, bag);
    //         test_scenario::return_to_sender(scenario, coin);
    //     };
    // }

    fun burn_kitty(kitty: Kitty): u8 {
        let Kitty{ id, kitty_id } = kitty;
        object::delete(id);
        kitty_id
    }
}
