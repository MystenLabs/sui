// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Ownership modes:
/// - either the `kiosk.owner` is set - address owner;
/// - or a Cap is issued;
/// - mode can be changed at any point by its owner / capability bearer.
///
///
module sui::kiosk {
    use sui::object::{Self, UID, ID};
    use sui::dynamic_object_field as dof;
    use sui::dynamic_field as df;
    use sui::publisher::{Self, Publisher};
    use sui::tx_context::{TxContext, sender};
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use std::option::{Self, Option};

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

    /// An object that stores collectibles of all sorts.
    /// For sale, for collecting reasons, for fun.
    struct Kiosk has key, store {
        id: UID,
        profits: Balance<SUI>,
        owner: Option<address>
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
        /// Can be used by the TransferPolicy implementors to ban
        /// some Kiosks or the opposite - relax some rules.
        from: ID,
    }

    /// A unique capability that allows owner of the `T` to authorize
    /// transfers. Can only be created with the `Publisher` object.
    struct AllowTransferCap<phantom T: key + store> has key, store {
        id: UID
    }

    // === Dynamic Field keys ===

    /// Dynamic field key for an item placed into the kiosk.
    struct Key has store, copy, drop { id: ID }

    /// Dynamic field key for an active offer to purchase the T.
    struct Offer has store, copy, drop { id: ID }

    // === New Kiosk + ownership modes ===

    /// Creates a new Kiosk with the owner set.
    public fun new_for_sender(ctx: &mut TxContext): Kiosk {
        Kiosk {
            id: object::new(ctx),
            profits: balance::zero(),
            owner: option::some(sender(ctx))
        }
    }

    /// Creates a new Kiosk without owner but with a Capability.
    public fun new_with_capability(ctx: &mut TxContext): (Kiosk, KioskOwnerCap) {
        let kiosk = Kiosk {
            id: object::new(ctx),
            profits: balance::zero(),
            owner: option::none()
        };

        let cap = KioskOwnerCap {
            id: object::new(ctx),
            for: object::id(&kiosk)
        };

        (kiosk, cap)
    }

    /// Switch the ownership mode.
    /// 1. If `kiosk.owner` is set, unset it and issue a `KioskOwnerCap`
    /// 2. If `kiosk.owner` is not set, exchange `KioskOwnerCap` for this setting.
    public fun switch_mode(kiosk: &mut Kiosk, cap: Option<KioskOwnerCap>, ctx: &mut TxContext): Option<KioskOwnerCap> {
        assert!(
            (option::is_some(&cap) && option::is_none(&kiosk.owner)) ||
            (option::is_none(&cap) && option::is_some(&kiosk.owner))
        , EIncorrectArgument);

        if (option::is_some(&cap)) {
            let KioskOwnerCap { id, for } = option::destroy_some(cap);
            assert!(for == object::id(kiosk), ENotOwner);
            kiosk.owner = option::some(sender(ctx));
            object::delete(id);
            option::none()
        } else {
            assert!(sender(ctx) == *option::borrow(&kiosk.owner), ENotOwner);
            kiosk.owner = option::none();
            option::destroy_none(cap);
            option::some(KioskOwnerCap {
                id: object::new(ctx),
                for: object::id(kiosk)
            })
        }
    }

    // === Publisher functions ===

    /// TODO: better naming
    public fun create_allow_transfer_cap<T: key + store>(
        pub: &Publisher, ctx: &mut TxContext
    ): AllowTransferCap<T> {
        // TODO: consider "is_module"
        assert!(publisher::is_package<T>(pub), 0);
        AllowTransferCap { id: object::new(ctx) }
    }

    // === Place and take from the Kiosk ===

    /// Place any object into a Safe.
    /// Performs an authorization check to make sure only owner can do that.
    public fun place<T: key + store>(
        self: &mut Kiosk, cap: &Option<KioskOwnerCap>, item: T, ctx: &TxContext
    ) {
        try_access(self, cap, ctx);
        dof::add(&mut self.id, Key { id: object::id(&item) }, item)
    }

    /// Take any object from the Safe.
    /// Performs an authorization check to make sure only owner can do that.
    public fun take<T: key + store>(
        self: &mut Kiosk, id: ID, cap: &Option<KioskOwnerCap>, ctx: &TxContext
    ): T {
        try_access(self, cap, ctx);
        df::remove_if_exists<Offer, u64>(&mut self.id, Offer { id });
        dof::remove(&mut self.id, Key { id })
    }

    // === Trading functionality ===

    /// Make an offer by setting a price for the item and making it publicly
    /// purchasable by anyone on the network.
    ///
    /// Performs an authorization check to make sure only owner can sell.
    public fun make_offer<T: key + store>(
        self: &mut Kiosk, id: ID, price: u64, cap: &Option<KioskOwnerCap>, ctx: &TxContext
    ) {
        try_access(self, cap, ctx);
        df::add(&mut self.id, Offer { id }, price)
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

        assert!(price == coin::value(&payment), EIncorrectAmount);
        balance::join(&mut self.profits, coin::into_balance(payment));

        (inner, TransferRequest<T> {
            paid: price,
            from: object::id(self),
        })
    }

    /// Allow a `TransferRequest` for the type `T`. The call is protected
    /// by the type constraint, as only the publisher of the `T` can get
    /// `AllowTransferCap<T>`.
    ///
    /// Note: unless there's a policy for `T` to allow transfers,
    /// Kiosk trades will not be possible.
    public fun allow<T: key + store>(
        _cap: &AllowTransferCap<T>, req: TransferRequest<T>
    ): (u64, ID) {
        let TransferRequest { paid, from } = req;
        (paid, from)
    }

    /// Withdraw profits from the Kiosk.
    /// If `kiosk.owner` is set -> check for transaction sender.
    /// If `kiosk.owner` is none -> require capability.
    public fun withdraw<T: key + store>(
        self: &mut Kiosk, cap: &Option<KioskOwnerCap>, ctx: &mut TxContext
    ): Coin<SUI> {
        try_access(self, cap, ctx);

        let amount = balance::value(&self.profits);
        coin::take(&mut self.profits, amount, ctx)
    }

    /// Abort if credentials are incorrect and the party attempts to access the Kiosk.
    public fun try_access(self: &Kiosk, cap: &Option<KioskOwnerCap>, ctx: &TxContext) {
        assert!(
            (option::is_some(cap) && option::is_none(&self.owner)) ||
            (option::is_none(cap) && option::is_some(&self.owner))
        , EIncorrectArgument);

        if (option::is_some(&self.owner)) {
            assert!(*option::borrow(&self.owner) == sender(ctx), ENotOwner);
        } else {
            assert!(option::borrow(cap).for == object::id(self), ENotOwner);
        };
    }
}
