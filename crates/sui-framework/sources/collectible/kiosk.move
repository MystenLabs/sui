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
    use std::type_name;
    use std::option::{Self, Option};
    use sui::object::{Self, UID, ID};
    use sui::dynamic_field as df;
    use sui::tx_context::{TxContext, sender};
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::event;
    use sui::nft_safe::{Self, NftSafe, OwnerCap, TransferRequest};

    // Collectible is a special case to avoid storing `Publisher`.
    friend sui::collectible;

    /// Trying to withdraw profits as owner and owner is not set.
    const EOwnerNotSet: u64 = 0;
    /// Trying to withdraw profits and sender is not owner.
    const ENotOwner: u64 = 1;
    /// Coin paid does not match the offer price.
    const EIncorrectAmount: u64 = 2;
    /// Incorrect arguments passed into `switch_mode` function.
    const EIncorrectArgument: u64 = 3;
    /// Transfer is accepted by a wrong Kiosk.
    const EWrongTarget: u64 = 4;
    /// Trying to withdraw higher amount than stored.
    const ENotEnough: u64 = 5;
    /// Trying to close a Kiosk and it has items in it.
    const ENotEmpty: u64 = 6;
    /// Attempt to take an item that has a `PurchaseCap` issued.
    const EListedExclusively: u64 = 7;
    /// `PurchaseCap` does not match the `Kiosk`.
    const EWrongKiosk: u64 = 8;

    struct Witness has drop {}

    /// An object that stores collectibles of all sorts.
    /// For sale, for collecting reasons, for fun.
    struct Kiosk has key, store {
        id: UID,
        /// Balance of the Kiosk - all profits from sales go here.
        profits: Balance<SUI>,
        /// Always point to `sender` of the transaction.
        /// Can be changed by calling `set_owner` with Cap.
        owner: address,
        /// Number of items stored in a Kiosk. Used to allow unpacking
        /// an empty Kiosk if it was wrapped or has a single owner.
        item_count: u32
    }

    /// A capability which locks an item and gives a permission to
    /// purchase it from a `Kiosk` for any price no less than `min_price`.
    ///
    /// Allows exclusive listing: only bearer of the `PurchaseCap` can
    /// purchase the asset. However, the capablity should be used
    /// carefully as losing it would lock the asset in the `Kiosk`.
    struct PurchaseCap<phantom T: key + store> has key, store {
        id: UID,
        /// ID of the `NftSafe` the cap belongs to.
        safe_id: ID,
        /// ID of the listed item.
        item_id: ID,
        /// Minimum price for which the item can be purchased.
        min_price: u64
    }

    // === Dynamic Field keys ===

    /// Dynamic field key for an active offer to purchase the T. If an
    /// item is listed without a `PurchaseCap`, exclusive is set to `false`.
    struct Offer has store, copy, drop { id: ID }

    // === Events ===

    /// Emitted when an item was listed by the safe owner. Can be used
    /// to track available offers anywhere on the network; the event is
    /// type-indexed which allows for searching for offers of a specific `T`
    struct NewOfferEvent<phantom T: key + store> has copy, drop {
        kiosk: ID,
        id: ID,
        price: u64
    }

    // === New Kiosk + ownership modes ===

    /// Creates a new Kiosk without owner but with a Capability.
    public fun new(ctx: &mut TxContext): (NftSafe<Kiosk>, OwnerCap) {
        let inner = Kiosk {
            id: object::new(ctx),
            profits: balance::zero(),
            owner: sender(ctx),
            item_count: 0
        };

        nft_safe::new(inner, ctx)
    }

    /// Unpacks and destroys a Kiosk returning the profits (even if "0").
    /// Can only be performed by the bearer of the `nft_safe::OwnerCap` in the
    /// case where there's no items inside and a `Kiosk` is not shared.
    public fun close_and_withdraw(
        self: NftSafe<Kiosk>,
        owner: OwnerCap,
        ctx: &mut TxContext,
    ): Coin<SUI> {
        let Kiosk { id, profits, owner: _, item_count: _ } = nft_safe::destroy_empty(
            self, owner, Witness {}
        );
        object::delete(id);

        coin::from_balance(profits, ctx)
    }

    /// Change the owner to the transaction sender.
    /// The change is purely cosmetical and does not affect any of the
    /// basic kiosk functions unless some logic for this is implemented
    /// in a third party module.
    public fun set_owner(
        self: &mut NftSafe<Kiosk>, owner: &OwnerCap, ctx: &TxContext
    ) {
        assert!(object::id(self) == nft_safe::owner_cap_safe(owner), ENotOwner);
        let kiosk = nft_safe::borrow_inner_mut(self);
        kiosk.owner = sender(ctx);
    }

    // === Place and take from the Kiosk ===

    /// Place any object into a Kiosk.
    /// Performs an authorization check to make sure only owner can do that.
    public fun place<T: key + store>(
        self: &mut NftSafe<Kiosk>, owner: &OwnerCap, item: T
    ) {
        nft_safe::assert_owner_cap(self, owner);
        nft_safe::deposit_nft(self, item, Witness {});

        let kiosk = nft_safe::borrow_inner_mut(self);
        kiosk.item_count = kiosk.item_count + 1;
    }

    /// Take any object from the Kiosk.
    /// Performs an authorization check to make sure only owner can do that.
    public fun take<T: key + store>(
        self: &mut NftSafe<Kiosk>, owner: &OwnerCap, id: ID
    ): T {
        let nft = nft_safe::get_nft_as_owner(self, owner, id, Witness {});

        let kiosk = nft_safe::borrow_inner_mut(self);
        kiosk.item_count = kiosk.item_count - 1;

        nft
    }

    // === Trading functionality: List and Purchase ===

    /// List the item by setting a price and making it available for purchase.
    /// Performs an authorization check to make sure only owner can sell.
    public fun list<T: key + store>(
        self: &mut NftSafe<Kiosk>,
        owner: &OwnerCap,
        nft_id: ID,
        price: u64,
    ) {
        let kiosk = nft_safe::borrow_inner_mut(self);
        let entity_id = object::uid_to_inner(&kiosk.id);

        nft_safe::list_nft(self, owner, entity_id, nft_id, Witness {});

        let kiosk = nft_safe::borrow_inner_mut(self);
        df::add(&mut kiosk.id, Offer { id: nft_id }, price);

        event::emit(NewOfferEvent<T> {
            kiosk: object::id(self), id: nft_id, price
        })
    }

    /// Calls `place` and `list` together - simplifies the flow.
    public fun place_and_list<T: key + store>(
        self: &mut NftSafe<Kiosk>, owner: &OwnerCap, item: T, price: u64
    ) {
        let id = object::id(&item);

        place(self, owner, item);
        list<T>(self, owner, id, price)
    }

    /// Make a trade: pay the owner of the item and request a Transfer to the `target`
    /// kiosk (to prevent item being taken by the approving party).
    ///
    /// Received `TransferRequest` needs to be handled by the publisher of the T,
    /// if they have a method implemented that allows a trade, it is possible to
    /// request their approval (by calling some function) so that the trade can be
    /// finalized.
    ///
    /// After a confirmation is received from the creator, an item can be placed to
    /// a destination safe.
    public fun purchase<T: key + store>(
        self: &mut NftSafe<Kiosk>, nft_id: ID, payment: Coin<SUI>
    ): (T, TransferRequest<T>) {
        let tx_info = nft_safe::get_transaction_info(
            balance::value(coin::balance(&payment)),
            type_name::get<SUI>(),
            object::id(self)
        );
              

        let (item, request) = nft_safe::get_nft_to_inner_entity<Kiosk, Witness, T>(
            self,
            nft_id,
            tx_info,
            Witness {}
        );

        let kiosk = nft_safe::borrow_inner_mut(self);
        let price = df::remove<Offer, u64>(&mut kiosk.id, Offer { id: nft_id });
        assert!(price == coin::value(&payment), EIncorrectAmount);
        balance::join(&mut kiosk.profits, coin::into_balance(payment));

        (item, request)
    }

    // === Trading Functionality: Exclusive listing with `PurchaseCap` ===

    /// Creates a `PurchaseCap` which gives the right to purchase an item
    /// for any price equal or higher than the `min_price`.
    public fun list_with_purchase_cap<T: key + store>(
        self: &mut NftSafe<Kiosk>,
        owner: &OwnerCap,
        nft_id: ID,
        min_price: u64,
        ctx: &mut TxContext,
    ): PurchaseCap<T> {
        // For exclusive listing, we require that the purchase cap UID is used
        // to claim the NFT.
        let purchase_cap_uid = object::new(ctx);
        nft_safe::exclusively_list_nft(
            self, owner, nft_id, &purchase_cap_uid, Witness {}
        );

        let kiosk = nft_safe::borrow_inner_mut(self);
        df::add(&mut kiosk.id, Offer { id: nft_id }, min_price);

        PurchaseCap<T> {
            id: purchase_cap_uid,
            item_id: nft_id,
            safe_id: object::id(self),
            min_price,
        }
    }

    /// Purchases with a `PurchaseCap` - allows to purchase an item for any
    /// price higher than min price.
    public fun purchase_with_cap<T: key + store>(
        self: &mut NftSafe<Kiosk>,
        purchase_cap: PurchaseCap<T>,
        payment: Coin<SUI>,
    ): (T, TransferRequest<T>) {
        let PurchaseCap {
            id: purchase_cap_uid, item_id, safe_id: _, min_price
        } = purchase_cap;

        let tx_info = nft_safe::get_transaction_info(
            balance::value(coin::balance(&payment)),
            type_name::get<SUI>(),
            object::uid_to_inner(&purchase_cap_uid)
        );

        let paid = coin::value(&payment);
        assert!(paid >= min_price, EIncorrectAmount);

        let (nft, request) = nft_safe::get_nft<Kiosk, Witness, T>(
            self, &purchase_cap_uid, item_id, tx_info, Witness {}
        );
        object::delete(purchase_cap_uid);

        let kiosk = nft_safe::borrow_inner_mut(self);
        df::remove<Offer, u64>(&mut kiosk.id, Offer { id: item_id });
        balance::join(&mut kiosk.profits, coin::into_balance(payment));

        (nft, request)
    }

    /// Return the `PurchaseCap` without making a purchase; remove an active offer and
    /// allow taking . Can only be returned to its `Kiosk`, aborts otherwise.
    public fun return_purchase_cap<T: key + store>(
        self: &mut NftSafe<Kiosk>,
        purchase_cap: PurchaseCap<T>
    ) {
        let PurchaseCap { id, item_id, safe_id: _, min_price: _ } = purchase_cap;

        nft_safe::remove_entity_from_nft_listing(
            self,
            item_id,
            &id,
            Witness {},
        );

        let kiosk = nft_safe::borrow_inner_mut(self);
        df::remove<Offer, u64>(&mut kiosk.id, Offer { id: item_id });
        object::delete(id)
    }

    /// Withdraw profits from the Kiosk.
    public fun withdraw(
        self: &mut NftSafe<Kiosk>,
        cap: &nft_safe::OwnerCap,
        amount: Option<u64>,
        ctx: &mut TxContext,
    ): Coin<SUI> {
        nft_safe::assert_owner_cap(self, cap);

        let kiosk = nft_safe::borrow_inner_mut(self);

        let amount = if (option::is_some(&amount)) {
            let amt = option::destroy_some(amount);
            assert!(amt <= balance::value(&kiosk.profits), ENotEnough);
            amt
        } else {
            balance::value(&kiosk.profits)
        };

        coin::take(&mut kiosk.profits, amount, ctx)
    }

    // === Kiosk fields access ===

    /// Get the UID to for dynamic field access. Requires a `KioskOwnerCap`
    /// to prevent third party attachements without owner's approval in the
    /// shared storage scenario.
    public fun uid_mut(self: &mut NftSafe<Kiosk>, owner: &OwnerCap): &mut UID {
        nft_safe::assert_owner_cap(self, owner);
        
        let kiosk = nft_safe::borrow_inner_mut(self);

        &mut kiosk.id
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

    // === PurchaseCap fields access ===

    /// Get the `safe_id` from the `PurchaseCap`.
    public fun purchase_cap_safe<T: key + store>(self: &PurchaseCap<T>): ID {
        self.safe_id
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
