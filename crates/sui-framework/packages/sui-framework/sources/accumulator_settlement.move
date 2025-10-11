// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::accumulator_settlement;

use sui::accumulator::{AccumulatorRoot, accumulator_key, U128, create_u128, destroy_u128};
use sui::bcs;
use sui::hash;

const ENotSystemAddress: u64 = 0;
const EInvalidSplitAmount: u64 = 1;

use fun sui::accumulator_metadata::remove_accumulator_metadata as AccumulatorRoot.remove_metadata;
use fun sui::accumulator_metadata::create_accumulator_metadata as AccumulatorRoot.create_metadata;

// === Settlement storage types and entry points ===

/// Called by settlement transactions to ensure that the settlement transaction has a unique
/// digest.
#[allow(unused_function)]
fun settlement_prologue(
    _accumulator_root: &mut AccumulatorRoot,
    _epoch: u64,
    _checkpoint_height: u64,
    _idx: u64,
    // Total input sui received from user transactions
    input_sui: u64,
    // Total output sui withdrawn by user transactions
    output_sui: u64,
    ctx: &TxContext,
) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    record_settlement_sui_conservation(input_sui, output_sui);
}

#[allow(unused_function)]
fun settle_u128<T>(
    accumulator_root: &mut AccumulatorRoot,
    owner: address,
    merge: u128,
    split: u128,
    ctx: &mut TxContext,
) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    // Merge and split should be netted out prior to calling this function.
    assert!((merge == 0 ) != (split == 0), EInvalidSplitAmount);

    let name = accumulator_key<T>(owner);

    if (accumulator_root.has_accumulator<T, U128>(name)) {
        let is_zero = {
            let value: &mut U128 = accumulator_root.borrow_accumulator_mut(name);
            value.update(merge, split);
            value.is_zero()
        };

        if (is_zero) {
            let value = accumulator_root.remove_accumulator<T, U128>(name);
            destroy_u128(value);
            accumulator_root.remove_metadata<T>(owner);
        }
    } else {
        // cannot split if the field does not yet exist
        assert!(split == 0, EInvalidSplitAmount);
        let value = create_u128(merge);

        accumulator_root.add_accumulator(name, value);
        accumulator_root.create_metadata<T>(owner, ctx);
    };
}

/// Called by the settlement transaction to track conservation of SUI.
native fun record_settlement_sui_conservation(input_sui: u64, output_sui: u64);

#[allow(unused_field)]
public struct EventStreamHead has store {
    /// Merkle Mountain Range of all events in the stream.
    mmr: vector<u256>,
    /// Checkpoint sequence number at which the event stream was written.
    checkpoint_seq: u64,
    /// Number of events in the stream.
    num_events: u64,
}

fun add_to_mmr(new_val: u256, mmr: &mut vector<u256>) {
    let mut i = 0;
    let mut cur = new_val;

    while (i < vector::length(mmr)) {
        let r = vector::borrow_mut(mmr, i);
        if (*r == 0) {
            *r = cur;
            return
        } else {
            cur = hash_two_to_one_u256(*r, cur);
            *r = 0;
        };
        i = i + 1;
    };

    // Vector length insufficient. Increase by 1.
    vector::push_back(mmr, cur);
}

fun u256_from_bytes(bytes: vector<u8>): u256 {
    bcs::new(bytes).peel_u256()
}

fun hash_two_to_one_u256(left: u256, right: u256): u256 {
    let left_bytes = bcs::to_bytes(&left);
    let right_bytes = bcs::to_bytes(&right);
    let mut concatenated = left_bytes;
    vector::append(&mut concatenated, right_bytes);
    u256_from_bytes(hash::blake2b256(&concatenated))
}

fun new_stream_head(new_root: u256, event_count_delta: u64, checkpoint_seq: u64): EventStreamHead {
    let mut initial_mmr = vector::empty();
    add_to_mmr(new_root, &mut initial_mmr);
    EventStreamHead {
        mmr: initial_mmr,
        checkpoint_seq: checkpoint_seq,
        num_events: event_count_delta,
    }
}

#[allow(unused_function)]
fun settle_events(
    accumulator_root: &mut AccumulatorRoot,
    stream_id: address,
    new_root: u256,
    event_count_delta: u64,
    checkpoint_seq: u64,
    ctx: &TxContext,
) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    let name = accumulator_key<EventStreamHead>(stream_id);
    if (accumulator_root.has_accumulator<EventStreamHead, EventStreamHead>(copy name)) {
        let head: &mut EventStreamHead = accumulator_root.borrow_accumulator_mut(name);
        add_to_mmr(new_root, &mut head.mmr);
        head.num_events = head.num_events + event_count_delta;
        head.checkpoint_seq = checkpoint_seq;
    } else {
        let head = new_stream_head(new_root, event_count_delta, checkpoint_seq);
        accumulator_root.add_accumulator(name, head);
    };
}

#[test]
fun test_mmr_addition() {
    let mut mmr = vector::empty();
    let fixed_leaf: u256 = 2;

    // Initial MMR should be empty
    assert!(vector::all!(&mmr, |x| *x == 0));

    // Round 1: Add first element - should be at position 0
    add_to_mmr(fixed_leaf, &mut mmr);
    assert!(vector::map_ref!(&mmr, |x| *x == 0) == 
            vector[false]);

    // Round 2: Add second element - should trigger merge and clear position 0
    add_to_mmr(fixed_leaf, &mut mmr);
    assert!(vector::map_ref!(&mmr, |x| *x == 0) == 
            vector[true, false]);

    // Round 3: Add third element - should place at position 0
    add_to_mmr(fixed_leaf, &mut mmr);
    assert!(vector::map_ref!(&mmr, |x| *x == 0) == 
            vector[false, false]);

    // Round 4: Add fourth element - should trigger cascade merge to position 2
    add_to_mmr(fixed_leaf, &mut mmr);
    assert!(vector::map_ref!(&mmr, |x| *x == 0) == 
            vector[true, true, false]);

    // Verify the final hash represents all 4 elements
    let leaf = fixed_leaf;
    let x = hash_two_to_one_u256(leaf, leaf);
    let y = hash_two_to_one_u256(x, x);
    assert!(mmr[2] == y);
}

#[test]
fun test_mmr_with_different_values() {
    let mut mmr = vector::empty();

    // Create different u256 values like we would get from different events
    let val1: u256 = 1;
    let val2: u256 = 2;
    let val3: u256 = 3;
    let val4: u256 = 4;

    // Verify these values are actually different
    assert!(val1 != val2);
    assert!(val2 != val3);
    assert!(val3 != val4);

    // Add them one by one and verify MMR behavior
    add_to_mmr(val1, &mut mmr);
    add_to_mmr(val2, &mut mmr);
    // After second add, position 0 should be empty, position 1 should have merged hash
    assert!(mmr[0] == 0);
    assert!(mmr[1] != 0);

    add_to_mmr(val3, &mut mmr);
    // Position 0 should now have val3, position 1 should still have the merged hash
    assert!(mmr[0] != 0);
    assert!(mmr[1] != 0);

    add_to_mmr(val4, &mut mmr);
    // Final state: positions 0,1 empty, position 2 has the final merged hash
    assert!(mmr[0] == 0);
    assert!(mmr[1] == 0);
    assert!(mmr[2] != 0);
}

#[test]
fun test_mmr_digest_compat_with_rust() {
    let mut mmr = vector::empty();
    let count = 8;

    let mut i = 0;
    while (i < count) {
        let fixed_new_val = 50 + i;
        add_to_mmr(fixed_new_val, &mut mmr);
        i = i + 1;
    };

    assert!(vector::length(&mmr) == 4);
    assert!(mmr[0] == 0);
    assert!(mmr[1] == 0);
    assert!(mmr[2] == 0);
    assert!(
        mmr[3] == 69725770072863840208899320192042305265295220676851872214494910464384102654361,
    );
}
