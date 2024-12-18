// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::bit_vector_tests;

use std::bit_vector;

#[test_only]
fun test_bitvector_set_unset_of_size(k: u64) {
    let mut bitvector = bit_vector::new(k);
    let mut index = 0;
    while (index < k) {
        bitvector.set(index);
        assert!(bitvector.is_index_set(index));
        index = index + 1;
        let mut index_to_right = index;
        while (index_to_right < k) {
            assert!(!bitvector.is_index_set(index_to_right));
            index_to_right = index_to_right + 1;
        };
    };
    // now go back down unsetting
    index = 0;

    while (index < k) {
        bitvector.unset(index);
        assert!(!bitvector.is_index_set(index));
        index = index + 1;
        let mut index_to_right = index;
        while (index_to_right < k) {
            assert!(bitvector.is_index_set(index_to_right));
            index_to_right = index_to_right + 1;
        };
    };
}

#[test]
#[expected_failure(abort_code = bit_vector::EINDEX)]
fun set_bit_out_of_bounds() {
    let mut bitvector = bit_vector::new(bit_vector::word_size());
    bitvector.set(bit_vector::word_size());
}

#[test]
#[expected_failure(abort_code = bit_vector::EINDEX)]
fun unset_bit_out_of_bounds() {
    let mut bitvector = bit_vector::new(bit_vector::word_size());
    bitvector.unset(bit_vector::word_size());
}

#[test]
#[expected_failure(abort_code = bit_vector::EINDEX)]
fun index_bit_out_of_bounds() {
    let bitvector = bit_vector::new(bit_vector::word_size());
    bitvector.is_index_set(bit_vector::word_size());
}

#[test]
fun test_set_bit_and_index_basic() {
    test_bitvector_set_unset_of_size(8)
}

#[test]
fun test_set_bit_and_index_odd_size() {
    test_bitvector_set_unset_of_size(140)
}

#[test]
fun longest_sequence_no_set_zero_index() {
    let bitvector = bit_vector::new(100);
    assert!(bitvector.longest_set_sequence_starting_at(0) == 0);
}

#[test]
fun longest_sequence_one_set_zero_index() {
    let mut bitvector = bit_vector::new(100);
    bitvector.set(1);
    assert!(bitvector.longest_set_sequence_starting_at(0) == 0);
}

#[test]
fun longest_sequence_no_set_nonzero_index() {
    let bitvector = bit_vector::new(100);
    assert!(bitvector.longest_set_sequence_starting_at(51) == 0);
}

#[test]
fun longest_sequence_two_set_nonzero_index() {
    let mut bitvector = bit_vector::new(100);
    bitvector.set(50);
    bitvector.set(52);
    assert!(bitvector.longest_set_sequence_starting_at(51) == 0);
}

#[test]
fun longest_sequence_with_break() {
    let mut bitvector = bit_vector::new(100);
    let mut i = 0;
    while (i < 20) {
        bitvector.set(i);
        i = i + 1;
    };
    // create a break in the run
    i = i + 1;
    while (i < 100) {
        bitvector.set(i);
        i = i + 1;
    };
    assert!(bitvector.longest_set_sequence_starting_at(0) == 20);
    assert!(bitvector.longest_set_sequence_starting_at(20) == 0);
    assert!(bitvector.longest_set_sequence_starting_at(21) == 100 - 21);
}

#[test]
fun test_shift_left() {
    let bitlen = 97;
    let mut bitvector = bit_vector::new(bitlen);

    let mut i = 0;
    while (i < bitlen) {
        bitvector.set(i);
        i = i + 1;
    };

    i = bitlen - 1;
    while (i > 0) {
        assert!(bitvector.is_index_set(i));
        bitvector.shift_left(1);
        assert!(!bitvector.is_index_set(i));
        i = i - 1;
    };
}

#[test]
fun test_shift_left_specific_amount() {
    let bitlen = 300;
    let shift_amount = 133;
    let mut bitvector = bit_vector::new(bitlen);

    bitvector.set(201);
    assert!(bitvector.is_index_set(201));

    bitvector.shift_left(shift_amount);
    assert!(bitvector.is_index_set(201 - shift_amount));
    assert!(!bitvector.is_index_set(201));

    // Make sure this shift clears all the bits
    bitvector.shift_left(bitlen  - 1);

    let mut i = 0;
    while (i < bitlen) {
        assert!(!bitvector.is_index_set(i));
        i = i + 1;
    }
}

#[test]
fun test_shift_left_specific_amount_to_unset_bit() {
    let bitlen = 50;
    let chosen_index = 24;
    let shift_amount = 3;
    let mut bitvector = bit_vector::new(bitlen);

    let mut i = 0;

    while (i < bitlen) {
        bitvector.set(i);
        i = i + 1;
    };

    bitvector.unset(chosen_index);
    assert!(!bitvector.is_index_set(chosen_index));

    bitvector.shift_left(shift_amount);

    i = 0;

    while (i < bitlen) {
        // only chosen_index - shift_amount and the remaining bits should be BitVector::unset
        if ((i == chosen_index - shift_amount) || (i >= bitlen - shift_amount)) {
            assert!(!bitvector.is_index_set(i));
        } else {
            assert!(bitvector.is_index_set(i));
        };
        i = i + 1;
    }
}

#[test]
fun shift_left_at_size() {
    let bitlen = 133;
    let mut bitvector = bit_vector::new(bitlen);

    let mut i = 0;
    while (i < bitlen) {
        bitvector.set(i);
        i = i + 1;
    };

    bitvector.shift_left(bitlen - 1);
    i = bitlen - 1;
    while (i > 0) {
        assert!(!bitvector.is_index_set(i));
        i = i - 1;
    };
}

#[test]
fun shift_left_more_than_size() {
    let bitlen = 133;
    let mut bitvector = bit_vector::new(bitlen);
    bitvector.shift_left(bitlen);
}

#[test]
#[expected_failure(abort_code = bit_vector::ELENGTH)]
fun empty_bitvector() {
    bit_vector::new(0);
}

#[test]
fun single_bit_bitvector() {
    let bitvector = bit_vector::new(1);
    assert!(bitvector.length() == 1);
}
