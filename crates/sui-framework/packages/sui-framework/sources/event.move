// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Events module. Defines the `sui::event::emit` function which
/// creates and sends a custom MoveEvent as a part of the effects
/// certificate of the transaction.
///
/// Every MoveEvent has the following properties:
///  - sender
///  - type signature (`T`)
///  - event data (the value of `T`)
///  - timestamp (local to a node)
///  - transaction digest
///
/// Example:
/// ```
/// module my::marketplace {
///    use sui::event;
///    /* ... */
///    struct ItemPurchased has copy, drop {
///      item_id: ID, buyer: address
///    }
///    entry fun buy(/* .... */) {
///       /* ... */
///       event::emit(ItemPurchased { item_id: ..., buyer: .... })
///    }
/// }
/// ```
module sui::event;

use std::type_name;
use sui::accumulator;

/// Emit a custom Move event, sending the data offchain.
///
/// Used for creating custom indexes and tracking onchain
/// activity in a way that suits a specific application the most.
///
/// The type `T` is the main way to index the event, and can contain
/// phantom parameters, eg `emit(MyEvent<phantom T>)`.
public native fun emit<T: copy + drop>(event: T);

public use fun destroy_stream as EventStream.destroy;
public use fun destroy_cap as EventStreamCap.destroy;
public use fun emit_authenticated as EventStreamCap.emit;

#[allow(unused_field)]
public struct EventStreamHead has store {
    /// Merkle Mountain Range of all events in the stream.
    mmr: vector<vector<u8>>,
    /// Checkpoint sequence number at which the event stream was written.
    checkpoint_seq: u64,
    /// Number of events in the stream.
    num_events: u64,
}

// A unique identifier for an event stream.
public struct EventStream has key, store {
    id: UID,
}

public fun new_event_stream(ctx: &mut TxContext): EventStream {
    EventStream {
        id: object::new(ctx),
    }
}

public fun destroy_stream(stream: EventStream) {
    let EventStream { id } = stream;
    id.delete();
}

/// A capability to write to a given event stream. 
public struct EventStreamCap has key, store {
    id: UID,
    stream_id: address,
}

public fun new_cap(stream: &EventStream, ctx: &mut TxContext): EventStreamCap {
    EventStreamCap {
        id: object::new(ctx),
        stream_id: stream.id.to_address(),
    }
}

public fun destroy_cap(cap: EventStreamCap) {
    let EventStreamCap { id, .. } = cap;
    id.delete();
}

public fun emit_authenticated<T: copy + drop>(cap: &EventStreamCap, event: T) {
    let accumulator_addr = accumulator::accumulator_address<EventStreamHead>(cap.stream_id);
    emit_authenticated_impl<EventStreamHead, T>(accumulator_addr, cap.stream_id, event);
}

public fun emit_authenticated_default<T: copy + drop>(event: T) {
    let stream_id = type_name::original_package_id<T>();
    let accumulator_addr = accumulator::accumulator_address<EventStreamHead>(stream_id);
    emit_authenticated_impl<EventStreamHead, T>(accumulator_addr, stream_id, event);
}

native fun emit_authenticated_impl<StreamHeadT, T: copy + drop>(
    accumulator_id: address,
    stream: address,
    event: T,
);

#[test_only]
/// Get the total number of events emitted during execution so far
public native fun num_events(): u32;

#[test_only]
/// Get all events of type `T` emitted during execution.
/// Can only be used in testing,
public native fun events_by_type<T: copy + drop>(): vector<T>;
