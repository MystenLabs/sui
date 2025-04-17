// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module shared_with_tto::shared_cash_register;

use common::identified_payment::{Self, IdentifiedPayment, EarmarkedPayment};
use std::string::String;
use sui::{coin::Coin, sui::SUI, transfer::Receiving, vec_set::{Self, VecSet}};

const EInvalidOwner: u64 = 0;
const ENotAuthorized: u64 = 2;

public struct CashRegister has key {
    id: UID,
    authorized_individuals: VecSet<address>,
    business_name: String,
    register_owner: address,
}

/// Create a cash register for the business to use with an initial owner,
/// business name, and authorized set of individuals that can process
/// payments.
public fun create_cash_register(
    mut authorized_individuals_vec: vector<address>,
    business_name: String,
    ctx: &mut TxContext,
) {
    let mut authorized_individuals = vec_set::empty();

    while (!vector::is_empty(&authorized_individuals_vec)) {
        let addr = vector::pop_back(&mut authorized_individuals_vec);
        vec_set::insert(&mut authorized_individuals, addr);
    };

    let register = CashRegister {
        id: object::new(ctx),
        authorized_individuals,
        business_name,
        register_owner: tx_context::sender(ctx),
    };
    transfer::share_object(register);
}

/// Transfer the ownership of this cash register to a new owner.
public fun transfer_cash_register_ownership(
    register: &mut CashRegister,
    new_owner: address,
    ctx: &TxContext,
) {
    assert!(register.register_owner == tx_context::sender(ctx), EInvalidOwner);
    register.register_owner = new_owner;
}

/// Update the business name associated with the cash register.
public fun update_business_name(register: &mut CashRegister, new_name: String, ctx: &TxContext) {
    assert!(register.register_owner == tx_context::sender(ctx), EInvalidOwner);
    register.business_name = new_name;
}

/// Add or remove an auhorized individual to the cash register. If removing them they must be in the set of authorized individuals.
public fun update_authorized_individuals(
    register: &mut CashRegister,
    addr: address,
    add_or_remove: bool,
    ctx: &TxContext,
) {
    assert!(register.register_owner == tx_context::sender(ctx), EInvalidOwner);
    if (add_or_remove) {
        assert!(vec_set::contains(&register.authorized_individuals, &addr), ENotAuthorized);
        vec_set::remove(&mut register.authorized_individuals, &addr);
    } else {
        vec_set::insert(&mut register.authorized_individuals, addr);
    }
}

//--------------------------------------------------------------------------------
// Changes from here down only -- the rest is the same as in the previous example.
//--------------------------------------------------------------------------------

/// Process a payment that has been made, removing it from the register and
/// returning the coin that can then be combined or sent elsewhere by the authorized individual.
/// Payments can only be processed by either an account in the `authorized_individuals` set or by the owner of the cash register.
public fun process_payment(
    register: &mut CashRegister,
    payment_ticket: Receiving<IdentifiedPayment>,
    ctx: &TxContext,
): Coin<SUI> {
    let sender = tx_context::sender(ctx);
    assert!(
        vec_set::contains(&register.authorized_individuals, &sender) || sender == register.register_owner,
        ENotAuthorized,
    );
    let payment: IdentifiedPayment = transfer::public_receive(&mut register.id, payment_ticket);
    let (_, coin) = identified_payment::unpack(payment);
    coin
}

/// Process a tip -- only the person who was tipped can process it despite it being sent to the shared object.
public fun process_tip(
    register: &mut CashRegister,
    earmarked_ticket: Receiving<EarmarkedPayment>,
    ctx: &TxContext,
): Coin<SUI> {
    let payment: IdentifiedPayment = identified_payment::receive(
        &mut register.id,
        earmarked_ticket,
        ctx,
    );
    let (_, coin) = identified_payment::unpack(payment);
    coin
}

// NB: The `pay` function from the previous example is now gone! They can
// now just use `identified_payment::make_payment` as the shared
// `CashRegister` object does not need to be a part of the transaction.
