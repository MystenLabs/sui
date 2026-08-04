// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::stream;

use sui::balance::{Self, Balance};
use sui::clock::Clock;
use sui::event;

#[error(code = 0)]
const EStreamNotStarted: vector<u8> = b"Stream has not started yet";
#[error(code = 1)]
const ENotRecipient: vector<u8> = b"Only the recipient can claim";
#[error(code = 2)]
const ENotSender: vector<u8> = b"Only the sender can cancel";
#[error(code = 3)]
const ENothingToClaim: vector<u8> = b"No claimable amount available";
#[error(code = 4)]
const EInvalidDuration: vector<u8> = b"End time must be after start time";

public struct StreamCreated<phantom T> has copy, drop {
    stream_id: ID,
    sender: address,
    recipient: address,
    total_amount: u64,
    start_time_ms: u64,
    end_time_ms: u64,
}

public struct StreamClaimed<phantom T> has copy, drop {
    stream_id: ID,
    recipient: address,
    amount: u64,
    claimed_to_date: u64,
}

public struct StreamCancelled<phantom T> has copy, drop {
    stream_id: ID,
    sender: address,
    recipient: address,
    paid_to_recipient: u64,
    refunded_to_sender: u64,
}

// docs::#stream
/// A linear vesting stream. The stream holds custody of the funds, so the
/// sender cannot spend them elsewhere once the stream is created.
public struct StreamPayment<phantom T> has key {
    id: UID,
    sender: address,
    recipient: address,
    total_amount: u64,
    claimed_amount: u64,
    start_time_ms: u64,
    end_time_ms: u64,
    funds: Balance<T>,
}

/// Create a new stream. Funds are locked until the recipient claims them.
public fun create<T>(
    funds: Balance<T>,
    recipient: address,
    start_time_ms: u64,
    end_time_ms: u64,
    ctx: &mut TxContext,
) {
    assert!(end_time_ms > start_time_ms, EInvalidDuration);

    let sender = ctx.sender();
    let total_amount = funds.value();
    let stream = StreamPayment<T> {
        id: object::new(ctx),
        sender,
        recipient,
        total_amount,
        claimed_amount: 0,
        start_time_ms,
        end_time_ms,
        funds,
    };

    event::emit(StreamCreated<T> {
        stream_id: object::id(&stream),
        sender,
        recipient,
        total_amount,
        start_time_ms,
        end_time_ms,
    });
    transfer::share_object(stream);
}

/// Amount vested as of `now_ms`, clamped to the stream window.
fun vested_amount<T>(stream: &StreamPayment<T>, now_ms: u64): u64 {
    if (now_ms <= stream.start_time_ms) {
        0
    } else if (now_ms >= stream.end_time_ms) {
        stream.total_amount
    } else {
        let elapsed = now_ms - stream.start_time_ms;
        let duration = stream.end_time_ms - stream.start_time_ms;
        (((stream.total_amount as u128) * (elapsed as u128) / (duration as u128)) as u64)
    }
}

/// Claim the vested-but-unclaimed portion into the recipient's address balance.
public fun claim<T>(stream: &mut StreamPayment<T>, clock: &Clock, ctx: &TxContext) {
    assert!(ctx.sender() == stream.recipient, ENotRecipient);

    let now = clock.timestamp_ms();
    assert!(now >= stream.start_time_ms, EStreamNotStarted);

    let claimable = stream.vested_amount(now) - stream.claimed_amount;
    assert!(claimable > 0, ENothingToClaim);

    stream.claimed_amount = stream.claimed_amount + claimable;
    balance::send_funds(stream.funds.split(claimable), stream.recipient);
    event::emit(StreamClaimed<T> {
        stream_id: object::id(stream),
        recipient: stream.recipient,
        amount: claimable,
        claimed_to_date: stream.claimed_amount,
    });
}

/// Cancel the stream. Vested-but-unclaimed funds go to the recipient;
/// the unvested remainder returns to the sender.
public fun cancel<T>(stream: StreamPayment<T>, clock: &Clock, ctx: &TxContext) {
    assert!(ctx.sender() == stream.sender, ENotSender);

    let owed_to_recipient = stream.vested_amount(clock.timestamp_ms()) - stream.claimed_amount;

    let StreamPayment { id, sender, recipient, mut funds, .. } = stream;
    let stream_id = id.to_inner();

    if (owed_to_recipient > 0) {
        balance::send_funds(funds.split(owed_to_recipient), recipient);
    };

    let refunded_to_sender = funds.value();
    if (refunded_to_sender > 0) {
        balance::send_funds(funds, sender);
    } else {
        funds.destroy_zero();
    };

    event::emit(StreamCancelled<T> {
        stream_id,
        sender,
        recipient,
        paid_to_recipient: owed_to_recipient,
        refunded_to_sender,
    });
    id.delete();
}
// docs::/#stream
