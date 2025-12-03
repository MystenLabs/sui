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
use sui::accumulator_settlement::EventStreamHead;

/// Emit a custom Move event, sending the data offchain.
///
/// Used for creating custom indexes and tracking onchain
/// activity in a way that suits a specific application the most.
///
/// The type `T` is the main way to index the event, and can contain
/// phantom parameters, eg `emit(MyEvent<phantom T>)`.
public native fun emit<T: copy + drop>(event: T);

/// Emits a custom Move event which can be authenticated by a light client.
///
/// This method emits the authenticated event to the event stream for the Move package that
/// defines the event type `T`.
/// Only the package that defines the type `T` can emit authenticated events to this stream.
public fun emit_authenticated<T: copy + drop>(event: T) {
    let stream_id = type_name::original_id<T>();
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
