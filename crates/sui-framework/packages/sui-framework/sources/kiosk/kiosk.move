// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Kiosk is a primitive for building open, zero-fee trading platforms
/// with a high degree of customization over transfer policies.

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
    use sui::tx_context::{TxContext, sender};
    use sui::transfer_policy::{
        Self,
        TransferPolicy,
        TransferRequest
    };
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::event;

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
    /// Attempt to `take` an item that is locked.
    const EItemLocked: u64 = 8;
    /// Taking or mutably borrowing an item that is listed.
    const EItemIsListed: u64 = 9;
    /// Item does not match `Borrow` in `return_val`.
    const EItemMismatch: u64 = 10;
    /// An is not found while trying to borrow.
    const EItemNotFound: u64 = 11;
    /// Delisting an item that is not listed.
    const ENotListed: u64 = 12;

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

    // === Utilities ===

    /// Hot potato to ensure an item was returned after being taken using
    /// the `borrow_val` call.
    struct Borrow { kiosk_id: ID, item_id: ID }

    // === Dynamic Field keys ===

    /// Dynamic field key for an item placed into the kiosk.
    struct Item has store, copy, drop { id: ID }

    /// Dynamic field key for an active offer to purchase the T. If an
    /// item is listed without a `PurchaseCap`, exclusive is set to `false`.
    struct Listing has store, copy, drop { id: ID, is_exclusive: bool }

    /// Dynamic field key which marks that an item is locked in the `Kiosk` and
    /// can't be `take`n. The item then can only be listed / sold via the PurchaseCap.
    /// Lock is released on `purchase`.
    struct Lock has store, copy, drop { id: ID }

    // === Events ===

    /// Emitted when an item was listed by the safe owner. Can be used
    /// to track available offers anywhere on the network; the event is
    /// type-indexed which allows for searching for offers of a specific `T`
    struct ItemListed<phantom T: key + store> has copy, drop {
        kiosk: ID,
        id: ID,
        price: u64
    }

    /// Emitted when an item was purchased from the `Kiosk`. Can be used
    /// to track finalized sales across the network. The event is emitted
    /// in both cases: when an item is purchased via the `PurchaseCap` or
    /// when it's purchased directly (via `list` + `purchase`).
    ///
    /// The `price` is also emitted and might differ from the `price` set
    /// in the `ItemListed` event. This is because the `PurchaseCap` only
    /// sets a minimum price for the item, and the actual price is defined
    /// by the trading module / extension.
    struct ItemPurchased<phantom T: key + store> has copy, drop {
        kiosk: ID,
        id: ID,
        price: u64
    }

    /// Emitted when an item was delisted by the safe owner. Can be used
    /// to close tracked offers.
    struct ItemDelisted<phantom T: key + store> has copy, drop {
        kiosk: ID,
        id: ID
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

    /// Update the `owner` field with a custom address. Can be used for
    /// implementing a custom logic that relies on the `Kiosk` owner.
    public fun set_owner_custom(
        self: &mut Kiosk, cap: &KioskOwnerCap, owner: address
    ) {
        assert!(object::id(self) == cap.for, ENotOwner);
        self.owner = owner
    }

    // === Place, Lock and Take from the Kiosk ===

    /// Place any object into a Kiosk.
    /// Performs an authorization check to make sure only owner can do that.
    /// Makes sure a `TransferPolicy` exists for `T`, otherwise assets can be
    /// locked in the `Kiosk` forever.
    public fun place<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, item: T
    ) {
        assert!(object::id(self) == cap.for, ENotOwner);
        self.item_count = self.item_count + 1;
        dof::add(&mut self.id, Item { id: object::id(&item) }, item)
    }

    /// Place an item to the `Kiosk` and issue a `Lock` for it. Once placed this
    /// way, an item can only be listed either with a `list` function or with a
    /// `list_with_purchase_cap`.
    ///
    /// Requires policy for `T` to make sure that there's an issued `TransferPolicy`
    /// and the item can be sold.
    public fun lock<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, _policy: &TransferPolicy<T>, item: T
    ) {
        df::add(&mut self.id, Lock { id: object::id(&item) }, true);
        place(self, cap, item)
    }

    /// Take any object from the Kiosk.
    /// Performs an authorization check to make sure only owner can do that.
    public fun take<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, id: ID
    ): T {
        assert!(object::id(self) == cap.for, ENotOwner);
        assert!(!is_locked(self, id), EItemLocked);
        assert!(!is_listed_exclusively(self, id), EListedExclusively);
        assert!(has_item(self, id), EItemNotFound);

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
        assert!(has_item_with_type<T>(self, id), EItemNotFound);
        assert!(!is_listed_exclusively(self, id), EListedExclusively);

        df::add(&mut self.id, Listing { id, is_exclusive: false }, price);
        event::emit(ItemListed<T> { kiosk: object::id(self), id, price })
    }

    /// Calls `place` and `list` together - simplifies the flow.
    public fun place_and_list<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, item: T, price: u64
    ) {
        let id = object::id(&item);
        place(self, cap, item);
        list<T>(self, cap, id, price)
    }

    /// Remove an existing listing from the `Kiosk` and keep the item in the
    /// user Kiosk. Can only be performed by the owner of the `Kiosk`.
    public fun delist<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, id: ID
    ) {
        assert!(object::id(self) == cap.for, ENotOwner);
        assert!(has_item_with_type<T>(self, id), EItemNotFound);
        assert!(!is_listed_exclusively(self, id), EListedExclusively);
        assert!(is_listed(self, id), ENotListed);

        df::remove<Listing, u64>(&mut self.id, Listing { id, is_exclusive: false });
        event::emit(ItemDelisted<T> { kiosk: object::id(self), id })
    }

    /// Make a trade: pay the owner of the item and request a Transfer to the `target`
    /// kiosk (to prevent item being taken by the approving party).
    ///
    /// Received `TransferRequest` needs to be handled by the publisher of the T,
    /// if they have a method implemented that allows a trade, it is possible to
    /// request their approval (by calling some function) so that the trade can be
    /// finalized.
    public fun purchase<T: key + store>(
        self: &mut Kiosk, id: ID, payment: Coin<SUI>
    ): (T, TransferRequest<T>) {
        let price = df::remove<Listing, u64>(&mut self.id, Listing { id, is_exclusive: false });
        let inner = dof::remove<Item, T>(&mut self.id, Item { id });

        self.item_count = self.item_count - 1;
        assert!(price == coin::value(&payment), EIncorrectAmount);
        balance::join(&mut self.profits, coin::into_balance(payment));
        df::remove_if_exists<Lock, bool>(&mut self.id, Lock { id });

        event::emit(ItemPurchased<T> { kiosk: object::id(self), id, price });

        (inner, transfer_policy::new_request(id, price, object::id(self)))
    }

    // === Trading Functionality: Exclusive listing with `PurchaseCap` ===

    /// Creates a `PurchaseCap` which gives the right to purchase an item
    /// for any price equal or higher than the `min_price`.
    public fun list_with_purchase_cap<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, id: ID, min_price: u64, ctx: &mut TxContext
    ): PurchaseCap<T> {
        assert!(object::id(self) == cap.for, ENotOwner);
        assert!(has_item_with_type<T>(self, id), EItemNotFound);
        assert!(!is_listed(self, id), EAlreadyListed);

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
        self: &mut Kiosk, purchase_cap: PurchaseCap<T>, payment: Coin<SUI>
    ): (T, TransferRequest<T>) {
        let PurchaseCap { id, item_id, kiosk_id, min_price } = purchase_cap;
        let paid = coin::value(&payment);

        assert!(paid >= min_price, EIncorrectAmount);
        assert!(object::id(self) == kiosk_id, EWrongKiosk);

        df::remove<Listing, u64>(&mut self.id, Listing { id: item_id, is_exclusive: true });
        df::add(&mut self.id, Listing { id: item_id, is_exclusive: false }, paid);
        object::delete(id);

        purchase<T>(self, item_id, payment)
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

    /// Check whether the `item` is present in the `Kiosk`.
    public fun has_item(self: &Kiosk, id: ID): bool {
        dof::exists_(&self.id, Item { id })
    }

    /// Check whether the `item` is present in the `Kiosk` and has type T.
    public fun has_item_with_type<T: key + store>(self: &Kiosk, id: ID): bool {
        dof::exists_with_type<Item, T>(&self.id, Item { id })
    }

    /// Check whether an item with the `id` is locked in the `Kiosk`. Meaning
    /// that the only two actions that can be performed on it are `list` and
    /// `list_with_purchase_cap`, it cannot be `take`n out of the `Kiosk`.
    public fun is_locked(self: &Kiosk, id: ID): bool {
        df::exists_(&self.id, Lock { id })
    }

    /// Check whether an `item` is listed (exclusively or non exclusively).
    public fun is_listed(self: &Kiosk, id: ID): bool {
        df::exists_(&self.id, Listing { id, is_exclusive: false })
        || is_listed_exclusively(self, id)
    }

    /// Check whether there's a `PurchaseCap` issued for an item.
    public fun is_listed_exclusively(self: &Kiosk, id: ID): bool {
        df::exists_(&self.id, Listing { id, is_exclusive: true })
    }

    /// Check whether the `KioskOwnerCap` matches the `Kiosk`.
    public fun has_access(self: &mut Kiosk, cap: &KioskOwnerCap): bool {
        object::id(self) == cap.for
    }

    /// Access the `UID` using the `KioskOwnerCap`.
    public fun uid_mut_as_owner(self: &mut Kiosk, cap: &KioskOwnerCap): &mut UID {
        assert!(object::id(self) == cap.for, ENotOwner);
        &mut self.id
    }

    /// Allow or disallow `uid` and `uid_mut` access via the `allow_extensions` setting.
    public fun set_allow_extensions(self: &mut Kiosk, cap: &KioskOwnerCap, allow_extensions: bool) {
        assert!(object::id(self) == cap.for, ENotOwner);
        self.allow_extensions = allow_extensions;
    }

    /// Get the immutable `UID` for dynamic field access.
    /// Aborts if `allow_extensions` set to `false`.
    ///
    /// Given the &UID can be used for reading keys and authorization,
    /// its access
    public fun uid(self: &Kiosk): &UID {
        assert!(self.allow_extensions, EExtensionsDisabled);
        &self.id
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

    // === Item borrowing ===

    /// Immutably borrow an item from the `Kiosk`. Any item can be `borrow`ed
    /// at any time.
    public fun borrow<T: key + store>(
        self: &Kiosk, cap: &KioskOwnerCap, id: ID
    ): &T {
        assert!(object::id(self) == cap.for, ENotOwner);
        assert!(has_item(self, id), EItemNotFound);

        dof::borrow(&self.id, Item { id })
    }

    /// Mutably borrow an item from the `Kiosk`.
    /// Item can be `borrow_mut`ed only if it's not `is_listed`.
    public fun borrow_mut<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, id: ID
    ): &mut T {
        assert!(object::id(self) == cap.for, ENotOwner);
        assert!(has_item(self, id), EItemNotFound);
        assert!(!is_listed(self, id), EItemIsListed);

        dof::borrow_mut(&mut self.id, Item { id })
    }

    /// Take the item from the `Kiosk` with a guarantee that it will be returned.
    /// Item can be `borrow_val`-ed only if it's not `is_listed`.
    public fun borrow_val<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, id: ID
    ): (T, Borrow) {
        assert!(object::id(self) == cap.for, ENotOwner);
        assert!(has_item(self, id), EItemNotFound);
        assert!(!is_listed(self, id), EItemIsListed);

        (
            dof::remove(&mut self.id, Item { id }),
            Borrow { kiosk_id: object::id(self), item_id: id }
        )
    }

    /// Return the borrowed item to the `Kiosk`. This method cannot be avoided
    /// if `borrow_val` is used.
    public fun return_val<T: key + store>(
        self: &mut Kiosk, item: T, borrow: Borrow
    ) {
        let Borrow { kiosk_id, item_id } = borrow;

        assert!(object::id(self) == kiosk_id, EWrongKiosk);
        assert!(object::id(&item) == item_id, EItemMismatch);

        dof::add(&mut self.id, Item { id: item_id }, item);
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
