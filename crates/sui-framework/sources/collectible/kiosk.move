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
    use sui::package::{Self, Publisher};
    use sui::tx_context::{TxContext, sender};
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::event;

    // Collectible is a special case to avoid storing `Publisher`.
    friend sui::collectible;

    /// For when trying to withdraw profits as owner and owner is not set.
    const EOwnerNotSet: u64 = 0;
    /// For when trying to withdraw profits and sender is not owner.
    const ENotOwner: u64 = 1;
    /// For when Coin paid does not match the offer price.
    const EIncorrectAmount: u64 = 2;
    /// For when incorrect arguments passed into `switch_mode` function.
    const EIncorrectArgument: u64 = 3;
    /// For when Transfer is accepted by a wrong Kiosk.
    const EWrongTarget: u64 = 4;
    /// For when trying to withdraw higher amount than stored.
    const ENotEnough: u64 = 5;
    /// For when trying to close a Kiosk and it has items in it.
    const ENotEmpty: u64 = 6;

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

    /// A capability that is issued for Kiosks that don't have owner
    /// specified.
    struct KioskOwnerCap has key, store {
        id: UID,
        for: ID
    }

    /// A "Hot Potato" forcing the buyer to get a transfer permission
    /// from the item type (`T`) owner on purchase attempt.
    struct TransferRequest<phantom T: key + store> {
        /// Amount of SUI paid for the item. Can be used to
        /// calculate the fee / transfer policy enforcement.
        paid: u64,
        /// The ID of the Kiosk the object is being sold from.
        /// Can be used by the TransferPolicy implementors to
        /// create an allowlist of Kiosks which can trade the type.
        from: ID,
    }

    /// A unique capability that allows owner of the `T` to authorize
    /// transfers. Can only be created with the `Publisher` object.
    struct TransferPolicyCap<phantom T: key + store> has key, store {
        id: UID
    }

    // === Dynamic Field keys ===

    /// Dynamic field key for an item placed into the kiosk.
    struct Key has store, copy, drop { id: ID }

    /// Dynamic field key for an active offer to purchase the T.
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

    /// Emitted when a publisher creates a new `TransferPolicyCap` making
    /// the discoverability and tracking the supported types easier.
    struct TransferPolicyCapIssued<phantom T: key + store> has copy, drop {
        id: ID
    }

    // === New Kiosk + ownership modes ===

    /// Creates a new Kiosk without owner but with a Capability.
    public fun new(ctx: &mut TxContext): (Kiosk, KioskOwnerCap) {
        let kiosk = Kiosk {
            id: object::new(ctx),
            profits: balance::zero(),
            owner: sender(ctx),
            item_count: 0
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
        let Kiosk { id, profits, owner: _, item_count } = self;
        let KioskOwnerCap { id: cap_id, for } = cap;

        assert!(object::uid_to_inner(&id) == for, ENotOwner);
        assert!(item_count == 0, ENotEmpty);

        object::delete(cap_id);
        object::delete(id);

        coin::from_balance(profits, ctx)
    }

    /// Change the owner to the transaction sender.
    /// The change is purely cosmetical and does not affect any of the
    /// basic kiosk functions unless some logic for this is implemented
    /// in a third party module.
    public fun set_owner(
        self: &mut Kiosk, cap: &KioskOwnerCap, ctx: &TxContext
    ) {
        assert!(object::id(self) == cap.for, ENotOwner);
        self.owner = sender(ctx);
    }

    // === Publisher functions ===

    /// Register a type in the Kiosk system and receive an `TransferPolicyCap`
    /// which is required to confirm kiosk deals for the `T`. If there's no
    /// `TransferPolicyCap` available for use, the type can not be traded in
    /// kiosks.
    public fun new_transfer_policy_cap<T: key + store>(
        pub: &Publisher, ctx: &mut TxContext
    ): TransferPolicyCap<T> {
        assert!(package::from_package<T>(pub), 0);
        let id = object::new(ctx);
        event::emit(TransferPolicyCapIssued<T> { id: object::uid_to_inner(&id) });
        TransferPolicyCap { id }
    }

    /// Special case for the `sui::collectible` module to be able to register
    /// type without a `Publisher` object. Is not magical and a similar logic
    /// can be implemented for the regular `register_type` call for wrapped types.
    public(friend) fun new_transfer_policy_cap_protected<T: key + store>(
        ctx: &mut TxContext
    ): TransferPolicyCap<T> {
        let id = object::new(ctx);
        event::emit(TransferPolicyCapIssued<T> { id: object::uid_to_inner(&id) });
        TransferPolicyCap { id }
    }

    /// Destroy a TransferPolicyCap.
    /// Can be performed by any party as long as they own it.
    public fun destroy_transfer_policy_cap<T: key + store>(
        cap: TransferPolicyCap<T>
    ) {
        let TransferPolicyCap { id } = cap;
        object::delete(id);
    }

    // === Place and take from the Kiosk ===

    /// Place any object into a Kiosk.
    /// Performs an authorization check to make sure only owner can do that.
    public fun place<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, item: T
    ) {
        assert!(object::id(self) == cap.for, ENotOwner);
        self.item_count = self.item_count + 1;
        dof::add(&mut self.id, Key { id: object::id(&item) }, item)
    }

    /// Take any object from the Kiosk.
    /// Performs an authorization check to make sure only owner can do that.
    public fun take<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, id: ID
    ): T {
        assert!(object::id(self) == cap.for, ENotOwner);
        self.item_count = self.item_count - 1;
        df::remove_if_exists<Offer, u64>(&mut self.id, Offer { id });
        dof::remove(&mut self.id, Key { id })
    }

    // === Trading functionality ===

    /// Make an offer by setting a price for the item and making it publicly
    /// purchasable by anyone on the network.
    ///
    /// Performs an authorization check to make sure only owner can sell.
    public fun make_offer<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, id: ID, price: u64
    ) {
        assert!(object::id(self) == cap.for, ENotOwner);
        df::add(&mut self.id, Offer { id }, price);
        event::emit(NewOfferEvent<T> {
            kiosk: object::id(self), id, price
        })
    }

    /// Place an item into the Kiosk and make an offer - simplifies the flow.
    public fun place_and_offer<T: key + store>(
        self: &mut Kiosk, cap: &KioskOwnerCap, item: T, price: u64
    ) {
        let id = object::id(&item);
        place(self, cap, item);
        make_offer<T>(self, cap, id, price)
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
        self: &mut Kiosk, id: ID, payment: Coin<SUI>
    ): (T, TransferRequest<T>) {
        let price = df::remove<Offer, u64>(&mut self.id, Offer { id });
        let inner = dof::remove<Key, T>(&mut self.id, Key { id });

        self.item_count = self.item_count - 1;
        assert!(price == coin::value(&payment), EIncorrectAmount);
        balance::join(&mut self.profits, coin::into_balance(payment));

        (inner, TransferRequest<T> {
            paid: price,
            from: object::id(self),
        })
    }

    /// Allow a `TransferRequest` for the type `T`. The call is protected
    /// by the type constraint, as only the publisher of the `T` can get
    /// `TransferPolicyCap<T>`.
    ///
    /// Note: unless there's a policy for `T` to allow transfers,
    /// Kiosk trades will not be possible.
    public fun allow_transfer<T: key + store>(
        _cap: &TransferPolicyCap<T>, req: TransferRequest<T>
    ): (u64, ID) {
        let TransferRequest { paid, from } = req;
        (paid, from)
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

    // === Fields access ===

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
}

#[test_only]
module sui::kiosk_creature {
    use sui::tx_context::{TxContext, sender};
    use sui::object::{Self, UID};
    use sui::transfer::transfer;
    use sui::package;

    struct Creature has key, store { id: UID }
    struct KIOSK_CREATURE has drop {}

    // Create a publisher + 2 `Creature`s -> to sender
    fun init(otw: KIOSK_CREATURE, ctx: &mut TxContext) {
        transfer(package::claim(otw, ctx), sender(ctx))
    }

    #[test_only]
    public fun new_creature(ctx: &mut TxContext): Creature {
        Creature { id: object::new(ctx) }
    }

    #[test_only]
    public fun init_collection(ctx: &mut TxContext) {
        init(KIOSK_CREATURE {}, ctx)
    }
}

#[test_only]
module sui::kiosk_tests {
    use sui::kiosk_creature::{Creature, new_creature, init_collection};
    use sui::test_scenario::{Self as ts};
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap, TransferPolicyCap};
    use sui::package::Publisher;
    use sui::transfer::{freeze_object, share_object, transfer};
    use sui::sui::SUI;
    use sui::object;
    use sui::coin;
    use std::option;
    use std::vector;

    /// The price for a Creature.
    const PRICE: u64 = 1000;

    /// Addresses for the current testing suite.
    fun folks(): (address, address) { (@0xBA3, @0xC3EA403) }

    #[test]
    fun test_placing() {
        let (user, creator) = folks();
        let test = ts::begin(creator);

        // Creator creates a collection and gets a Publisher object.
        ts::next_tx(&mut test, creator); {
           init_collection(ts::ctx(&mut test));
        };

        // Creator creates a Kiosk and registers a type.
        // No transfer policy set, TransferPolicyCap is frozen.
        ts::next_tx(&mut test, creator); {
            let pub = ts::take_from_address<Publisher>(&test, creator);
            let (kiosk, kiosk_cap) = kiosk::new(ts::ctx(&mut test));
            let allow_cap = kiosk::new_transfer_policy_cap<Creature>(&pub, ts::ctx(&mut test));

            share_object(kiosk);
            transfer(pub, creator);
            freeze_object(allow_cap);
            transfer(kiosk_cap, creator);
        };


        // Get the TransferPolicyCap from the effects + Kiosk
        let effects = ts::next_tx(&mut test, creator);
        let cap_id = *vector::borrow(&ts::frozen(&effects), 0);
        let kiosk_id = *vector::borrow(&ts::shared(&effects), 0);
        let creature = new_creature(ts::ctx(&mut test));
        let creature_id = object::id(&creature);

        // Place an offer to sell a `creature` for a `PRICE`.
        ts::next_tx(&mut test, creator); {
            let kiosk = ts::take_shared_by_id<Kiosk>(&test, kiosk_id);
            let kiosk_cap = ts::take_from_address<KioskOwnerCap>(&test, creator);

            kiosk::place_and_offer(
                &mut kiosk,
                &kiosk_cap,
                creature,
                PRICE
            );

            ts::return_shared(kiosk);
            transfer(kiosk_cap, creator);
        };

        let effects = ts::next_tx(&mut test, creator);
        assert!(ts::num_user_events(&effects) == 1, 0);

        //
        ts::next_tx(&mut test, user); {
            let kiosk = ts::take_shared_by_id<Kiosk>(&test, kiosk_id);
            let cap = ts::take_immutable_by_id<TransferPolicyCap<Creature>>(&test, cap_id);
            let coin = coin::mint_for_testing<SUI>(PRICE, ts::ctx(&mut test));

            // Is there a change the system can be tricked?
            // Say, someone makes a purchase of 2 Creatures at the same time.
            let (creature, request) = kiosk::purchase(&mut kiosk, creature_id, coin);
            let (paid, from) = kiosk::allow_transfer(&cap, request);

            assert!(paid == PRICE, 0);
            assert!(from == object::id(&kiosk), 0);

            transfer(creature, user);
            ts::return_shared(kiosk);
            ts::return_immutable(cap);
        };

        ts::next_tx(&mut test, creator); {
            let kiosk = ts::take_shared_by_id<Kiosk>(&test, kiosk_id);
            let kiosk_cap = ts::take_from_address<KioskOwnerCap>(&test, creator);

            let profits_1 = kiosk::withdraw(
                &mut kiosk,
                &kiosk_cap,
                option::some(PRICE / 2),
                ts::ctx(&mut test)
            );

            let profits_2 = kiosk::withdraw(
                &mut kiosk,
                &kiosk_cap,
                option::none(),
                ts::ctx(&mut test)
            );

            assert!(coin::value(&profits_1) == coin::value(&profits_2), 0);
            transfer(profits_1, creator);
            transfer(profits_2, creator);
            transfer(kiosk_cap, creator);
            ts::return_shared(kiosk);
        };

        ts::end(test);
    }
}
