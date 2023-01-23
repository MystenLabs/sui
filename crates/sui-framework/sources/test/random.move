// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::random {
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
                vector::push_back(&mut output, *vector::borrow(&mut digest, i));
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
        let bytes = next_digest(random);
        let (value, i) = (0u128, 0u8);
        while (i < 16) {
            let byte = (vector::pop_back(&mut bytes) as u128);
            value = value + (byte << 8*i);
            i = i + 1;
        };
        value
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
        let bytes = next_digest(random);
        let (value, i) = (0u64, 0u8);
        while (i < 8) {
            let byte = (vector::pop_back(&mut bytes) as u64);
            value = value + (byte << 8*i);
            i = i + 1;
        };
        value
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
        let bytes = next_digest(random);
        (vector::pop_back(&mut bytes) as u32)
            + ((vector::pop_back(&mut bytes) as u32) << 8)
            + ((vector::pop_back(&mut bytes) as u32) << 16)
            + ((vector::pop_back(&mut bytes) as u32) << 24)
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
        let bytes = next_digest(random);
        (vector::pop_back(&mut bytes) as u16)
            + ((vector::pop_back(&mut bytes) as u16) << 8)
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
        *vector::borrow(&next_digest(random), 0)
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
    fun test_next_bool() {
        // Compare with test vector
        let random = new(b"seed");
        assert!(next_bool(&mut random) == true, 0);
        assert!(next_bool(&mut random) == false, 1);
        assert!(next_bool(&mut random) == true, 2);
        assert!(next_bool(&mut random) == true, 3);
        assert!(next_bool(&mut random) == false, 4);
        assert!(next_bool(&mut random) == false, 5);
        assert!(next_bool(&mut random) == true, 6);
        assert!(next_bool(&mut random) == true, 7);
    }

    #[test]
    fun test_next_u8() {
        // Compare with test vector
        let random = new(b"seed");
        assert!(next_u8(&mut random) == 191, 0);
        assert!(next_u8(&mut random) == 172, 1);
        assert!(next_u8(&mut random) == 101, 2);
        assert!(next_u8(&mut random) == 43, 3);
        assert!(next_u8(&mut random) == 10, 4);
        assert!(next_u8(&mut random) == 142, 5);
        assert!(next_u8(&mut random) == 249, 6);
        assert!(next_u8(&mut random) == 141, 7);
    }

    #[test]
    fun test_next_u8_in_range() {
        let random = new(b"seed");

        let i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < vector::length(&bounds)) {
            let upper_bound = *vector::borrow(&bounds, i);
            let j = 0;
            while (j < tests) {
                assert!(next_u8_in_range(&mut random, upper_bound) < upper_bound, 0);
                j = j + 1;
            };
            i = i + 1;
        }
    }

    #[test]
    fun test_next_u16() {
        // Compare with test vector
        let random = new(b"seed");
        assert!(next_u16(&mut random) == 23524, 0);
        assert!(next_u16(&mut random) == 30390, 1);
        assert!(next_u16(&mut random) == 60645, 2);
        assert!(next_u16(&mut random) == 2488, 3);
        assert!(next_u16(&mut random) == 5672, 4);
        assert!(next_u16(&mut random) == 36807, 5);
        assert!(next_u16(&mut random) == 54591, 6);
        assert!(next_u16(&mut random) == 41523, 7);
    }

    #[test]
    fun test_next_u16_in_range() {
        let random = new(b"seed");

        let i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < vector::length(&bounds)) {
            let upper_bound = *vector::borrow(&bounds, i);
            let j = 0;
            while (j < tests) {
                assert!(next_u16_in_range(&mut random, upper_bound) < upper_bound, 0);
                j = j + 1;
            };
            i = i + 1;
        }
    }

    #[test]
    fun test_next_u32() {
        // Compare with test vector
        let random = new(b"seed");
        assert!(next_u32(&mut random) == 2356042724, 0);
        assert!(next_u32(&mut random) == 2194372278, 1);
        assert!(next_u32(&mut random) == 1943727333, 2);
        assert!(next_u32(&mut random) == 3674540472, 3);
        assert!(next_u32(&mut random) == 560141864, 4);
        assert!(next_u32(&mut random) == 2309459911, 5);
        assert!(next_u32(&mut random) == 2130498879, 6);
        assert!(next_u32(&mut random) == 2063835699, 7);
    }

    #[test]
    fun test_next_u32_in_range() {
        let random = new(b"seed");

        let i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < vector::length(&bounds)) {
            let upper_bound = *vector::borrow(&bounds, i);
            let j = 0;
            while (j < tests) {
                assert!(next_u32_in_range(&mut random, upper_bound) < upper_bound, 0);
                j = j + 1;
            };
            i = i + 1;
        }
    }

    #[test]
    fun test_next_u64() {
        // Compare with test vector
        let random = new(b"seed");
        assert!(next_u64(&mut random) == 5845420307181886436, 0);
        assert!(next_u64(&mut random) == 7169586959492019894, 1);
        assert!(next_u64(&mut random) == 8821413273700855013, 2);
        assert!(next_u64(&mut random) == 17006289909767801272, 3);
        assert!(next_u64(&mut random) == 8349531451798263336, 4);
        assert!(next_u64(&mut random) == 1662646395949518791, 5);
        assert!(next_u64(&mut random) == 17661794895045383487, 6);
        assert!(next_u64(&mut random) == 12177043863244087859, 7);
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

    #[test]
    fun test_next_u128() {
        // Compare with test vector
        let random = new(b"seed");
        assert!(next_u128(&mut random) == 69353424864165392191432166318042668004, 0);
        assert!(next_u128(&mut random) == 12194030816161474852776228502914168502, 1);
        assert!(next_u128(&mut random) == 206987904376642456854249538403010538725, 2);
        assert!(next_u128(&mut random) == 197466311403128565716545068165788666296, 3);
        assert!(next_u128(&mut random) == 15530841291297409371230861184202905128, 4);
        assert!(next_u128(&mut random) == 165552967413296855339223280683074424775, 5);
        assert!(next_u128(&mut random) == 8783412497932783467700075003507430719, 6);
        assert!(next_u128(&mut random) == 253037866491608363794848265776744604211, 7);
    }

    #[test]
    fun test_next_u128_in_range() {
        let random = new(b"seed");

        let i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < vector::length(&bounds)) {
            let upper_bound = *vector::borrow(&bounds, i);
            let j = 0;
            while (j < tests) {
                assert!(next_u128_in_range(&mut random, upper_bound) < upper_bound, 0);
                j = j + 1;
            };
            i = i + 1;
        }
    }

    #[test]
    fun test_next_u256() {
        // Compare with test vector
        let random = new(b"seed");
        assert!(next_u256(&mut random) == 86613847811709056614394817717810651756872989141966064397767345384420144995300, 0);
        assert!(next_u256(&mut random) == 77800816446193124932998172029673839405392227151477062245771844997605157992118, 1);
        assert!(next_u256(&mut random) == 45725748659421166954417450578509602234536262489471512215816310423253128244453, 2);
        assert!(next_u256(&mut random) == 19696026105199390416727112585766461108620822978182620644600554326664686143928, 3);
        assert!(next_u256(&mut random) == 4837202928086718576193880638295431461498764555598430221157283092238776342056, 4);
        assert!(next_u256(&mut random) == 64381219501514114493056586541507267846517000509074341237350219945295894515655, 5);
        assert!(next_u256(&mut random) == 112652690078173752677112416039396981893266964593788132944794761758835606345023, 6);
        assert!(next_u256(&mut random) == 64098809897132458178637712714755106201123339790293900115362940842770040070707, 7);
    }

    #[test]
    fun test_next_u256_in_range() {
        let random = new(b"seed");

        let i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < vector::length(&bounds)) {
            let upper_bound = *vector::borrow(&bounds, i);
            let j = 0;
            while (j < tests) {
                assert!(next_u256_in_range(&mut random, upper_bound) < upper_bound, 0);
                j = j + 1;
            };
            i = i + 1;
        }
    }

}
