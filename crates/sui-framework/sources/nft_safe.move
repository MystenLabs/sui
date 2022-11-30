// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Safe for collectibles.
///
/// - listing functionality
/// - paying royalties to creators
/// - no restrictions on transfers or taking / pulling an asset
module sui::safe {
    use sui::object::{Self, ID, UID};
    use std::option::{Self, Option};
    use sui::tx_context::{Self, TxContext};
    use sui::dynamic_object_field as dof;
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    // use sui::dynamic_field as df;

    /// Fee in basis points to pay to the creator.
    const FEE: u64 = 100;

    /// Safe is abstraction layer which separates and protects
    /// collectibles or other types of assets which require royalties
    /// or overall transfer safety.
    struct Safe has key, store {
        id: UID,
        owner: Option<address>,
        whitelist: Option<vector<address>>,
    }

    /// Free action of creating a new Safe. Owner and whitelisted (up to X)
    /// safes can be specified to allow free transfers between them.
    public fun create_safe(ctx: &mut TxContext): Safe {
        Safe {
            id: object::new(ctx),
            owner: option::some(tx_context::sender(ctx)),
            whitelist: option::none()
        }
    }

    /// Type linker. Which witness type matches royalty type.
    // struct TypeLink<phantom R> has copy, store, drop {}

    /// Add an Object to the safe effectively locking it from the outer world.
    ///
    /// When an Object is first added to the Safe, a witness parameter `S` is locked,
    /// and will travel along the `T`, marking the proper `RoyaltyReceipt<S>` for `T`.
    public fun put<T: key + store>(self: &mut Safe, item: T) {
        // df::add(&mut self.id, TypeLink<T> {}, TypeLink<R> {});
        dof::add(&mut self.id, object::id(&item), item)
        // abort 0
    }

    struct Listing has copy, store, drop { price: u64, item_id: ID }

    /// List an item for sale in a safe.
    public fun list<T: key + store>(self: &mut Safe, item_id: ID, price: u64) {
        let item = dof::remove<ID, T>(&mut self.id, item_id);
        dof::add(&mut self.id, Listing { price, item_id }, item)
    }

    /// Purchase a listed item from a Safe by an item ID.
    public fun purchase<T: key + store>(
        self: &mut Safe, target: &mut Safe, item_id: ID, payment: Coin<SUI>, _ctx: &mut TxContext
    ) {
        let price = coin::value(&payment);
        let item = dof::remove<Listing, T>(&mut self.id, Listing { price, item_id });

        put(target, item);

        // we need to do something with the payment
        sui::transfer::transfer(payment, sui::tx_context::sender(_ctx))
    }

    /// Take an item from the Safe freeing it from the safe.
    public fun take<T: key + store>(self: &mut Safe, item_id: ID): T {
        dof::remove(&mut self.id, item_id)
    }

    /// Borrow an Object from the safe allowing read access. If additional constraints
    /// are needed, Safe can be wrapped into an access-control wrapper.
    public fun borrow<T: key + store>(self: &mut Safe, item_id: ID): &T {
        dof::borrow(&self.id, item_id)
    }

    /// Mutably borrow an Object from the safe allowing modifications. Access control can
    /// be enforced on the higher level if needed.
    public fun borrow_mut<T: key + store>(self: &mut Safe, item_id: ID): &mut T {
        dof::borrow_mut(&mut self.id, item_id)
    }

    // Listing / Purchases / Royalty

    // In case there's a need to borrow full value for the transaction.
    // If safe is not restricted, this function becomes a very fancy way of atomic swaps.
    // Take it but with a Promise to put back. :wink:

    struct Promise { expects: ID /* , safe: ID */ }

    public fun take_with_promise<T: key + store>(self: &mut Safe, item_id: ID): (T, Promise) {
        (dof::remove(&mut self.id, *&item_id), Promise { expects: item_id })
    }

    public fun return_promise<T: key + store>(self: &mut Safe, item: T, promise: Promise) {
        let Promise { expects } = promise;
        assert!(object::id(&item) == expects, 0);
        dof::add(&mut self.id, object::id(&item), item)
    }

    /// A very fancy way to prove that object was destroyed within the current transaction.
    /// This way we ensure that the Object was unpacked. Yay!
    ///
    /// We can consider taking the responsibility of deleting the UID; most of the cleanups
    /// and dynamic objects can be managed prior to this call (eg nothing is stopping us from it)
    public fun prove_destruction(id: UID, promise: Promise) {
        let Promise { expects } = promise;
        assert!(object::uid_to_inner(&id) == expects, 0);
        object::delete(id)
    }
}

#[test_only]
module sui::safe_tests {
    use sui::test_scenario::{Self as ts};
    use sui::safe;

    fun people(): (address) { (@0x1) }

    #[test]
    public fun test() {
        let (p1) = people();
        let test = ts::begin(&p1);

        // ...

        ts::end(test);
    }
}
