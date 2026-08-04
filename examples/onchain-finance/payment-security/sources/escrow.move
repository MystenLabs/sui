// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::escrow;

use sui::balance::{Self, Balance};
use sui::event;
use sui::sui::SUI;

#[error(code = 0)]
const EWrongEscrowCap: vector<u8> = b"Capability does not match this escrow";

public struct SenderCap has key, store {
    id: UID,
    escrow_id: ID,
}

public struct RecipientCap has key, store {
    id: UID,
    escrow_id: ID,
}

public struct EscrowCreated has copy, drop {
    escrow_id: ID,
    sender: address,
    recipient: address,
    amount: u64,
}

public struct EscrowClaimed has copy, drop {
    escrow_id: ID,
    recipient: address,
    amount: u64,
}

public struct EscrowCancelled has copy, drop {
    escrow_id: ID,
    sender: address,
    amount: u64,
}

// docs::#escrow
/// A shared escrow that is consumed on claim or cancellation, making settlement single-use.
public struct Escrow has key {
    id: UID,
    sender: address,
    recipient: address,
    balance: Balance<SUI>,
}

/// Lock funds, give each party a capability marker, and share the tracked escrow.
public fun create(payment: Balance<SUI>, recipient: address, ctx: &mut TxContext): SenderCap {
    let sender = ctx.sender();
    let amount = payment.value();
    let escrow = Escrow {
        id: object::new(ctx),
        sender,
        recipient,
        balance: payment,
    };
    let escrow_id = object::id(&escrow);
    let recipient_cap = RecipientCap { id: object::new(ctx), escrow_id };

    event::emit(EscrowCreated { escrow_id, sender, recipient, amount });
    transfer::public_transfer(recipient_cap, recipient);
    transfer::share_object(escrow);
    SenderCap { id: object::new(ctx), escrow_id }
}

/// Claim and consume the escrow and recipient capability exactly once.
public fun claim(escrow: Escrow, cap: RecipientCap) {
    assert!(cap.escrow_id == object::id(&escrow), EWrongEscrowCap);

    let RecipientCap { id: cap_id, .. } = cap;
    cap_id.delete();
    let Escrow { id, recipient, balance, .. } = escrow;
    let amount = balance.value();
    let escrow_id = id.to_inner();
    balance::send_funds(balance, recipient);
    event::emit(EscrowClaimed { escrow_id, recipient, amount });
    id.delete();
}

/// Cancel and consume the escrow and sender capability exactly once.
public fun cancel(escrow: Escrow, cap: SenderCap) {
    assert!(cap.escrow_id == object::id(&escrow), EWrongEscrowCap);

    let SenderCap { id: cap_id, .. } = cap;
    cap_id.delete();
    let Escrow { id, sender, balance, .. } = escrow;
    let amount = balance.value();
    let escrow_id = id.to_inner();
    balance::send_funds(balance, sender);
    event::emit(EscrowCancelled { escrow_id, sender, amount });
    id.delete();
}
// docs::/#escrow
