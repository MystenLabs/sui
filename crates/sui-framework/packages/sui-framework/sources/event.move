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

// TODO: Should EventStream have key?
public struct EventStream has store {
    // A unique identifier for the event stream.
    name: UID,
}

public fun destroy_stream(stream: EventStream) {
    let EventStream { name } = stream;
    name.delete();
}


public fun new_event_stream(ctx: &mut TxContext): EventStream {
    EventStream {
        name: object::new(ctx),
    }
}

public fun get_cap(stream: &EventStream, ctx: &mut TxContext): EventStreamCap {
    EventStreamCap {
        id: object::new(ctx),
        stream_id: stream.name.to_address(),
    }
}

public struct EventStreamCap has key, store {
    id: UID,
    stream_id: address,
}

public fun default_event_stream_cap<T: copy + drop>(ctx: &mut TxContext): EventStreamCap {
    EventStreamCap {
        id: object::new(ctx),
        stream_id: type_name::get_original_package_id<T>(),
    }
}

public fun emit_authenticated<T: copy + drop>(cap: &EventStreamCap, event: T) {
    emit_authenticated_impl(cap.stream_id, event);
}

public fun destroy_cap(cap: EventStreamCap) {
    let EventStreamCap { id, .. } = cap;
    id.delete();
}

/// TODO: needs verifier rule like `emit` to ensure it is only called in package that defines `T`
/// Like `emit`, but also adds an on-chain committment to the event to the
/// stream `stream`.
public native fun emit_authenticated_impl<T: copy + drop>(stream: address, event: T);

#[test_only]
/// Get the total number of events emitted during execution so far
public native fun num_events(): u32;

#[test_only]
/// Get all events of type `T` emitted during execution.
/// Can only be used in testing,
public native fun events_by_type<T: copy + drop>(): vector<T>;
