// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

///
module sui::kiosk {
    use sui::object::{Self, UID, ID};
    use sui::dynamic_object_field as dof;
    use sui::dynamic_field as df;
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;

    use std::option::Option;

    /// A Hot Potato making sure the buyer gets an authorization
    /// from the owner of the T to perform a transfer after a purchase.
    ///
    /// Contains the amount paid for the `T` so the commission could be
    /// calculated; `from` field contains the seller of the `T`.
    struct TransferRequest<T: key + store> {
        inner: T,
        paid: u64,
        from: Option<address>
    }

    /// A unique capability that allows owner of the `T` to authorize
    /// transfers. Can only be created with the `Publisher` object.
    struct AcceptTransferCap<phantom T: key + store> has key, store {
        id: UID
    }

    /// An object that stores collectibles of all sorts.
    /// For sale, for collecting reasons, for fun.
    struct Kiosk has key, store {
        id: UID,
        profits: Balance<SUI>,
        owner: Option<address>
    }

    /// Custom key for the items placed into the kiosk.
    struct Key has store, copy, drop {
        id: ID
    }

    /// An active offer to purchase the T.
    struct Offer has store, copy, drop {
        id: ID
    }

    public fun place<T: key + store>(self: &mut Kiosk, item: T) {
        dof::add(&mut self.id, Key { id: object::id(&item) }, item)
    }

    public fun make_offer<T: key + store>(self: &mut Kiosk, item_id: ID, price: u64) {
        df::add(&mut self.id, Offer { id: item_id }, price)
    }

    public fun take<T: key + store>(self: &mut Kiosk, item: T) {
        // dof::< bee boo >
    }

    public fun purchase<T: key + store>(self: &mut Kiosk, id: ID, payment: Coin<SUI>): TransferRequest<T> {
        let price = df::remove<Offer, u64>(&mut self.id, Offer { id });
        let inner = dof::remove<Key, T>(&mut self.id, Key { id });

        assert!(price == coin::value(&payment), 0);
        balance::join(&mut self.profits, coin::into_balance(payment));

        TransferRequest {
            inner,
            paid: price,
            from: self.owner
        }
    }

    public fun allow<T: key + store>(cap: &AcceptTransferCap<T>, req: TransferRequest<T>): (T, u64, Option<address>) {
        let TransferRequest { inner, paid, from } = req;
        (inner, paid, from)
    }
}
