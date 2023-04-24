// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Kiosk is a trading primitive and the main building block for Asset trading.
///
/// 1. Anyone can create a new Kiosk by running `kiosk::new()`
/// 2. By default, only the owner of the Kiosk can place/take/borrow items.
/// 3. The owner can also list items for sale (in SUI) right in the Kiosk allowing
/// anyone on the network to purchase it.
/// 4. Kiosk enforces `TransferPolicy` on every purchase; the buyer must complete
/// the Transfer Policy requirements to unblock the transaction.
/// 5. Kiosk supports strong policy enforcement by allowing "locking" an asset in
/// the Kiosk and only allowing it to be sold (can't be taken). To lock an item,
/// use `kiosk::lock()` method.
/// 6. If there's a need to use the trading functionality in a third party module,
/// owner can create a `PurchaseCap` which locks the asset and allows the bearer
/// to purchase it from the Kiosk for any price no less than the minimum price set
/// in the `PurchaseCap` (this allows for variable-price sales).
/// 7. Kiosk can be extended with a custom functionality by using `PurchaseCap`
/// and dynamic fields (see `kiosk` section in examples).
///
/// Kiosk requires TransferPolicy approval on every purchase, be it via the simple
/// purchase flow or a purchase via the `PurchaseCap`, therefore giving creators an
/// option to enforce custom rules on the trading of their assets.
///
/// See `sui::transfer_policy` for mode details on `TransferPolicy` and `TransferRequest`.
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
    use sui::kiosk_permissions as permissions;

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
    /// Extension is not allowed to perform this action.
    const EExtNotPermitted: u64 = 12;
    /// Extension is not installed.
    const EExtNotInstalled: u64 = 13;

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
        /// Minimum price for which the item can be purchased. This field
        /// acts as a guarantee of payment in cases when the `PurchaseCap`
        /// is entrusted to the third party.
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
    /// can't be `take`n. The item then can only be listed / sold via the `PurchaseCap`.
    /// Lock is released on `purchase`.
    struct Lock has store, copy, drop { id: ID }

    /// Dynamic field key for an extension abilities configuration. Certain
    /// extensions might need to have access to owner-only functions if the owner
    /// authorizes their usage. Currently supported methods are: `borrow`, `place`
    /// and `lock`.
    ///
    /// The `permissions` can support up to 16 different "actions" and all their
    /// combinations in the future.
    struct Extension<phantom E> has store, copy, drop {}

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
        place_(self, item)
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
        assert!(has_item(self, id), EItemNotFound);
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
        assert!(has_item(self, id), EItemNotFound);
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

    // === Kiosk Extensions ===

    /// Add a new extension to the Kiosk; depending on the `cap` parameter, the extension
    /// might be able to call `ext_place` (and `ext_lock`), `ext_borrow` and `ext_borrow_mut`
    /// functions.
    ///
    /// The call visibility is intentionally `entry` to make sure that the extension is
    /// explicitly installed by the owner of the Kiosk (and avoid arbitrary execution).
    entry fun add_extension<E: drop>(
        self: &mut Kiosk,
        cap: &KioskOwnerCap,
        permissions: u16
    ) {
        assert!(object::id(self) == cap.for, ENotOwner);
        df::add(&mut self.id, Extension<E> {}, permissions);
    }

    /// Check whether an extension is installed.
    public fun has_extension<E: drop>(self: &Kiosk): bool {
        df::exists_(&self.id, Extension<E> {})
    }

    /// Get the permissions set for the extension.
    public fun get_extension_permissions<E: drop>(self: &Kiosk): u16 {
        assert!(has_extension<E>(self), EExtNotInstalled);
        *df::borrow(&self.id, Extension<E> {})
    }

    /// Remove an extension from the Kiosk; can be performed any time, even if the
    /// extension does not implement uninstallation logic.
    public fun remove_extension<E>(self: &mut Kiosk, cap: &KioskOwnerCap): Option<u16> {
        assert!(object::id(self) == cap.for, ENotOwner);
        df::remove_if_exists(&mut self.id, Extension<E> {})
    }

    /// Extension: place an item if the `Place` action is enabled.
    public fun place_as_extension<E: drop, T: key + store>(
        _ext: E, self: &mut Kiosk, item: T
    ) {
        let permissions = get_extension_permissions<E>(self);
        assert!(permissions::can_place(permissions), EExtNotPermitted);
        place_(self, item)
    }

    /// Extension: place and lock an item if the `Lock` action is enabled.
    public fun lock_as_extension<E: drop, T: key + store>(
        _ext: E, self: &mut Kiosk, _policy: &TransferPolicy<T>, item: T
    ) {
        let permissions = get_extension_permissions<E>(self);
        assert!(permissions::can_place(permissions), EExtNotPermitted);
        df::add(&mut self.id, Lock { id: object::id(&item) }, true);
        place_(self, item)
    }

    /// Extension: borrow an item if the `Borrow` action is enabled.
    public fun borrow_as_extension<E: drop, T: key + store>(
        _ext: E, self: &Kiosk, id: ID
    ): &T {
        let permissions = get_extension_permissions<E>(self);
        assert!(permissions::can_borrow(permissions), EExtNotPermitted);
        assert!(has_item(self, id), EItemNotFound);
        dof::borrow(&self.id, Item { id })
    }

    // === Extension-supported calls and Internal ===

    /// Internal method - place an item in the `Kiosk`. Can be called by the Kiosk
    /// Owner or by an Extension if the "Place" capability is enabled.
    fun place_<T: key + store>(self: &mut Kiosk, item: T) {
        self.item_count = self.item_count + 1;
        dof::add(&mut self.id, Item { id: object::id(&item) }, item)
    }

    // === Kiosk fields access ===

    /// Check whether the an `item` is present in the `Kiosk`.
    public fun has_item(self: &Kiosk, id: ID): bool {
        dof::exists_(&self.id, Item { id })
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

    /// Access the `UID` using the `Extension` setting. Any installed extension can
    /// get mutable access to Kiosk UID no matter which permissions are set.
    public fun uid_mut_as_extension<E: drop>(_ext: E, self: &mut Kiosk): &mut UID {
        assert!(has_extension<E>(self), EExtNotInstalled);
        &mut self.id
    }

    /// Allow or disallow `uid_mut` access via the `allow_extensions` setting.
    public fun set_allow_extensions(
        self: &mut Kiosk, cap: &KioskOwnerCap, allow_extensions: bool
    ) {
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

    /// Immutably borrow an item from the `Kiosk`. Any item can be `borrow`ed at any time.
    public fun borrow<T: key + store>(self: &Kiosk, cap: &KioskOwnerCap, id: ID): &T {
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

    /// Return the borrowed item to the `Kiosk`. This method cannot be avoided if
    /// `borrow_val` is used.
    public fun return_val<T: key + store>(self: &mut Kiosk, item: T, borrow: Borrow) {
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

    #[test_only]
    /// Test-only version of `add_extension`
    public fun add_extension_for_testing<E: drop>(
        self: &mut Kiosk, cap: &KioskOwnerCap, permissions: u16
    ) {
        add_extension<E>(self, cap, permissions);
    }
}

/// Utility module implementing the permissions for the `Kiosk`.
///
/// Permissions:
/// - `place_as_extension` and `lock_as_extension`
/// - `borrow_as_extension`
/// - `borrow_mut_as_extension`
module sui::kiosk_permissions {
    friend sui::kiosk;

    /// Check whether the first bit of the value is set (odd value)
    public fun can_place(permissions: u16): bool { permissions & 0x01 != 0 }
    /// Check whether the second bit of the value is set;
    public fun can_borrow(permissions: u16): bool { permissions & 0x02 != 0 }
    /// Check whether the third bit of the value is set;
    public fun can_borrow_mut(permissions: u16): bool { permissions & 0x04 != 0 }

    #[test]
    /// Test the bits of the value.
    fun test_permissions() {
        assert!(check(0x0) == vector[false, false, false], 0); // 000
        assert!(check(0x1) == vector[false, false, true], 0);  // 001
        assert!(check(0x2) == vector[false, true, false], 0);  // 010
        assert!(check(0x3) == vector[false, true, true], 0);   // 011
        assert!(check(0x4) == vector[true, false, false], 0);  // 100
        assert!(check(0x5) == vector[true, false, true], 0);   // 101
    }

    /// Add the `place_as_extension` and `lock_as_extension` permission to the permissions set.
    public fun add_place(permissions: &mut u16) { *permissions = *permissions | 0x01 }

    /// Add the `borrow_as_extension` permission to the permissions set.
    public fun add_borrow(permissions: &mut u16) { *permissions = *permissions | 0x02 }

    /// Add the `borrow_mut_as_extension` permission to the permissions set.
    public fun add_borrow_mut(permissions: &mut u16) { *permissions = *permissions | 0x04 }

    #[test_only]
    /// Turn the bits into a vector of booleans for testing.
    fun check(self: u16): vector<bool> {
        vector[
            can_borrow_mut(self),
            can_borrow(self),
            can_place(self),
        ]
    }

    #[test]
    fun kiosk_permissions() {
        let permissions = 0u16;
        assert!(!can_place(permissions), 0);
        assert!(!can_borrow(permissions), 1);
        assert!(!can_borrow_mut(permissions), 2);

        add_place(&mut permissions);
        assert!(can_place(permissions), 3);
        assert!(!can_borrow(permissions), 4);
        assert!(!can_borrow_mut(permissions), 5);

        add_borrow(&mut permissions);
        assert!(can_place(permissions), 6);
        assert!(can_borrow(permissions), 7);
        assert!(!can_borrow_mut(permissions), 8);

        add_borrow_mut(&mut permissions);
        assert!(can_place(permissions), 9);
        assert!(can_borrow(permissions), 10);
        assert!(can_borrow_mut(permissions), 11);
    }
}
