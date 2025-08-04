// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module std::bit_vector;

/// The provided index is out of bounds
const EINDEX: u64 = 0x20000;
/// An invalid length of bitvector was given
const ELENGTH: u64 = 0x20001;

#[allow(unused_const)]
const WORD_SIZE: u64 = 1;
/// The maximum allowed bitvector size
const MAX_SIZE: u64 = 1024;

/// The `BitVector` structure contains a length parameter and a vector of boolean values.
/// This is a special vector that uses `true` to indicate that the bit is "1",
/// and `false` to indicate that the bit is "0".
public struct BitVector has copy, drop, store {
    length: u64,
    bit_field: vector<bool>,
}

/// Create a new `BitVector` of the corresponding length with all `false` in the vector.
public fun new(length: u64): BitVector {
    assert!(length > 0, ELENGTH);
    assert!(length < MAX_SIZE, ELENGTH);

    BitVector {
        length,
        bit_field: vector::tabulate!(length, |_| false)
    }
}

/// Set the bit at `bit_index` in the `bitvector` regardless of its previous state.
public fun set(bitvector: &mut BitVector, bit_index: u64) {
    assert!(bit_index < bitvector.bit_field.length(), EINDEX);
    let x = &mut bitvector.bit_field[bit_index];
    *x = true;
}

/// Unset the bit at `bit_index` in the `bitvector` regardless of its previous state.
public fun unset(bitvector: &mut BitVector, bit_index: u64) {
    assert!(bit_index < bitvector.bit_field.length(), EINDEX);
    let x = &mut bitvector.bit_field[bit_index];
    *x = false;
}

/// Shift the `bitvector` left by `amount`. If `amount` is greater than the
/// bitvector's length the bitvector will be zeroed out.
public fun shift_left(bitvector: &mut BitVector, amount: u64) {
    bitvector.length.do!(|i| {
        if (i + amount < bitvector.length && bitvector.is_index_set(i + amount)) {
            bitvector.set(i);
        } else {
            bitvector.unset(i);
        };
    });
}

/// Return the value of the bit at `bit_index` in the `bitvector`. `true`
/// represents "1" and `false` represents "0".
public fun is_index_set(bitvector: &BitVector, bit_index: u64): bool {
    assert!(bit_index < bitvector.bit_field.length(), EINDEX);
    bitvector.bit_field[bit_index]
}

/// Return the length (number of usable bits) of this bitvector
public fun length(bitvector: &BitVector): u64 {
    bitvector.bit_field.length()
}

/// Returns the length of the longest sequence of set bits starting at (and
/// including) `start_index` in the `bitvector`. If there is no such
/// sequence, then `0` is returned.
public fun longest_set_sequence_starting_at(bitvector: &BitVector, start_index: u64): u64 {
    assert!(start_index < bitvector.length, EINDEX);

    'longest: {
        (bitvector.length - start_index).do!(|i| if (!bitvector.is_index_set(start_index + i)) return 'longest i);
        bitvector.length - start_index
    }
}

#[test_only]
public fun word_size(): u64 {
    WORD_SIZE
}
