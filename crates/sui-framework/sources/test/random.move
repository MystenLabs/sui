// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::random {
    use std::hash;
    use std::vector;
    use sui::bcs;

    // Internally, the pseudorandom generator uses a hash chain over Sha3-256
    // which has an output length of 32 bytes.
    const DIGEST_LENGTH: u64 = 32;

    /// This represents a seeded pseudorandom generator. Note that the generated
    /// values are not safe to use for cryptographic purposes.
    struct Random has store, drop {
        state: vector<u8>,
    }

    /// Update the state of the generator and return a vector holding `DIGEST_LENGTH`
    /// random bytes.
    fun next_digest(random: &mut Random): vector<u8> {
        random.state = hash::sha3_256(random.state);
        random.state
    }

    /// Create a new pseudorandom generator with the given seed.
    public fun new(seed: vector<u8>): Random {
        Random { state: seed }
    }

    /// Use the given pseudorandom generator to generate a vector with l random bytes.
    public fun next_bytes(random: &mut Random, l: u64): vector<u8> {
        // We need ceil(l / DIGEST_LENGTH) digests to fill the array
        let quotient = l / DIGEST_LENGTH;
        let remainder = l - quotient * DIGEST_LENGTH;

        let (i, output) = (0, vector[]);
        while (i < quotient) {
            vector::append(&mut output, next_digest(random));
            i = i + 1;
        };

        // If quotient is not exact, fill the remaining bytes
        if (remainder > 0) {
            let (i, digest) = (0, next_digest(random));
            while (i < remainder) {
                vector::push_back(&mut output, *vector::borrow(&mut digest, i));
                i = i + 1;
            };
        };

        output
    }

    /// Use the given pseudorandom generator to generate a random `u64` integer.
    public fun next_u64(random: &mut Random): u64 {
        let bytes = next_digest(random);
        bcs::peel_u64(&mut bcs::new(bytes))
    }

    /// Use the given pseudorandom generator to generate an `u64` integer in the range
    /// [0, ..., 2^bit_length - 1].
    fun next_u64_with_bit_length(random: &mut Random, bit_length: u8): u64 {
        next_u64(random) >> (64 - bit_length)
    }

    /// Compute the bit length of n.
    fun bit_length(n: u64): u8 {
        if (n == 0) {
            0
        } else {
            // Use binary search to find the bit length of n
            let (length, mid) = (1, 32);
            while (mid > 0) {
                let n_mod_mid = n >> mid;
                if (n_mod_mid > 0) {
                    // The bit length of n is strictly larger than mid.
                    length = length + mid;
                    n = n_mod_mid;
                };
                mid = mid >> 1;
            };
            length
        }
    }

    /// Use the given pseudo-random generator and a non-zero `upper_bound` to generate a
    /// random `u64` integer in the range [0, ..., upper_bound - 1].
    public fun next_u64_in_range(random: &mut Random, upper_bound: u64): u64 {
        assert!(upper_bound > 0, 0);
        let bit_length = bit_length(upper_bound);
        let candidate = next_u64_with_bit_length(random, bit_length);
        while (candidate >= upper_bound) {
            candidate = next_u64_with_bit_length(random, bit_length);
        };
        candidate
    }

    /// Use the given pseudorandom generator to generate a random `u8`.
    public fun next_u8(random: &mut Random): u8 {
        *vector::borrow(&next_digest(random), 0)
    }

    /// Use the given pseudorandom generator to generate a random `bool`.
    public fun next_bool(random: &mut Random): bool {
        next_u8(random) % 2 == 1
    }

    #[test]
    fun test_next_bytes() {
        let lengths = vector[1, 31, 32, 33, 63, 64, 65];

        let i = 0;
        while (i < vector::length(&lengths)) {
            let length = *vector::borrow(&lengths, i);

            // The length should be the requested length
            let random1 = new(b"seed");
            let bytes = next_bytes(&mut random1, length);
            assert!(vector::length(&bytes) == length, 0);

            // Two generators with different seeds should give different outputs
            let random1 = new(b"seed 1");
            let random2 = new(b"seed 2");
            assert!(next_bytes(&mut random1, length) !=
                next_bytes(&mut random2, length), 2);

            // Two generators with the same seed should give the same output
            let random1 = new(b"seed");
            let random2 = new(b"seed");
            assert!(next_bytes(&mut random1, length) ==
                next_bytes(&mut random2, length), 3);

            i = i + 1;
        }
    }

    #[test]
    fun test_next_u64_in_range() {
        let random = new(b"seed");

        let i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < vector::length(&bounds)) {
            let upper_bound = *vector::borrow(&bounds, i);
            let j = 0;
            while (j < tests) {
                assert!(next_u64_in_range(&mut random, upper_bound) < upper_bound, 0);
                j = j + 1;
            };
            i = i + 1;
        }
    }

}
