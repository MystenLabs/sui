// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::escrow;

use sui::balance::{Self, Balance};
use sui::coin::Coin;
use sui::event;
use sui::sui::SUI;

#[error]
const ENotRecipient: vector<u8> = b"Only the designated recipient can claim";
#[error]
const ENotSender: vector<u8> = b"Only the sender can cancel";
#[error]
const EWrongEscrowCap: vector<u8> = b"Capability does not match this escrow";

public struct SenderCap has key {
    id: UID,
    escrow_id: ID,
}

public struct RecipientCap has key {
    id: UID,
    escrow_id: ID,
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
public struct Escrow has key {
    id: UID,
    sender: address,
    recipient: address,
    balance: Balance<SUI>,
}

/// Create an escrow. The sender deposits funds.
public fun create(coin: Coin<SUI>, recipient: address, ctx: &mut TxContext): SenderCap {
    let escrow = Escrow {
        id: object::new(ctx),
        sender: ctx.sender(),
        recipient,
        balance: coin.into_balance(),
    };
    let escrow_id = object::id(&escrow);
    let recipient_cap = RecipientCap { id: object::new(ctx), escrow_id };
    transfer::transfer(recipient_cap, recipient);
    transfer::share_object(escrow);
    SenderCap { id: object::new(ctx), escrow_id }
}

/// Claim the escrow. Only the recipient can call this.
/// Pays out through address balances.
public fun claim(escrow: Escrow, cap: RecipientCap, ctx: &mut TxContext) {
    assert!(cap.escrow_id == object::id(&escrow), EWrongEscrowCap);
    let RecipientCap { id: cap_id, escrow_id: _ } = cap;
    cap_id.delete();
    let Escrow { id, sender: _, recipient, balance } = escrow;
    let amount = balance.value();
    balance::send_funds(balance, recipient);
    event::emit(EscrowClaimed { escrow_id: id.to_inner(), recipient, amount });
    id.delete();
}

/// Cancel the escrow and return funds. Only the sender can call this.
/// Returns funds through address balances.
public fun cancel(escrow: Escrow, cap: SenderCap, ctx: &mut TxContext) {
    assert!(cap.escrow_id == object::id(&escrow), EWrongEscrowCap);
    let SenderCap { id: cap_id, escrow_id: _ } = cap;
    cap_id.delete();
    let Escrow { id, sender, recipient: _, balance } = escrow;
    let amount = balance.value();
    balance::send_funds(balance, sender);
    event::emit(EscrowCancelled { escrow_id: id.to_inner(), sender, amount });
    id.delete();
}
// docs::/#escrow
