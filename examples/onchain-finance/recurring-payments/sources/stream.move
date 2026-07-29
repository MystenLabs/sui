// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#stream
module example::stream;

use sui::balance::Balance;
use sui::clock::Clock;
use sui::coin::{Self, Coin};
use sui::sui::SUI;

#[error]
const EStreamNotStarted: vector<u8> = b"Stream has not started yet";
#[error]
const ENotRecipient: vector<u8> = b"Only the recipient can claim";
#[error]
const ENothingToClaim: vector<u8> = b"No claimable amount available";

public struct StreamPayment has key {
    id: UID,
    sender: address,
    recipient: address,
    total_amount: u64,
    claimed_amount: u64,
    start_time_ms: u64,
    end_time_ms: u64,
    balance: Balance<SUI>,
}

#[error]
const EInvalidDuration: vector<u8> = b"End time must be after start time";

/// Create a new stream. Funds are locked until the recipient claims them.
public fun create(
    coin: Coin<SUI>,
    recipient: address,
    start_time_ms: u64,
    end_time_ms: u64,
    ctx: &mut TxContext,
) {
    assert!(end_time_ms > start_time_ms, EInvalidDuration);
    let total = coin.value();
    let stream = StreamPayment {
        id: object::new(ctx),
        sender: ctx.sender(),
        recipient,
        total_amount: total,
        claimed_amount: 0,
        start_time_ms,
        end_time_ms,
        balance: coin.into_balance(),
    };
    transfer::share_object(stream);
}

/// Claim the unlocked portion of the stream.
public fun claim(stream: &mut StreamPayment, clock: &Clock, ctx: &mut TxContext): Coin<SUI> {
    assert!(ctx.sender() == stream.recipient, ENotRecipient);

    let now = clock.timestamp_ms();
    assert!(now >= stream.start_time_ms, EStreamNotStarted);

    let elapsed = if (now >= stream.end_time_ms) {
        stream.end_time_ms - stream.start_time_ms
    } else {
        now - stream.start_time_ms
    };

    let total_duration = stream.end_time_ms - stream.start_time_ms;
    let vested = (stream.total_amount as u128) * (elapsed as u128) / (total_duration as u128);
    let claimable = (vested as u64) - stream.claimed_amount;

    assert!(claimable > 0, ENothingToClaim);

    stream.claimed_amount = stream.claimed_amount + claimable;
    coin::from_balance(stream.balance.split(claimable), ctx)
}

/// Cancel the stream. Vested-but-unclaimed funds go to the recipient;
/// unvested funds return to the sender.
public fun cancel(stream: StreamPayment, clock: &Clock, ctx: &mut TxContext) {
    assert!(ctx.sender() == stream.sender, ENotRecipient);

    let now = clock.timestamp_ms();
    let elapsed = if (now >= stream.end_time_ms) {
        stream.end_time_ms - stream.start_time_ms
    } else if (now > stream.start_time_ms) {
        now - stream.start_time_ms
    } else {
        0
    };

    let total_duration = stream.end_time_ms - stream.start_time_ms;
    let vested = (stream.total_amount as u128) * (elapsed as u128) / (total_duration as u128);
    let owed_to_recipient = (vested as u64) - stream.claimed_amount;

    let StreamPayment { id, sender, recipient, mut balance, .. } = stream;

    // Transfer vested-but-unclaimed funds to recipient
    if (owed_to_recipient > 0) {
        let recipient_coin = coin::from_balance(balance.split(owed_to_recipient), ctx);
        transfer::public_transfer(recipient_coin, recipient);
    };

    // Return unvested remainder to sender
    if (balance.value() > 0) {
        let sender_coin = coin::from_balance(balance, ctx);
        transfer::public_transfer(sender_coin, sender);
    } else {
        balance.destroy_zero();
    };

    id.delete();
}
// docs::/#stream
