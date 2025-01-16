// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[lint_allow(coin_field)]
module common::identified_payment;

use sui::{coin::{Self, Coin}, dynamic_field, event, sui::SUI, transfer::Receiving};

const ENotEarmarkedForSender: u64 = 0;

/// An `IdentifiedPayment` is an object that represents a payment that has
/// been made for a specific good or service that is identified by a
/// `payment_id` that is unique to the good or service provided for the customer.
/// NB: This has the `store` ability to allow the `make_shared_payment`
///     function. Without this `IdentifiedPayment` could be `key` only and
///     custom transfer and receiving rules can be written for it.
public struct IdentifiedPayment has key, store {
    id: UID,
    payment_id: u64,
    coin: Coin<SUI>,
}

/// An `EarmarkedPayment` payment is an `IdentifiedPayment` that is
/// earmarked for a specific address. E.g., in the restaurant example, you
/// may tip your serverd with an `EarmarkedPayment` which will ensure that only
/// server that you specified in your tip can receive it.
/// Since this object is `key` only it can only be transferred and
/// received by functions defined in this module.
public struct EarmarkedPayment has key {
    id: UID,
    payment: IdentifiedPayment,
    `for`: address,
}

/// Event emitted when a payment is made. This contains the `payment_id`
/// that the payment is being made for, the `payment_amount` that is being made,
/// and the `originator` of the payment.
public struct SentPaymentEvent has copy, drop {
    payment_id: u64,
    paid_to: address,
    payment_amount: u64,
    originator: address,
}

/// Event emitted when a payment is processed. This contains the
/// `payment_id` of the payment, and the amount processed.
public struct ProcessedPaymentEvent has copy, drop {
    payment_id: u64,
    payment_amount: u64,
}

/// Make a payment with the given payment ID to the provided `to` address.
/// Will create an `IdentifiedPayment` object that can be unpacked by the
/// recipient, and also emits an event.
public fun make_payment(payment_id: u64, coin: Coin<SUI>, to: address, ctx: &mut TxContext) {
    let payment_amount = coin::value(&coin);
    let identified_payment = IdentifiedPayment {
        id: object::new(ctx),
        payment_id,
        coin,
    };
    event::emit(SentPaymentEvent {
        payment_id,
        paid_to: to,
        payment_amount,
        originator: tx_context::sender(ctx),
    });
    transfer::transfer(identified_payment, to);
}

/// Only needed for the non transfer-to-object-based cash register.
public fun make_shared_payment(
    register_uid: &mut UID,
    payment_id: u64,
    coin: Coin<SUI>,
    ctx: &mut TxContext,
) {
    let payment_amount = coin::value(&coin);
    let identified_payment = IdentifiedPayment {
        id: object::new(ctx),
        payment_id,
        coin,
    };
    event::emit(SentPaymentEvent {
        payment_id,
        paid_to: object::uid_to_address(register_uid),
        payment_amount,
        originator: tx_context::sender(ctx),
    });
    dynamic_field::add(register_uid, payment_id, identified_payment)
}

/// Process an `IdentifiedPayment` payment returning back the payments ID,
/// along with the coin that was sent in the payment.
public fun unpack(identified_payment: IdentifiedPayment): (u64, Coin<SUI>) {
    let IdentifiedPayment { id, payment_id, coin } = identified_payment;
    object::delete(id);
    event::emit(ProcessedPaymentEvent {
        payment_id,
        payment_amount: coin::value(&coin),
    });
    (payment_id, coin)
}

//---------------------------------------------------------------------------
// Functions for `EarmarkedPayment`s
//---------------------------------------------------------------------------

/// Custom transfer rule for `EarmarkedPayment` payments -- anyone can transfer them.
public fun transfer(earmarked: EarmarkedPayment, to: address) {
    transfer::transfer(earmarked, to);
}

/// An example of a custom receiving rule -- this behaves in a similar manner
/// to custom transfer rules: if the object is `key` only , the
/// `sui::transfer::receive` function can only be called on the object from
/// within the same module that defined that object.
///
/// In this case `EarmarkedPayment` is defined with `key` only, so this is
/// defining a custom receive rule that specifies that only `for` can receive
/// the payment no matter what object it was sent to.
public fun receive(
    parent: &mut UID,
    ticket: Receiving<EarmarkedPayment>,
    ctx: &TxContext,
): IdentifiedPayment {
    let EarmarkedPayment { id, payment, `for` } = transfer::receive(parent, ticket);
    assert!(tx_context::sender(ctx) == `for`, ENotEarmarkedForSender);
    object::delete(id);
    payment
}
