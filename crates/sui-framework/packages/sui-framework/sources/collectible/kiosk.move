// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Kiosk is a primitive for building open, zero-fee trading platforms
/// for assets with a high degree of customization over transfer
/// policies.
///
/// The system has 3 main audiences:
///
/// 1. Creators: for a type to be tradable in the Kiosk ecosystem,
/// creator (publisher) of the type needs to issue a `TransferPolicyCap`
/// which gives them a power to enforce any constraint on trades by
/// either using one of the pre-built primitives (see `sui::royalty`)
/// or by implementing a custom policy. The latter requires additional
/// support for discoverability in the ecosystem and should be performed
/// within the scope of an Application or some platform.
///
/// - A type can not be traded in the Kiosk unless there's a policy for it.
/// - 0-royalty policy is just as easy as "freezing" the `AllowTransferCap`
///   making it available for everyone to authorize deals "for free"
///
/// 2. Traders: anyone can create a Kiosk and depending on whether it's
/// a shared object or some shared-wrapper the owner can trade any type
/// that has issued `TransferPolicyCap` in a Kiosk. To do so, they need
/// to make an offer, and any party can purchase the item for the amount of
/// SUI set in the offer. The responsibility to follow the transfer policy
/// set by the creator of the `T` is on the buyer.
///
/// 3. Marketplaces: marketplaces can either watch for the offers made in
/// personal Kiosks or even integrate the Kiosk primitive and build on top
/// of it. In the custom logic scenario, the `TransferPolicyCap` can also
/// be used to implement application-specific transfer rules.
///
module sui::kiosk {
    use std::option::{Self, Option};
    use sui::object::{Self, UID, ID};
    use sui::dynamic_object_field as dof;
    use sui::dynamic_field as df;
    use sui::transfer_policy::{Self, TransferRequest};
    use sui::tx_context::{TxContext, sender};
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::event;

    // Collectible is a special case to avoid storing `Publisher`.
    friend sui::collectible;

    /// Trying to withdraw profits and sender is not owner.
    const ENotOwner: u64 = 0;
    /// Coin paid does not match the offer price.
    const EIncorrectAmount: u64 = 1;
    /// Trying to withdraw higher amount than stored.
    const ENotEnough: u64 = 2;
    /// Trying to close a Kiosk and it has items in it.
    const ENotEmpty: u64 = 3;
    /// Attempt to take an item that has a `PurchaseCap` issued.
    const EListedExclusively: u64 = 4;
    /// `PurchaseCap` does not match the `Kiosk`.
    const EWrongKiosk: u64 = 5;
    /// Tryng to exclusively list an already listed item.
    const EAlreadyListed: u64 = 6;
    /// Trying to call `uid_mut` when extensions disabled
    const EExtensionsDisabled: u64 = 7;

    /// An object which allows selling collectibles within "kiosk" ecosystem.
    /// By default gives the functionality to list an item openly - for anyone
    /// to purchase providing the guarantees for creators that every transfer
    /// needs to be approved via the `TransferPolicy`.
    struct Kiosk has key, store {
        id: UID,
        /// Balance of the Kiosk - all profits from sales go here.
        profits: Balance<SUI>,
        /// Always point to `sender` of the transaction.
        /// Can be changed by calling `set_owner` with Cap.
        owner: address,
        /// Number of items stored in a Kiosk. Used to allow unpacking
        /// an empty Kiosk if it was wrapped or has a single owner.
        item_count: u32,
        /// Whether to open the UID to public. Set to `true` by default
        /// but the owner can switch the state if necessary.
        allow_extensions: bool
    }

    /// A Capability granting the bearer a right to `place` and `take` items
    /// from the `Kiosk` as well as to `list` them and `list_with_purchase_cap`.
    struct KioskOwnerCap has key, store {
        id: UID,
        for: ID
    }

    /// A capability which locks an item and gives a permission to
    /// purchase it from a `Kiosk` for any price no less than `min_price`.
    ///
    /// Allows exclusive listing: only bearer of the `PurchaseCap` can
    /// purchase the asset. However, the capablity should be used
    /// carefully as losing it would lock the asset in the `Kiosk`.
    ///
    /// The main application for the `PurchaseCap` is building extensions
    /// on top of the `Kiosk`.
    struct PurchaseCap<phantom T: key + store> has key, store {
        id: UID,
        /// ID of the `Kiosk` the cap belongs to.
        kiosk_id: ID,
        /// ID of the listed item.
        item_id: ID,
        /// Minimum price for which the item can be purchased.
        min_price: u64
    }

    // === Dynamic Field keys ===

    /// Dynamic field key for an item placed into the kiosk.
    struct Item has store, copy, drop { id: ID }

    /// Dynamic field key for an active offer to purchase the T. If an
    /// item is listed without a `PurchaseCap`, exclusive is set to `false`.
    struct Listing has store, copy, drop { id: ID, is_exclusive: bool }

    // === Events ===

    /// Emitted when an item was listed by the safe owner. Can be used
    /// to track available offers anywhere on the network; the event is
    /// type-indexed which allows for searching for offers of a specific `T`
    struct ItemListed<phantom T: key + store> has copy, drop {
        kiosk: ID,
        id: ID,
        price: u64
    }

    // === Kiosk packing and unpacking ===

    /// Creates a new `Kiosk` with a matching `KioskOwnerCap`.
    public fun new(ctx: &mut TxContext): (Kiosk, KioskOwnerCap) {
        let kiosk = Kiosk {
            id: object::new(ctx),
            profits: balance::zero(),
            owner: sender(ctx),
            item_count: 0,
            allow_extensions: true
        };

        let cap = KioskOwnerCap {
            id: object::new(ctx),
            for: object::id(&kiosk)
        };

        (kiosk, cap)
    }

    /// Unpacks and destroys a Kiosk returning the profits (even if "0").
    /// Can only be performed by the bearer of the `KioskOwnerCap` in the
    /// case where there's no items inside and a `Kiosk` is not shared.
    public fun close_and_withdraw(
        self: Kiosk, cap: KioskOwnerCap, ctx: &mut TxContext
    ): Coin<SUI> {
        let Kiosk { id, profits, owner: _, item_count, allow_extensions: _ } = self;
        let KioskOwnerCap { id: cap_id, for } = cap;

        assert!(object::uid_to_inner(&id) == for, ENotOwner);
        assert!(item_count == 0, ENotEmpty);

        object::delete(cap_id);
        object::delete(id);

        coin::from_balance(profits, ctx)
    }

    /// Change the `owner` field to the transaction sender.
    /// The change is purely cosmetical and does not affect any of the
    /// basic kiosk functions unless some logic for this is implemented
    /// in a third party module.
    public fun set_owner(
        self: &mut Kiosk, cap: &KioskOwnerCap, ctx: &TxContext
    ) {
        assert!(object::id(self) == cap.for, ENotOwner);
        self.owner = sender(ctx);
    }

    // === Place and take from the Kiosk ===

    /// Place any object into a Kiosk.
    /// Performs an authorization check to make sure only owner can do that.
    public fun place<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, item: T
    ) {
        assert!(object::id(self) == cap.for, ENotOwner);
        self.item_count = self.item_count + 1;
        dof::add(&mut self.id, Item { id: object::id(&item) }, item)
    }

    /// Take any object from the Kiosk.
    /// Performs an authorization check to make sure only owner can do that.
    public fun take<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, id: ID
    ): T {
        assert!(object::id(self) == cap.for, ENotOwner);
        assert!(!df::exists_<Listing>(&mut self.id, Listing { id, is_exclusive: true }), EListedExclusively);

        self.item_count = self.item_count - 1;
        df::remove_if_exists<Listing, u64>(&mut self.id, Listing { id, is_exclusive: false });
        dof::remove(&mut self.id, Item { id })
    }

    // === Trading functionality: List and Purchase ===

    /// List the item by setting a price and making it available for purchase.
    /// Performs an authorization check to make sure only owner can sell.
    public fun list<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, id: ID, price: u64
    ) {
        assert!(object::id(self) == cap.for, ENotOwner);
        assert!(!df::exists_<Listing>(&mut self.id, Listing { id, is_exclusive: true }), EListedExclusively);

        df::add(&mut self.id, Listing { id, is_exclusive: false }, price);
        event::emit(ItemListed<T> {
            kiosk: object::id(self), id, price
        })
    }

    /// Calls `place` and `list` together - simplifies the flow.
    public fun place_and_list<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, item: T, price: u64
    ) {
        let id = object::id(&item);
        place(self, cap, item);
        list<T>(self, cap, id, price)
    }

    /// Make a trade: pay the owner of the item and request a Transfer to the `target`
    /// kiosk (to prevent item being taken by the approving party).
    ///
    /// Received `TransferRequest` needs to be handled by the publisher of the T,
    /// if they have a method implemented that allows a trade, it is possible to
    /// request their approval (by calling some function) so that the trade can be
    /// finalized.
    public fun purchase<T: key + store>(
        self: &mut Kiosk, id: ID, payment: Coin<SUI>, ctx: &mut TxContext
    ): (T, TransferRequest<T>) {
        let price = df::remove<Listing, u64>(&mut self.id, Listing { id, is_exclusive: false });
        let inner = dof::remove<Item, T>(&mut self.id, Item { id });

        self.item_count = self.item_count - 1;
        assert!(price == coin::value(&payment), EIncorrectAmount);
        balance::join(&mut self.profits, coin::into_balance(payment));

        (inner, transfer_policy::new_request(price, object::id(self), ctx))
    }

    // === Trading Functionality: Exclusive listing with `PurchaseCap` ===

    /// Creates a `PurchaseCap` which gives the right to purchase an item
    /// for any price equal or higher than the `min_price`.
    public fun list_with_purchase_cap<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, id: ID, min_price: u64, ctx: &mut TxContext
    ): PurchaseCap<T> {
        assert!(object::id(self) == cap.for, ENotOwner);
        assert!(!df::exists_<Listing>(&mut self.id, Listing { id, is_exclusive: false }), EAlreadyListed);

        let uid = object::new(ctx);
        df::add(&mut self.id, Listing { id, is_exclusive: true }, min_price);

        PurchaseCap<T> {
            id: uid,
            item_id: id,
            kiosk_id: cap.for,
            min_price,
        }
    }

    /// Unpack the `PurchaseCap` and call `purchase`. Sets the payment amount
    /// as the price for the listing making sure it's no less than `min_amount`.
    public fun purchase_with_cap<T: key + store>(
        self: &mut Kiosk, purchase_cap: PurchaseCap<T>, payment: Coin<SUI>, ctx: &mut TxContext
    ): (T, TransferRequest<T>) {
        let PurchaseCap { id, item_id, kiosk_id, min_price } = purchase_cap;
        let paid = coin::value(&payment);

        assert!(paid >= min_price, EIncorrectAmount);
        assert!(object::id(self) == kiosk_id, EWrongKiosk);

        df::remove<Listing, u64>(&mut self.id, Listing { id: item_id, is_exclusive: true });
        df::add(&mut self.id, Listing { id: item_id, is_exclusive: false }, paid);
        object::delete(id);

        purchase<T>(self, item_id, payment, ctx)
    }

    /// Return the `PurchaseCap` without making a purchase; remove an active offer and
    /// allow the item for taking. Can only be returned to its `Kiosk`, aborts otherwise.
    public fun return_purchase_cap<T: key + store>(
        self: &mut Kiosk, purchase_cap: PurchaseCap<T>
    ) {
        let PurchaseCap { id, item_id, kiosk_id, min_price: _ } = purchase_cap;

        assert!(object::id(self) == kiosk_id, EWrongKiosk);
        df::remove<Listing, u64>(&mut self.id, Listing { id: item_id, is_exclusive: true });
        object::delete(id)
    }

    /// Withdraw profits from the Kiosk.
    public fun withdraw(
        self: &mut Kiosk, cap: &KioskOwnerCap, amount: Option<u64>, ctx: &mut TxContext
    ): Coin<SUI> {
        assert!(object::id(self) == cap.for, ENotOwner);

        let amount = if (option::is_some(&amount)) {
            let amt = option::destroy_some(amount);
            assert!(amt <= balance::value(&self.profits), ENotEnough);
            amt
        } else {
            balance::value(&self.profits)
        };

        coin::take(&mut self.profits, amount, ctx)
    }

    // === Kiosk fields access ===

    /// Check whether the `KioskOwnerCap` matches the `Kiosk`.
    public fun has_access(self: &mut Kiosk, cap: &KioskOwnerCap): bool {
        object::id(self) == cap.for
    }

    /// Access the `UID` using the `KioskOwnerCap`.
    public fun uid_mut_as_owner(self: &mut Kiosk, cap: &KioskOwnerCap): &mut UID {
        assert!(object::id(self) == cap.for, ENotOwner);
        &mut self.id
    }

    /// Get the mutable `UID` for dynamic field access and extensions.
    /// Aborts if `allow_extensions` set to `false`.
    public fun uid_mut(self: &mut Kiosk): &mut UID {
        assert!(self.allow_extensions, EExtensionsDisabled);
        &mut self.id
    }

    /// Get the owner of the Kiosk.
    public fun owner(self: &Kiosk): address {
        self.owner
    }

    /// Get the number of items stored in a Kiosk.
    public fun item_count(self: &Kiosk): u32 {
        self.item_count
    }

    /// Get the amount of profits collected by selling items.
    public fun profits_amount(self: &Kiosk): u64 {
        balance::value(&self.profits)
    }

    /// Get mutable access to `profits` - useful for extendability.
    public fun profits_mut(self: &mut Kiosk, cap: &KioskOwnerCap): &mut Balance<SUI> {
        assert!(object::id(self) == cap.for, ENotOwner);
        &mut self.profits
    }

    // === PurchaseCap fields access ===

    /// Get the `kiosk_id` from the `PurchaseCap`.
    public fun purchase_cap_kiosk<T: key + store>(self: &PurchaseCap<T>): ID {
        self.kiosk_id
    }

    /// Get the `Item_id` from the `PurchaseCap`.
    public fun purchase_cap_item<T: key + store>(self: &PurchaseCap<T>): ID {
        self.item_id
    }

    /// Get the `min_price` from the `PurchaseCap`.
    public fun purchase_cap_min_price<T: key + store>(self: &PurchaseCap<T>): u64 {
        self.min_price
    }
}

#[test_only]
/// Kiosk testing strategy:
/// - [ ] test purchase flow
/// - [ ] test purchase cap flow
/// - [ ] test withdraw methods
module sui::kiosk_tests {
    use std::vector;
    use sui::tx_context::{Self, TxContext};
    use sui::transfer_policy::{Self as policy, TransferPolicy, TransferPolicyCap};
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::object::{Self, ID, UID};
    use sui::sui::SUI;
    use sui::package;
    use sui::coin;

    const AMT: u64 = 10_000;

    struct Asset has key, store { id: UID }
    struct OTW has drop {}

    /// Prepare: accounts
    /// Alice, Bob and my favorite guy - Carl
    fun folks(): (address, address, address) { (@0xA11CE, @0xB0B, @0xCA51) }

    /// Prepare: TransferPolicy<Asset>
    fun get_policy(ctx: &mut TxContext): (TransferPolicy<Asset>, TransferPolicyCap<Asset>) {
        let publisher = package::test_claim(OTW {}, ctx);
        let (policy, cap) = policy::new(&publisher, ctx);
        package::burn_publisher(publisher);
        (policy, cap)
    }

    /// Prepare: Asset
    fun get_asset(ctx: &mut TxContext): (Asset, ID) {
        let uid = object::new(ctx);
        let id = object::uid_to_inner(&uid);
        (Asset { id: uid }, id)
    }

    #[test]
    fun test_place_and_take() {
        let ctx = &mut tx_context::dummy();
        let (policy, policy_cap) = get_policy(ctx);
        let (asset, item_id) = get_asset(ctx);
        let (kiosk, owner_cap) = kiosk::new(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        let asset = kiosk::take(&mut kiosk, &owner_cap, item_id);

        return_kiosk(kiosk, owner_cap, ctx);
        return_assets(vector[ asset ]);
        return_policy(policy, policy_cap, ctx);
    }

    #[test]
    fun test_purchase() {
        let ctx = &mut tx_context::dummy();
        let (policy, policy_cap) = get_policy(ctx);
        let (asset, item_id) = get_asset(ctx);
        let (kiosk, owner_cap) = kiosk::new(ctx);

        kiosk::place_and_list(&mut kiosk, &owner_cap, asset, AMT);

        let payment = coin::mint_for_testing<SUI>(AMT, ctx);
        let (asset, request) = kiosk::purchase(&mut kiosk, item_id, payment, ctx);
        policy::confirm_request(&mut policy, request);

        return_kiosk(kiosk, owner_cap, ctx);
        return_assets(vector[ asset ]);
        return_policy(policy, policy_cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EIncorrectAmount)]
    fun test_purchase_wrong_amount() {
        let ctx = &mut tx_context::dummy();
        let (policy, policy_cap) = get_policy(ctx);
        let (asset, item_id) = get_asset(ctx);
        let (kiosk, owner_cap) = kiosk::new(ctx);

        kiosk::place_and_list(&mut kiosk, &owner_cap, asset, AMT);

        let payment = coin::mint_for_testing<SUI>(AMT + 1, ctx);
        let (asset, request) = kiosk::purchase(&mut kiosk, item_id, payment, ctx);
        policy::confirm_request(&mut policy, request);
        return_assets(vector[ asset ]);
        return_policy(policy, policy_cap, ctx);

        abort 1337
    }

    #[test]
    fun test_purchase_cap() {
        let ctx = &mut tx_context::dummy();
        let (policy, policy_cap) = get_policy(ctx);
        let (asset, item_id) = get_asset(ctx);
        let (kiosk, owner_cap) = kiosk::new(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        let purchase_cap = kiosk::list_with_purchase_cap(&mut kiosk, &owner_cap, item_id, AMT, ctx);

        let payment = coin::mint_for_testing<SUI>(AMT, ctx);
        let (asset, request) = kiosk::purchase_with_cap(&mut kiosk, purchase_cap, payment, ctx);
        policy::confirm_request(&mut policy, request);

        return_kiosk(kiosk, owner_cap, ctx);
        return_assets(vector[ asset ]);
        return_policy(policy, policy_cap, ctx);
    }

    #[test]
    fun test_purchase_cap_return() {
        let ctx = &mut tx_context::dummy();
        let (policy, policy_cap) = get_policy(ctx);
        let (asset, item_id) = get_asset(ctx);
        let (kiosk, owner_cap) = kiosk::new(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        let purchase_cap = kiosk::list_with_purchase_cap<Asset>(&mut kiosk, &owner_cap, item_id, AMT, ctx);
        kiosk::return_purchase_cap(&mut kiosk, purchase_cap);
        let asset = kiosk::take(&mut kiosk, &owner_cap, item_id);

        return_kiosk(kiosk, owner_cap, ctx);
        return_assets(vector[ asset ]);
        return_policy(policy, policy_cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EAlreadyListed)]
    fun test_purchase_cap_already_listed_fail() {
        let ctx = &mut tx_context::dummy();
        let (asset, item_id) = get_asset(ctx);
        let (kiosk, owner_cap) = kiosk::new(ctx);

        kiosk::place_and_list(&mut kiosk, &owner_cap, asset, AMT);
        let _purchase_cap = kiosk::list_with_purchase_cap<Asset>(&mut kiosk, &owner_cap, item_id, AMT, ctx);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EListedExclusively)]
    fun test_purchase_cap_issued_list_fail() {
        let ctx = &mut tx_context::dummy();
        let (asset, item_id) = get_asset(ctx);
        let (kiosk, owner_cap) = kiosk::new(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        let purchase_cap = kiosk::list_with_purchase_cap<Asset>(&mut kiosk, &owner_cap, item_id, AMT, ctx);
        kiosk::list<Asset>(&mut kiosk, &owner_cap, item_id, AMT);
        kiosk::return_purchase_cap(&mut kiosk, purchase_cap);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotEmpty)]
    fun test_kiosk_has_items() {
        let ctx = &mut tx_context::dummy();
        let (asset, _item_id) = get_asset(ctx);
        let (kiosk, owner_cap) = kiosk::new(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        return_kiosk(kiosk, owner_cap, ctx);
    }

    /// Cleanup: TransferPolicy
    fun return_policy(policy: TransferPolicy<Asset>, cap: TransferPolicyCap<Asset>, ctx: &mut TxContext): u64 {
        let profits = policy::destroy_and_withdraw(policy, cap, ctx);
        coin::burn_for_testing(profits)
    }

    /// Cleanup: Kiosk
    fun return_kiosk(kiosk: Kiosk, cap: KioskOwnerCap, ctx: &mut TxContext): u64 {
        let profits = kiosk::close_and_withdraw(kiosk, cap, ctx);
        coin::burn_for_testing(profits)
    }

    /// Cleanup: vector<Asset>
    fun return_assets(assets: vector<Asset>) {
        while (vector::length(&assets) > 0) {
            let Asset { id } = vector::pop_back(&mut assets);
            object::delete(id)
        };

        vector::destroy_empty(assets)
    }
}
