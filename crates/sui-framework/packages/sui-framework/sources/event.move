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
use std::bcs;
use sui::hash;
use sui::dynamic_field;
use sui::accumulator;
use sui::address;

const ENotSystemAddress: u64 = 0;

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

public struct EventStreamHead has store {
    /// Merkle root for all events in the current checkpoint.
    root: vector<u8>,
    /// Hash of the previous version of the head object.
    prev: vector<u8>,
}

entry fun update_head(accumulator_root: &mut accumulator::Accumulator, stream_id: address, new_root: vector<u8>, ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    let name = accumulator::get_accumulator_field_name<EventStreamHead>(stream_id);
    let accumulator_root_id = accumulator_root.id();

    if (dynamic_field::exists_with_type<accumulator::Key, EventStreamHead>(accumulator_root_id, name)) {
        let head: &mut EventStreamHead = dynamic_field::borrow_mut(accumulator_root_id, name);
        let prev_bytes = bcs::to_bytes(head);
        let prev = hash::blake2b256(&prev_bytes);
        head.prev = prev;
        head.root = new_root;
    } else {
        let head = EventStreamHead {
            root: new_root,
            prev: address::to_bytes(address::from_u256(0)),
        };
        dynamic_field::add(accumulator_root_id, name, head);
    };
}


// TODO: Should EventStream have key?
public struct EventStream has store {
    // A unique identifier for the event stream.
    name: UID,
}

public fun new_event_stream(ctx: &mut TxContext): EventStream {
    EventStream {
        name: object::new(ctx),
    }
}

public fun destroy_stream(stream: EventStream) {
    let EventStream { name } = stream;
    name.delete();
}

public struct EventStreamCap has key, store {
    id: UID,
    stream_id: address,
}

public fun get_cap(stream: &EventStream, ctx: &mut TxContext): EventStreamCap {
    EventStreamCap {
        id: object::new(ctx),
        stream_id: stream.name.to_address(),
    }
}

public fun default_event_stream_cap<T: copy + drop>(ctx: &mut TxContext): EventStreamCap {
    EventStreamCap {
        id: object::new(ctx),
        stream_id: type_name::get_original_package_id<T>(),
    }
}

public fun destroy_cap(cap: EventStreamCap) {
    let EventStreamCap { id, .. } = cap;
    id.delete();
}

public fun emit_authenticated<T: copy + drop>(cap: &EventStreamCap, event: T) {
    let accumulator_addr = accumulator::get_accumulator_field_address<EventStreamHead>(cap.stream_id);

    emit_authenticated_impl<EventStreamHead, T>(accumulator_addr, cap.stream_id, event);
}

/// TODO: needs verifier rule like `emit` to ensure it is only called in package that defines `T`
/// Like `emit`, but also adds an on-chain committment to the event to the
/// stream `stream`.
native fun emit_authenticated_impl<StreamHeadT, T: copy + drop>(accumulator_id: address, stream: address, event: T);

#[test_only]
/// Get the total number of events emitted during execution so far
public native fun num_events(): u32;

#[test_only]
/// Get all events of type `T` emitted during execution.
/// Can only be used in testing,
public native fun events_by_type<T: copy + drop>(): vector<T>;
