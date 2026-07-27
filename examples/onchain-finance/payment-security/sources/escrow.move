// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#escrow
module example::escrow;

use sui::coin::Coin;
use sui::sui::SUI;

const ENotRecipient: u64 = 0;
const ENotSender: u64 = 1;

public struct Escrow has key {
    id: UID,
    sender: address,
    recipient: address,
    balance: Coin<SUI>,
}

/// Create an escrow. The sender deposits funds.
public fun create(coin: Coin<SUI>, recipient: address, ctx: &mut TxContext) {
    let escrow = Escrow {
        id: object::new(ctx),
        sender: ctx.sender(),
        recipient,
        balance: coin,
    };
    transfer::share_object(escrow);
}

/// Claim the escrow. Only the recipient can call this.
public fun claim(escrow: Escrow, ctx: &TxContext) {
    assert!(ctx.sender() == escrow.recipient, ENotRecipient);
    let Escrow { id, sender: _, recipient: _, balance } = escrow;
    transfer::public_transfer(balance, ctx.sender());
    id.delete();
}

/// Cancel the escrow and return funds. Only the sender can call this.
public fun cancel(escrow: Escrow, ctx: &TxContext) {
    assert!(ctx.sender() == escrow.sender, ENotSender);
    let Escrow { id, sender, recipient: _, balance } = escrow;
    transfer::public_transfer(balance, sender);
    id.delete();
}
// docs::/#escrow
