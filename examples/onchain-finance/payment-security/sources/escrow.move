// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::escrow;

use sui::balance::{Self, Balance};
use sui::coin::{Self, Coin};
use sui::event;
use sui::sui::SUI;

#[error]
const ENotRecipient: vector<u8> = b"Only the designated recipient can claim";
#[error]
const ENotSender: vector<u8> = b"Only the sender can cancel";

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
public fun create(coin: Coin<SUI>, recipient: address, ctx: &mut TxContext) {
    let escrow = Escrow {
        id: object::new(ctx),
        sender: ctx.sender(),
        recipient,
        balance: coin.into_balance(),
    };
    transfer::share_object(escrow);
}

/// Claim the escrow. Only the recipient can call this.
public fun claim(escrow: Escrow, ctx: &mut TxContext) {
    assert!(ctx.sender() == escrow.recipient, ENotRecipient);
    let Escrow { id, sender: _, recipient, balance } = escrow;
    let amount = balance.value();
    let coin = coin::from_balance(balance, ctx);
    transfer::public_transfer(coin, recipient);
    event::emit(EscrowClaimed { escrow_id: id.to_inner(), recipient, amount });
    id.delete();
}

/// Cancel the escrow and return funds. Only the sender can call this.
public fun cancel(escrow: Escrow, ctx: &mut TxContext) {
    assert!(ctx.sender() == escrow.sender, ENotSender);
    let Escrow { id, sender, recipient: _, balance } = escrow;
    let amount = balance.value();
    let coin = coin::from_balance(balance, ctx);
    transfer::public_transfer(coin, sender);
    event::emit(EscrowCancelled { escrow_id: id.to_inner(), sender, amount });
    id.delete();
}
// docs::/#escrow
