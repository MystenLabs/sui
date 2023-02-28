// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::test_random_tests {
    use sui::test_random::{new, next_bool, next_u8, next_u8_in_range, next_u16, next_u16_in_range, next_u32, next_u32_in_range, next_u64, next_u64_in_range, next_u128, next_u128_in_range, next_u256, next_u256_in_range};
    use std::vector;
    use sui::test_random::next_bytes;

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
        assert!(next_bool(&mut random) == false, 0);
        assert!(next_bool(&mut random) == false, 1);
        assert!(next_bool(&mut random) == true, 2);
        assert!(next_bool(&mut random) == false, 3);
        assert!(next_bool(&mut random) == false, 4);
        assert!(next_bool(&mut random) == true, 5);
        assert!(next_bool(&mut random) == true, 6);
        assert!(next_bool(&mut random) == true, 7);
    }

    #[test]
    fun test_next_u8() {
        // Compare with test vector
        let random = new(b"seed");
        assert!(next_u8(&mut random) == 228, 0);
        assert!(next_u8(&mut random) == 182, 1);
        assert!(next_u8(&mut random) == 229, 2);
        assert!(next_u8(&mut random) == 184, 3);
        assert!(next_u8(&mut random) == 40, 4);
        assert!(next_u8(&mut random) == 199, 5);
        assert!(next_u8(&mut random) == 63, 6);
        assert!(next_u8(&mut random) == 51, 7);
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
        assert!(
            next_u256(&mut random) == 86613847811709056614394817717810651756872989141966064397767345384420144995300,
            0
        );
        assert!(
            next_u256(&mut random) == 77800816446193124932998172029673839405392227151477062245771844997605157992118,
            1
        );
        assert!(
            next_u256(&mut random) == 45725748659421166954417450578509602234536262489471512215816310423253128244453,
            2
        );
        assert!(
            next_u256(&mut random) == 19696026105199390416727112585766461108620822978182620644600554326664686143928,
            3
        );
        assert!(
            next_u256(&mut random) == 4837202928086718576193880638295431461498764555598430221157283092238776342056,
            4
        );
        assert!(
            next_u256(&mut random) == 64381219501514114493056586541507267846517000509074341237350219945295894515655,
            5
        );
        assert!(
            next_u256(&mut random) == 112652690078173752677112416039396981893266964593788132944794761758835606345023,
            6
        );
        assert!(
            next_u256(&mut random) == 64098809897132458178637712714755106201123339790293900115362940842770040070707,
            7
        );
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
