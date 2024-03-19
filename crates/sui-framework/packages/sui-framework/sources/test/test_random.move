// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::test_random {
    use std::hash;
    use std::vector;

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
                vector::push_back(&mut output, *vector::borrow(&digest, i));
                i = i + 1;
            };
        };

        output
    }

    /// Use the given pseudorandom generator to generate a random `u256` integer.
    public fun next_u256(random: &mut Random): u256 {
        let bytes = next_digest(random);
        let (value, i) = (0u256, 0u8);
        while (i < 32) {
            let byte = (vector::pop_back(&mut bytes) as u256);
            value = value + (byte << 8*i);
            i = i + 1;
        };
        value
    }

    /// Use the given pseudo-random generator and a non-zero `upper_bound` to generate a
    /// random `u256` integer in the range [0, ..., upper_bound - 1]. Note that if the upper
    /// bound is not a power of two, the distribution will not be completely uniform.
    public fun next_u256_in_range(random: &mut Random, upper_bound: u256): u256 {
        assert!(upper_bound > 0, 0);
        next_u256(random) % upper_bound
    }

    /// Use the given pseudorandom generator to generate a random `u128` integer.
    public fun next_u128(random: &mut Random): u128 {
        (next_u256_in_range(random, 1 << 128) as u128)
    }

    /// Use the given pseudo-random generator and a non-zero `upper_bound` to generate a
    /// random `u128` integer in the range [0, ..., upper_bound - 1]. Note that if the upper
    /// bound is not a power of two, the distribution will not be completely uniform.
    public fun next_u128_in_range(random: &mut Random, upper_bound: u128): u128 {
        assert!(upper_bound > 0, 0);
        next_u128(random) % upper_bound
    }

    /// Use the given pseudorandom generator to generate a random `u64` integer.
    public fun next_u64(random: &mut Random): u64 {
        (next_u256_in_range(random, 1 << 64) as u64)
    }

    /// Use the given pseudo-random generator and a non-zero `upper_bound` to generate a
    /// random `u64` integer in the range [0, ..., upper_bound - 1]. Note that if the upper
    /// bound is not a power of two, the distribution will not be completely uniform.
    public fun next_u64_in_range(random: &mut Random, upper_bound: u64): u64 {
        assert!(upper_bound > 0, 0);
        next_u64(random) % upper_bound
    }

    /// Use the given pseudorandom generator to generate a random `u32`.
    public fun next_u32(random: &mut Random): u32 {
        (next_u256_in_range(random, 1 << 32) as u32)
    }

    /// Use the given pseudo-random generator and a non-zero `upper_bound` to generate a
    /// random `u32` integer in the range [0, ..., upper_bound - 1]. Note that if the upper
    /// bound is not a power of two, the distribution will not be completely uniform.
    public fun next_u32_in_range(random: &mut Random, upper_bound: u32): u32 {
        assert!(upper_bound > 0, 0);
        next_u32(random) % upper_bound
    }

    /// Use the given pseudorandom generator to generate a random `u16`.
    public fun next_u16(random: &mut Random): u16 {
        (next_u256_in_range(random, 1 << 16) as u16)
    }

    /// Use the given pseudo-random generator and a non-zero `upper_bound` to generate a
    /// random `u16` integer in the range [0, ..., upper_bound - 1]. Note that if the upper
    /// bound is not a power of two, the distribution will not be completely uniform.
    public fun next_u16_in_range(random: &mut Random, upper_bound: u16): u16 {
        assert!(upper_bound > 0, 0);
        next_u16(random) % upper_bound
    }

    /// Use the given pseudorandom generator to generate a random `u8`.
    public fun next_u8(random: &mut Random): u8 {
        vector::pop_back(&mut next_digest(random))
    }

    /// Use the given pseudo-random generator and a non-zero `upper_bound` to generate a
    /// random `u8` integer in the range [0, ..., upper_bound - 1]. Note that if the upper
    /// bound is not a power of two, the distribution will not be completely uniform.
    public fun next_u8_in_range(random: &mut Random, upper_bound: u8): u8 {
        assert!(upper_bound > 0, 0);
        next_u8(random) % upper_bound
    }

    /// Use the given pseudorandom generator to generate a random `bool`.
    public fun next_bool(random: &mut Random): bool {
        next_u8(random) % 2 == 1
    }
}
