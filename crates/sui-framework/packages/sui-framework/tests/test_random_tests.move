// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::test_random_tests {
    use sui::test_random::new;

    #[test]
    fun test_next_bytes() {
        let lengths = vector[1, 31, 32, 33, 63, 64, 65];

        let mut i = 0;
        while (i < lengths.length()) {
            let length = lengths[i];

            // The length should be the requested length
            let mut random1 = new(b"seed");
            let bytes = random1.next_bytes(length);
            assert!(bytes.length() == length);

            // Two generators with different seeds should give different outputs
            let mut random1 = new(b"seed 1");
            let mut random2 = new(b"seed 2");
            assert!(random1.next_bytes(length) !=
                random2.next_bytes(length), 2);

            // Two generators with the same seed should give the same output
            let mut random1 = new(b"seed");
            let mut random2 = new(b"seed");
            assert!(random1.next_bytes(length) ==
                random2.next_bytes(length), 3);

            i = i + 1;
        }
    }

    #[test]
    fun test_next_bool() {
        // Compare with test vector
        let mut random = new(b"seed");
        assert!(random.next_bool() == false);
        assert!(random.next_bool() == false);
        assert!(random.next_bool() == true);
        assert!(random.next_bool() == false);
        assert!(random.next_bool() == false);
        assert!(random.next_bool() == true);
        assert!(random.next_bool() == true);
        assert!(random.next_bool() == true);
    }

    #[test]
    fun test_next_u8() {
        // Compare with test vector
        let mut random = new(b"seed");
        assert!(random.next_u8() == 228);
        assert!(random.next_u8() == 182);
        assert!(random.next_u8() == 229);
        assert!(random.next_u8() == 184);
        assert!(random.next_u8() == 40);
        assert!(random.next_u8() == 199);
        assert!(random.next_u8() == 63);
        assert!(random.next_u8() == 51);
    }

    #[test]
    fun test_next_u8_in_range() {
        let mut random = new(b"seed");

        let mut i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < bounds.length()) {
            let upper_bound = bounds[i];
            let mut j = 0;
            while (j < tests) {
                assert!(random.next_u8_in_range(upper_bound) < upper_bound);
                j = j + 1;
            };
            i = i + 1;
        }
    }

    #[test]
    fun test_next_u16() {
        // Compare with test vector
        let mut random = new(b"seed");
        assert!(random.next_u16() == 23524);
        assert!(random.next_u16() == 30390);
        assert!(random.next_u16() == 60645);
        assert!(random.next_u16() == 2488);
        assert!(random.next_u16() == 5672);
        assert!(random.next_u16() == 36807);
        assert!(random.next_u16() == 54591);
        assert!(random.next_u16() == 41523);
    }

    #[test]
    fun test_next_u16_in_range() {
        let mut random = new(b"seed");

        let mut i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < bounds.length()) {
            let upper_bound = bounds[i];
            let mut j = 0;
            while (j < tests) {
                assert!(random.next_u16_in_range(upper_bound) < upper_bound);
                j = j + 1;
            };
            i = i + 1;
        }
    }

    #[test]
    fun test_next_u32() {
        // Compare with test vector
        let mut random = new(b"seed");
        assert!(random.next_u32() == 2356042724);
        assert!(random.next_u32() == 2194372278);
        assert!(random.next_u32() == 1943727333);
        assert!(random.next_u32() == 3674540472);
        assert!(random.next_u32() == 560141864);
        assert!(random.next_u32() == 2309459911);
        assert!(random.next_u32() == 2130498879);
        assert!(random.next_u32() == 2063835699);
    }

    #[test]
    fun test_next_u32_in_range() {
        let mut random = new(b"seed");

        let mut i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < bounds.length()) {
            let upper_bound = bounds[i];
            let mut j = 0;
            while (j < tests) {
                assert!(random.next_u32_in_range(upper_bound) < upper_bound);
                j = j + 1;
            };
            i = i + 1;
        }
    }

    #[test]
    fun test_next_u64() {
        // Compare with test vector
        let mut random = new(b"seed");
        assert!(random.next_u64() == 5845420307181886436);
        assert!(random.next_u64() == 7169586959492019894);
        assert!(random.next_u64() == 8821413273700855013);
        assert!(random.next_u64() == 17006289909767801272);
        assert!(random.next_u64() == 8349531451798263336);
        assert!(random.next_u64() == 1662646395949518791);
        assert!(random.next_u64() == 17661794895045383487);
        assert!(random.next_u64() == 12177043863244087859);
    }

    #[test]
    fun test_next_u64_in_range() {
        let mut random = new(b"seed");

        let mut i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < bounds.length()) {
            let upper_bound = bounds[i];
            let mut j = 0;
            while (j < tests) {
                assert!(random.next_u64_in_range(upper_bound) < upper_bound);
                j = j + 1;
            };
            i = i + 1;
        }
    }

    #[test]
    fun test_next_u128() {
        // Compare with test vector
        let mut random = new(b"seed");
        assert!(random.next_u128() == 69353424864165392191432166318042668004);
        assert!(random.next_u128() == 12194030816161474852776228502914168502);
        assert!(random.next_u128() == 206987904376642456854249538403010538725);
        assert!(random.next_u128() == 197466311403128565716545068165788666296);
        assert!(random.next_u128() == 15530841291297409371230861184202905128);
        assert!(random.next_u128() == 165552967413296855339223280683074424775);
        assert!(random.next_u128() == 8783412497932783467700075003507430719);
        assert!(random.next_u128() == 253037866491608363794848265776744604211);
    }

    #[test]
    fun test_next_u128_in_range() {
        let mut random = new(b"seed");

        let mut i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < bounds.length()) {
            let upper_bound = bounds[i];
            let mut j = 0;
            while (j < tests) {
                assert!(random.next_u128_in_range(upper_bound) < upper_bound);
                j = j + 1;
            };
            i = i + 1;
        }
    }

    #[test]
    fun test_next_u256() {
        // Compare with test vector
        let mut random = new(b"seed");
        assert!(
            random.next_u256() == 86613847811709056614394817717810651756872989141966064397767345384420144995300,
            0
        );
        assert!(
            random.next_u256() == 77800816446193124932998172029673839405392227151477062245771844997605157992118,
            1
        );
        assert!(
            random.next_u256() == 45725748659421166954417450578509602234536262489471512215816310423253128244453,
            2
        );
        assert!(
            random.next_u256() == 19696026105199390416727112585766461108620822978182620644600554326664686143928,
            3
        );
        assert!(
            random.next_u256() == 4837202928086718576193880638295431461498764555598430221157283092238776342056,
            4
        );
        assert!(
            random.next_u256() == 64381219501514114493056586541507267846517000509074341237350219945295894515655,
            5
        );
        assert!(
            random.next_u256() == 112652690078173752677112416039396981893266964593788132944794761758835606345023,
            6
        );
        assert!(
            random.next_u256() == 64098809897132458178637712714755106201123339790293900115362940842770040070707,
            7
        );
    }

    #[test]
    fun test_next_u256_in_range() {
        let mut random = new(b"seed");

        let mut i = 0;
        let bounds = vector[1, 7, 8, 9, 15, 16, 17];
        let tests = 10;
        while (i < bounds.length()) {
            let upper_bound = bounds[i];
            let mut j = 0;
            while (j < tests) {
                assert!(random.next_u256_in_range(upper_bound) < upper_bound);
                j = j + 1;
            };
            i = i + 1;
        }
    }
}
