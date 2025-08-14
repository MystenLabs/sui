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
use sui::dynamic_field;
use sui::accumulator;
use sui::hash;
use sui::bcs;

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
public use fun new_cap as EventStream.get_cap;

#[allow(unused_field)]
public struct EventStreamHead has store {
    /// Merkle Mountain Range of all events in the stream.
    mmr: vector<vector<u8>>,
    /// Checkpoint sequence number at which the event stream was written.
    checkpoint_seq: u64,
    /// Number of events in the stream.
    num_events: u64,
}

/// Initialize an empty Merkle Mountain Range
fun init_mmr(): vector<vector<u8>> {
    vector::empty()
}

/// Add a new value to the MMR using the standard MMR append algorithm
fun add_to_mmr(new_val: vector<u8>, mmr: &mut vector<vector<u8>>) {
    let mut i = 0;
    let mut cur = new_val;
    
    while (i < vector::length(mmr)) {
        let r = vector::borrow_mut(mmr, i);
        if (vector::is_empty(r)) {
            *r = cur;
            return
        } else {
            cur = hash_two_to_one_via_bcs(*r, cur);
            *r = vector::empty();
        };
        i = i + 1;
    };

    // Vector length insufficient. Increase by 1.
    vector::push_back(mmr, cur);
}

/// Hash two values together using BCS serialization and Blake2b
fun hash_two_to_one_via_bcs(left: vector<u8>, right: vector<u8>): vector<u8> {
    let left_bytes = bcs::to_bytes(&left);
    let right_bytes = bcs::to_bytes(&right);
    let mut concatenated = left_bytes;
    vector::append(&mut concatenated, right_bytes);
    hash::blake2b256(&concatenated)
}

entry fun update_head(accumulator_root: &mut accumulator::AccumulatorRoot, stream_id: address, new_root: vector<u8>, event_count_delta: u64, checkpoint_seq: u64, ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    let name = accumulator::accumulator_key<EventStreamHead>(stream_id);
    if (dynamic_field::exists_with_type<accumulator::Key<EventStreamHead>, EventStreamHead>(accumulator_root.id(), copy name)) {
        let head: &mut EventStreamHead = dynamic_field::borrow_mut<accumulator::Key<EventStreamHead>, EventStreamHead>(accumulator_root.id_mut(), name);
        add_to_mmr(new_root, &mut head.mmr);
        head.num_events = head.num_events + event_count_delta;
        head.checkpoint_seq = checkpoint_seq;
    } else {
        let mut initial_mmr = init_mmr();
        add_to_mmr(new_root, &mut initial_mmr);
        let head = EventStreamHead {
            mmr: initial_mmr,
            checkpoint_seq: checkpoint_seq,
            num_events: event_count_delta,
        };
        dynamic_field::add<accumulator::Key<EventStreamHead>, EventStreamHead>(accumulator_root.id_mut(), name, head);
    };
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

#[test]
fun test_mmr_addition() {
    let mut mmr = init_mmr();
    let fixed_new_val = vector::tabulate!(32, |_| 2);

    // Initial MMR should be empty
    assert!(vector::all!(&mmr, |x| vector::is_empty(x)));

    // Round 1: Add first element - should be at position 0
    add_to_mmr(fixed_new_val, &mut mmr);
    assert!(vector::map_ref!(&mmr, |x| vector::is_empty(x)) == 
            vector[false]);

    // Round 2: Add second element - should trigger merge and clear position 0
    add_to_mmr(fixed_new_val, &mut mmr);
    assert!(vector::map_ref!(&mmr, |x| vector::is_empty(x)) == 
            vector[true, false]);

    // Round 3: Add third element - should place at position 0
    add_to_mmr(fixed_new_val, &mut mmr);
    assert!(vector::map_ref!(&mmr, |x| vector::is_empty(x)) == 
            vector[false, false]);

    // Round 4: Add fourth element - should trigger cascade merge to position 2
    add_to_mmr(fixed_new_val, &mut mmr);
    assert!(vector::map_ref!(&mmr, |x| vector::is_empty(x)) == 
            vector[true, true, false]);

    // Verify the final hash represents all 4 elements
    let x = hash_two_to_one_via_bcs(fixed_new_val, fixed_new_val);
    let y = hash_two_to_one_via_bcs(x, x);
    assert!(mmr[2] == y);
}

#[test]
fun test_mmr_with_different_values() {
    let mut mmr = init_mmr();
    
    // Create different values like we would get from different events  
    let val1 = vector[150, 121, 52, 32]; // Similar to what we see in test output
    let val2 = vector[42, 233, 246, 96];  // Different hash
    let val3 = vector[148, 79, 131, 129]; // Different hash  
    let val4 = vector[50, 231, 238, 28];  // Different hash

    // Verify these values are actually different
    assert!(val1 != val2);
    assert!(val2 != val3);
    assert!(val3 != val4);

    // Add them one by one and verify MMR behavior
    add_to_mmr(val1, &mut mmr);
    add_to_mmr(val2, &mut mmr);  
    // After second add, position 0 should be empty, position 1 should have merged hash
    assert!(vector::is_empty(&mmr[0]));
    assert!(!vector::is_empty(&mmr[1]));

    add_to_mmr(val3, &mut mmr);
    // Position 0 should now have val3, position 1 should still have the merged hash
    assert!(!vector::is_empty(&mmr[0]));
    assert!(!vector::is_empty(&mmr[1]));

    add_to_mmr(val4, &mut mmr);
    // Final state: positions 0,1 empty, position 2 has the final merged hash
    assert!(vector::is_empty(&mmr[0]));
    assert!(vector::is_empty(&mmr[1]));
    assert!(!vector::is_empty(&mmr[2]));
}
