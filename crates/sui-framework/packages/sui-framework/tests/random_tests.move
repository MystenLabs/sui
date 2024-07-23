// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
#[allow(unused_use)]
module sui::random_tests {
    use sui::test_utils::assert_eq;
    use sui::bcs;
    use sui::test_scenario;
    use sui::random::{Self, Random};

    // TODO: add a test from https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication800-22r1a.pdf ?

    #[test]
    fun random_test_basic_flow() {
        let mut scenario = test_scenario::begin(@0x0);

        random::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            0,
            x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
            scenario.ctx(),
        );

        let mut gen = random_state.new_generator(scenario.ctx());
        let _o256 = gen.generate_u256();

        test_scenario::return_shared(random_state);
        scenario.end();
    }

    #[test]
    fun test_new_generator() {
        let global_random1 = x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F";
        let global_random2 = x"2F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1A";

        // Create Random
        let mut scenario = test_scenario::begin(@0x0);
        random::create_for_testing(scenario.ctx());
        scenario.end();

        // Set random to global_random1
        let mut scenario = test_scenario::begin(@0x0);
        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            0,
            global_random1,
            scenario.ctx(),
        );
        scenario.next_tx(@0x0);
        let gen1 = random_state.new_generator(scenario.ctx());
        test_scenario::return_shared(random_state);
        scenario.end();

        // Set random again to global_random1
        let mut scenario = test_scenario::begin(@0x0);
        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            1,
            global_random1,
            scenario.ctx(),
        );
        scenario.next_tx(@0x0);
        let gen2 = random_state.new_generator(scenario.ctx());
        test_scenario::return_shared(random_state);
        scenario.end();

        // Set random to global_random2
        let mut scenario = test_scenario::begin(@0x0);
        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            2,
            global_random2,
            scenario.ctx(),
        );
        scenario.next_tx(@0x0);
        let gen3 = random_state.new_generator(scenario.ctx());
        let gen4 = random_state.new_generator(scenario.ctx());
        test_scenario::return_shared(random_state);
        scenario.end();

        assert!(gen1.generator_counter() == 0);
        assert!(gen1.generator_buffer().is_empty());
        assert!(gen1.generator_seed() == gen2.generator_seed());
        assert!(gen1.generator_seed() != gen3.generator_seed());
        assert!(gen3.generator_seed() != gen4.generator_seed());
    }

    #[test]
    fun random_tests_regression() {
        let mut scenario = test_scenario::begin(@0x0);

        random::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            0,
            x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
            scenario.ctx(),
        );

        // Regression (not critical for security, but still an indication that something is wrong).
        let mut gen = random_state.new_generator(scenario.ctx());
        let o256 = gen.generate_u256();
        assert!(o256 == 85985798878417437391783029796051418802193098452099584085821130568389745847195);
        let o128 = gen.generate_u128();
        assert!(o128 == 332057125240408555349883177059479920214);
        let o64 = gen.generate_u64();
        assert!(o64 == 13202990749492462163);
        let o32 = gen.generate_u32();
        assert!(o32 == 3316307786);
        let o16 = gen.generate_u16();
        assert!(o16 == 5961);
        let o8 = gen.generate_u8();
        assert!(o8 == 222);
        let output = gen.generate_u128_in_range(51, 123456789);
        assert!(output == 99859235);
        let output = gen.generate_u64_in_range(51, 123456789);
        assert!(output == 87557915);
        let output = gen.generate_u32_in_range(51, 123456789);
        assert!(output == 57096277);
        let output = gen.generate_u16_in_range(51, 1234);
        assert!(output == 349);
        let output = gen.generate_u8_in_range(51, 123);
        assert!(output == 60);
        let output = gen.generate_bytes(11);
        assert!(output == x"252cfdbb59205fcc509c9e");
        let output = gen.generate_bool();
        assert!(output == true);

        test_scenario::return_shared(random_state);
        scenario.end();
    }

    #[test]
    fun test_bytes() {
        let mut scenario = test_scenario::begin(@0x0);

        random::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            0,
            x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
            scenario.ctx(),
        );

        let mut gen = random_state.new_generator(scenario.ctx());

        // Check the output size & internal generator state
        assert!(gen.generator_buffer().is_empty());
        let output = gen.generate_bytes(1);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 31);
        assert!(output.length() == 1);
        let output = gen.generate_bytes(2);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 29);
        assert!(output.length() == 2);
        let output = gen.generate_bytes(29);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 0);
        assert!(output.length() == 29);
        let output = gen.generate_bytes(11);
        assert!(gen.generator_counter() == 2);
        assert!(gen.generator_buffer().length() == 21);
        assert!(output.length() == 11);
        let output = gen.generate_bytes(32 * 2);
        assert!(gen.generator_counter() == 4);
        assert!(gen.generator_buffer().length() == 21);
        assert!(output.length() == 32 * 2);
        let output = gen.generate_bytes(32 * 5 + 5);
        assert!(gen.generator_counter() == 9);
        assert!(gen.generator_buffer().length() == 16);
        assert!(output.length() == 32 * 5 + 5);

        // Sanity check that the output is not all zeros.
        let output = gen.generate_bytes(10);
        let mut i = 0;
        loop {
            // should break before the overflow
            if (output[i] != 0u8) break;
            i = i + 1;
        };

        // Sanity check that 2 different outputs are different.
        let output1 = gen.generate_bytes(10);
        let output2 = gen.generate_bytes(10);
        i = 0;
        loop {
            // should break before the overflow
            if (&output1[i] != &output2[i]) break;
            i = i + 1;
        };

        test_scenario::return_shared(random_state);
        scenario.end();
    }

    #[test]
    fun random_tests_uints() {
        let mut scenario = test_scenario::begin(@0x0);

        random::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            0,
            x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
            scenario.ctx(),
        );

        // u256
        let mut gen = random_state.new_generator(scenario.ctx());
        assert!(gen.generator_buffer().is_empty());
        let output1 = gen.generate_u256();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 0);
        let output2 = gen.generate_u256();
        assert!(gen.generator_counter() == 2);
        assert!(gen.generator_buffer().length() == 0);
        assert!(output1 != output2);
        let _output3 = gen.generate_u8();
        let _output4 = gen.generate_u256();
        assert!(gen.generator_counter() == 4);
        assert!(gen.generator_buffer().length() == 31);
        // Check that we indeed generate all bytes as random
        let mut i = 0;
        while (i < 32) {
            let x = gen.generate_u256();
            let x_bytes = bcs::to_bytes(&x);
            if (x_bytes[i] != 0u8) i = i + 1;
        };

        // u128
        gen = random_state.new_generator(scenario.ctx());
        assert!(gen.generator_buffer().is_empty());
        let output1 = gen.generate_u128();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 16);
        let output2 = gen.generate_u128();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 0);
        assert!(output1 != output2);
        let _output3 = gen.generate_u8();
        let _output4 = gen.generate_u128();
        assert!(gen.generator_counter() == 2);
        assert!(gen.generator_buffer().length() == 15);
        let mut i = 0;
        while (i < 16) {
            let x = gen.generate_u128();
            let x_bytes = bcs::to_bytes(&x);
            if (x_bytes[i] != 0u8) i = i + 1;
        };

        // u64
        gen = random_state.new_generator(scenario.ctx());
        assert!(gen.generator_buffer().is_empty());
        let output1 = gen.generate_u64();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 24);
        let output2 = gen.generate_u64();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 16);
        assert!(output1 != output2);
        let _output3 = gen.generate_u8();
        let _output4 = gen.generate_u64();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 7);
        let mut i = 0;
        while (i < 8) {
            let x = gen.generate_u64();
            let x_bytes = bcs::to_bytes(&x);
            if (x_bytes[i] != 0u8) i = i + 1;
        };

        // u32
        gen = random_state.new_generator(scenario.ctx());
        assert!(gen.generator_buffer().is_empty());
        let output1 = gen.generate_u32();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 28);
        let output2 = gen.generate_u32();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 24);
        assert!(output1 != output2);
        let _output3 = gen.generate_u8();
        let _output4 = gen.generate_u32();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 19);
        let mut i = 0;
        while (i < 4) {
            let x = gen.generate_u32();
            let x_bytes = bcs::to_bytes(&x);
            if (x_bytes[i] != 0u8) i = i + 1;
        };

        // u16
        gen = random_state.new_generator(scenario.ctx());
        assert!(gen.generator_buffer().is_empty());
        let output1 = gen.generate_u16();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 30);
        let output2 = gen.generate_u16();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 28);
        assert!(output1 != output2);
        let _output3 = gen.generate_u8();
        let _output4 = gen.generate_u16();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 25);
        let mut i = 0;
        while (i < 2) {
            let x = gen.generate_u16();
            let x_bytes = bcs::to_bytes(&x);
            if (x_bytes[i] != 0u8) i = i + 1;
        };

        // u8
        gen = random_state.new_generator(scenario.ctx());
        assert!(gen.generator_buffer().is_empty());
        let output1 = gen.generate_u8();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 31);
        let output2 = gen.generate_u8();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 30);
        assert!(output1 != output2);
        let _output3 = gen.generate_u128();
        let _output4 = gen.generate_u8();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 13);
        loop {
            let x = gen.generate_u8();
            if (x != 0u8) break
        };

        // bool
        gen = random_state.new_generator(scenario.ctx());
        assert!(gen.generator_buffer().is_empty());
        let output1 = gen.generate_bool();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 31);
        let output2 = gen.generate_bool();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 30);
        assert!(output1 != output2);
        let _output3 = gen.generate_u128();
        let _output4 = gen.generate_u8();
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 13);
        let mut saw_false = false;
        loop {
            let x = gen.generate_bool();
            saw_false = saw_false || !x;
            if (x && saw_false) break;
        };

        test_scenario::return_shared(random_state);
        scenario.end();
    }

    #[test]
    fun test_shuffle() {
        let mut scenario = test_scenario::begin(@0x0);

        random::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            0,
            x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
            scenario.ctx(),
        );

        let mut gen = random_state.new_generator(scenario.ctx());
        let mut v: vector<u16> = vector[0, 1, 2, 3, 4];
        gen.shuffle(&mut v);
        assert!(v.length() == 5);
        let mut i: u16 = 0;
        while (i < 5) {
            assert!(v.contains(&i));
            i = i + 1;
        };

        // check that numbers indeed eventaually move to all positions
        loop {
            gen.shuffle(&mut v);
            if ((v[4] == 1u16)) break;
        };
        loop {
            gen.shuffle(&mut v);
            if ((v[0] == 2u16)) break;
        };

        let mut v: vector<u32> = vector[];
        gen.shuffle(&mut v);
        assert!(v.length() == 0);

        let mut v: vector<u32> = vector[321];
        gen.shuffle(&mut v);
        assert!(v.length() == 1);
        assert!(v[0] == 321u32);

        test_scenario::return_shared(random_state);
        scenario.end();
    }

    #[test]
    fun random_tests_in_range() {
        let mut scenario = test_scenario::begin(@0x0);

        random::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            0,
            x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
            scenario.ctx(),
        );

        // generate_u128_in_range
        let mut gen = random_state.new_generator(scenario.ctx());
        let output1 = gen.generate_u128_in_range(11, 123454321);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 8);
        let output2 = gen.generate_u128_in_range(11, 123454321);
        assert!(gen.generator_counter() == 2);
        assert!(gen.generator_buffer().length() == 16);
        assert!(output1 != output2);
        let output = gen.generate_u128_in_range(123454321, 123454321 + 1);
        assert!((output == 123454321) || (output == 123454321 + 1));
        // test the edge case of u128_in_range (covers also the other in_range functions)
        let _output = gen.generate_u128_in_range(0, 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF);
        let mut i = 0;
        while (i < 50) {
            let min = gen.generate_u128();
            let max = min + (gen.generate_u64() as u128);
            let output = gen.generate_u128_in_range(min, max);
            assert!(output >= min);
            assert!(output <= max);
            i = i + 1;
        };

        // generate_u64_in_range
        gen = random_state.new_generator(scenario.ctx());
        let output1 = gen.generate_u64_in_range(11, 123454321);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 16);
        let output2 = gen.generate_u64_in_range(11, 123454321);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 0);
        assert!(output1 != output2);
        let output = gen.generate_u64_in_range(123454321, 123454321 + 1);
        assert!((output == 123454321) || (output == 123454321 + 1));
        let mut i = 0;
        while (i < 50) {
            let min = gen.generate_u64();
            let max = min + (gen.generate_u32() as u64);
            let output = gen.generate_u64_in_range(min, max);
            assert!(output >= min);
            assert!(output <= max);
            i = i + 1;
        };

        // generate_u32_in_range
        gen = random_state.new_generator(scenario.ctx());
        let output1 = gen.generate_u32_in_range(11, 123454321);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 20);
        let output2 = gen.generate_u32_in_range(11, 123454321);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 8);
        assert!(output1 != output2);
        let output = gen.generate_u32_in_range(123454321, 123454321 + 1);
        assert!((output == 123454321) || (output == 123454321 + 1));
        let mut i = 0;
        while (i < 50) {
            let min = gen.generate_u32();
            let max = min + (gen.generate_u16() as u32);
            let output = gen.generate_u32_in_range(min, max);
            assert!(output >= min);
            assert!(output <= max);
            i = i + 1;
        };

        // generate_u16_in_range
        gen = random_state.new_generator(scenario.ctx());
        let output1 = gen.generate_u16_in_range(11, 12345);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 22);
        let output2 = gen.generate_u16_in_range(11, 12345);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 12);
        assert!(output1 != output2);
        let output = gen.generate_u16_in_range(12345, 12345 + 1);
        assert!((output == 12345) || (output == 12345 + 1));
        let mut i = 0;
        while (i < 50) {
            let min = gen.generate_u16();
            let max = min + (gen.generate_u8() as u16);
            let output = gen.generate_u16_in_range(min, max);
            assert!(output >= min);
            assert!(output <= max);
            i = i + 1;
        };

        // generate_u8_in_range
        gen = random_state.new_generator(scenario.ctx());
        let output1 = gen.generate_u8_in_range(11, 123);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 23);
        let output2 = gen.generate_u8_in_range(11, 123);
        assert!(gen.generator_counter() == 1);
        assert!(gen.generator_buffer().length() == 14);
        assert!(output1 != output2);
        let output = gen.generate_u8_in_range(123, 123 + 1);
        assert!((output == 123) || (output == 123 + 1));
        let mut i = 0;
        while (i < 50) {
            let (min, max) = (gen.generate_u8(), gen.generate_u8());
            let (min, max) = if (min < max) (min, max) else (max, min);
            let (min, max) = if (min == max) (min, max + 1) else (min, max);
            let output = gen.generate_u8_in_range(min, max);
            assert!(output >= min);
            assert!(output <= max);
            i = i + 1;
        };

        // in range with min=max should return min
        assert_eq(gen.generate_u32_in_range(123, 123), 123);

        test_scenario::return_shared(random_state);
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = random::EInvalidRange)]
    fun random_tests_invalid_range() {
        let mut scenario = test_scenario::begin(@0x0);

        random::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            0,
            x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
            scenario.ctx(),
        );

        let mut gen = random_state.new_generator(scenario.ctx());
        let _output = gen.generate_u128_in_range(511, 500);

        test_scenario::return_shared(random_state);
        scenario.end();
    }

    #[test]
    fun random_tests_update_after_epoch_change() {
        let mut scenario = test_scenario::begin(@0x0);
        random::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            0,
            vector[0, 1, 2, 3],
            scenario.ctx()
        );
        random_state.update_randomness_state_for_testing(
            1,
            vector[4, 5, 6, 7],
            scenario.ctx()
        );

        scenario.next_epoch(@0x0);

        random_state.update_randomness_state_for_testing(
            0,
            vector[8, 9, 10, 11],
            scenario.ctx()
        );

        test_scenario::return_shared(random_state);
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = random::EInvalidRandomnessUpdate)]
    fun random_tests_duplicate() {
        let mut scenario = test_scenario::begin(@0x0);
        random::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            0,
            vector[0, 1, 2, 3],
            scenario.ctx()
        );
        random_state.update_randomness_state_for_testing(
            0,
            vector[0, 1, 2, 3],
            scenario.ctx()
        );

        test_scenario::return_shared(random_state);
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = random::EInvalidRandomnessUpdate)]
    fun random_tests_out_of_order() {
        let mut scenario = test_scenario::begin(@0x0);
        random::create_for_testing(scenario.ctx());
        scenario.next_tx(@0x0);

        let mut random_state = scenario.take_shared<Random>();
        random_state.update_randomness_state_for_testing(
            0,
            vector[0, 1, 2, 3],
            scenario.ctx()
        );
        random_state.update_randomness_state_for_testing(
            3,
            vector[0, 1, 2, 3],
            scenario.ctx()
        );

        test_scenario::return_shared(random_state);
        scenario.end();
    }
}
