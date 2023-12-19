
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
#[allow(unused_use)]
module sui::random_tests {
    use std::vector;
    use sui::test_scenario::Scenario;
    use sui::tx_context::TxContext;
    use sui::bcs;
    use sui::test_scenario;
    use sui::random::{
        Self,
        Random,
        update_randomness_state_for_testing, generator_seed, generator_counter, generator_buffer, bytes,
        generate_u256, generate_u128, generate_u64, generate_u32, generate_u16, generate_u8, generate_u128_in_range,
        generate_u64_in_range, generate_u32_in_range, generate_u16_in_range, generate_u8_in_range, new_request, fulfill,
        RandomGenerator,
    };


    const RANDOMNESS1: vector<u8> = x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F";
    const RANDOMNESS2: vector<u8> = x"2F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F";

    // TODO: add a test from https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication800-22r1a.pdf ?

    fun update_random(random: &mut Random, round: u64, value: vector<u8>, ctx: &TxContext) {
        update_randomness_state_for_testing(
            random,
            round,
            value,
            3,
            ctx
        );
    }

    fun get_generator(random: &mut Random, scenario: &mut Scenario): RandomGenerator {
        let curr_round = random::get_latest_round(random);
        update_random(random, curr_round + 1, RANDOMNESS1, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);
        let req = new_request(random, test_scenario::ctx(scenario));
        update_random(random, curr_round + 2, RANDOMNESS1, test_scenario::ctx(scenario));
        update_random(random, curr_round + 3, RANDOMNESS1, test_scenario::ctx(scenario));
        fulfill(&req, random)
    }

    #[test]
    fun test_basic_flow() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random = test_scenario::take_shared<Random>(scenario);
        let gen = get_generator(&mut random, scenario);
        let _o256 = generate_u256(&mut gen);

        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_deterministic_fulfill() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random = test_scenario::take_shared<Random>(scenario);
        update_random(&mut random, 1, RANDOMNESS1, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);
        let req = new_request(&random, test_scenario::ctx(scenario));
        update_random(&mut random, 2, RANDOMNESS1, test_scenario::ctx(scenario));
        update_random(&mut random, 3, RANDOMNESS1, test_scenario::ctx(scenario));
        let gen1 = fulfill(&req, &random);
        let gen2 = fulfill(&req, &random);
        assert!(generator_seed(&gen1) == generator_seed(&gen2), 0);

        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_different_req_same_round() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random = test_scenario::take_shared<Random>(scenario);
        update_random(&mut random, 1, RANDOMNESS1, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);
        let req1 = new_request(&random, test_scenario::ctx(scenario));
        let req2 = new_request(&random, test_scenario::ctx(scenario));
        update_random(&mut random, 2, RANDOMNESS1, test_scenario::ctx(scenario));
        update_random(&mut random, 3, RANDOMNESS1, test_scenario::ctx(scenario));
        let gen1 = fulfill(&req1, &random);
        let gen2 = fulfill(&req2, &random);
        assert!(generator_seed(&gen1) != generator_seed(&gen2), 0);

        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_new_generator() {
        // Create Random
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::end(scenario_val);

        // Set random to RANDOMNESS1
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let random = test_scenario::take_shared<Random>(scenario);
        update_random(&mut random, 1, RANDOMNESS1, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);
        let req = new_request(&random, test_scenario::ctx(scenario));
        update_random(&mut random, 2, RANDOMNESS1, test_scenario::ctx(scenario));
        update_random(&mut random, 3, RANDOMNESS1, test_scenario::ctx(scenario));
        let gen1 = fulfill(&req, &random);
        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);

        // Set random again to RANDOMNESS1 (though previous rounds were RANDOMNESS2)
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let random = test_scenario::take_shared<Random>(scenario);
        update_random(&mut random, 4, RANDOMNESS2, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);
        let req = new_request(&random, test_scenario::ctx(scenario));
        update_random(&mut random, 5, RANDOMNESS2, test_scenario::ctx(scenario));
        update_random(&mut random, 6, RANDOMNESS1, test_scenario::ctx(scenario));
        let gen2 = fulfill(&req, &random);
        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);

        // Set random to RANDOMNESS2
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let random = test_scenario::take_shared<Random>(scenario);
        update_random(&mut random, 7, RANDOMNESS2, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);
        let req3 = new_request(&random, test_scenario::ctx(scenario));
        let req4 = new_request(&random, test_scenario::ctx(scenario));
        update_random(&mut random, 8, RANDOMNESS2, test_scenario::ctx(scenario));
        update_random(&mut random, 9, RANDOMNESS2, test_scenario::ctx(scenario));
        let gen3 = fulfill(&req3, &random);
        let gen4 = fulfill(&req4, &random);
        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);

        assert!(generator_counter(&gen1) == 0, 0);
        assert!(*generator_buffer(&gen1) == vector::empty(), 0);
        assert!(generator_seed(&gen1) == generator_seed(&gen2), 0);
        assert!(generator_seed(&gen1) != generator_seed(&gen3), 0);
        assert!(generator_seed(&gen3) != generator_seed(&gen4), 0);
    }

    #[test]
    fun test_regression() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random = test_scenario::take_shared<Random>(scenario);
        let _gen = get_generator(&mut random, scenario);

        // Regression (not critical for security, but still an indication that something is wrong).
        // TODO: update regression

        // let o256 = generate_u256(&mut gen);
        // assert!(o256 == 85985798878417437391783029796051418802193098452099584085821130568389745847195, 0);
        // let o128 = generate_u128(&mut gen);
        // assert!(o128 == 332057125240408555349883177059479920214, 0);
        // let o64 = generate_u64(&mut gen);
        // assert!(o64 == 13202990749492462163, 0);
        // let o32 = generate_u32(&mut gen);
        // assert!(o32 == 3316307786, 0);
        // let o16 = generate_u16(&mut gen);
        // assert!(o16 == 5961, 0);
        // let o8 = generate_u8(&mut gen);
        // assert!(o8 == 222, 0);
        // let output = generate_u128_in_range(&mut gen, 51, 123456789);
        // assert!(output == 99859235, 0);
        // let output = generate_u64_in_range(&mut gen, 51, 123456789);
        // assert!(output == 87557915, 0);
        // let output = generate_u32_in_range(&mut gen, 51, 123456789);
        // assert!(output == 57096277, 0);
        // let output = generate_u16_in_range(&mut gen, 51, 1234);
        // assert!(output == 349, 0);
        // let output = generate_u8_in_range(&mut gen, 51, 123);
        // assert!(output == 60, 0);
        // let output = bytes(&mut gen, 11);
        // assert!(output == x"252cfdbb59205fcc509c9e", 0);

        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_bytes() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random = test_scenario::take_shared<Random>(scenario);
        let gen = get_generator(&mut random, scenario);

        // Check the output size & internal generator state
        assert!(*generator_buffer(&gen) == vector::empty(), 0);
        let output = bytes(&mut gen, 1);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 31, 0);
        assert!(vector::length(&output) == 1, 0);
        let output = bytes(&mut gen, 2);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 29, 0);
        assert!(vector::length(&output) == 2, 0);
        let output = bytes(&mut gen, 29);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 0, 0);
        assert!(vector::length(&output) == 29, 0);
        let output = bytes(&mut gen, 11);
        assert!(generator_counter(&gen) == 2, 0);
        assert!(vector::length(generator_buffer(&gen)) == 21, 0);
        assert!(vector::length(&output) == 11, 0);
        let output = bytes(&mut gen, 32 * 2);
        assert!(generator_counter(&gen) == 4, 0);
        assert!(vector::length(generator_buffer(&gen)) == 21, 0);
        assert!(vector::length(&output) == 32 * 2, 0);
        let output = bytes(&mut gen, 32 * 5 + 5);
        assert!(generator_counter(&gen) == 9, 0);
        assert!(vector::length(generator_buffer(&gen)) == 16, 0);
        assert!(vector::length(&output) == 32 * 5 + 5, 0);

        // Sanity check that the output is not all zeros.
        let output = bytes(&mut gen, 10);
        let i = 0;
        while (true) { // should break before the overflow
            if (*vector::borrow(&output, i) != 0u8)
                break;
            i = i + 1;
        };

        // Sanity check that 2 different outputs are different.
        let output1 = bytes(&mut gen, 10);
        let output2 = bytes(&mut gen, 10);
        i = 0;
        while (true) { // should break before the overflow
            if (vector::borrow(&output1, i) != vector::borrow(&output2, i))
                break;
            i = i + 1;
        };

        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_uints() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random = test_scenario::take_shared<Random>(scenario);
        let gen = get_generator(&mut random, scenario);

        assert!(*generator_buffer(&gen) == vector::empty(), 0);
        let output1 = generate_u256(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 0, 0);
        let output2 = generate_u256(&mut gen);
        assert!(generator_counter(&gen) == 2, 0);
        assert!(vector::length(generator_buffer(&gen)) == 0, 0);
        assert!(output1 != output2, 0);
        let _output3 = generate_u8(&mut gen);
        let _output4 = generate_u256(&mut gen);
        assert!(generator_counter(&gen) == 4, 0);
        assert!(vector::length(generator_buffer(&gen)) == 31, 0);
        // Check that we indeed generate all bytes as random
        let i = 0;
        while (i < 32) {
            let x = generate_u256(&mut gen);
            let x_bytes = bcs::to_bytes(&x);
            if (*vector::borrow(&x_bytes, i) != 0u8)
                i = i + 1;
        };

        // u128
        gen = get_generator(&mut random, scenario);
        assert!(*generator_buffer(&gen) == vector::empty(), 0);
        let output1 = generate_u128(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 16, 0);
        let output2 = generate_u128(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 0, 0);
        assert!(output1 != output2, 0);
        let _output3 = generate_u8(&mut gen);
        let _output4 = generate_u128(&mut gen);
        assert!(generator_counter(&gen) == 2, 0);
        assert!(vector::length(generator_buffer(&gen)) == 15, 0);
        let i = 0;
        while (i < 16) {
            let x = generate_u128(&mut gen);
            let x_bytes = bcs::to_bytes(&x);
            if (*vector::borrow(&x_bytes, i) != 0u8)
                i = i + 1;
        };

        // u64
        gen = get_generator(&mut random, scenario);
        assert!(*generator_buffer(&gen) == vector::empty(), 0);
        let output1 = generate_u64(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 24, 0);
        let output2 = generate_u64(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 16, 0);
        assert!(output1 != output2, 0);
        let _output3 = generate_u8(&mut gen);
        let _output4 = generate_u64(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 7, 0);
        let i = 0;
        while (i < 8) {
            let x = generate_u64(&mut gen);
            let x_bytes = bcs::to_bytes(&x);
            if (*vector::borrow(&x_bytes, i) != 0u8)
                i = i + 1;
        };

        // u32
        gen = get_generator(&mut random, scenario);
        assert!(*generator_buffer(&gen) == vector::empty(), 0);
        let output1 = generate_u32(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 28, 0);
        let output2 = generate_u32(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 24, 0);
        assert!(output1 != output2, 0);
        let _output3 = generate_u8(&mut gen);
        let _output4 = generate_u32(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 19, 0);
        let i = 0;
        while (i < 4) {
            let x = generate_u32(&mut gen);
            let x_bytes = bcs::to_bytes(&x);
            if (*vector::borrow(&x_bytes, i) != 0u8)
                i = i + 1;
        };

        // u16
        gen = get_generator(&mut random, scenario);
        assert!(*generator_buffer(&gen) == vector::empty(), 0);
        let output1 = generate_u16(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 30, 0);
        let output2 = generate_u16(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 28, 0);
        assert!(output1 != output2, 0);
        let _output3 = generate_u8(&mut gen);
        let _output4 = generate_u16(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 25, 0);
        let i = 0;
        while (i < 2) {
            let x = generate_u16(&mut gen);
            let x_bytes = bcs::to_bytes(&x);
            if (*vector::borrow(&x_bytes, i) != 0u8)
                i = i + 1;
        };

        // u8
        gen = get_generator(&mut random, scenario);
        assert!(*generator_buffer(&gen) == vector::empty(), 0);
        let output1 = generate_u8(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 31, 0);
        let output2 = generate_u8(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 30, 0);
        assert!(output1 != output2, 0);
        let _output3 = generate_u128(&mut gen);
        let _output4 = generate_u8(&mut gen);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 13, 0);
        while (true) {
            let x = generate_u8(&mut gen);
            if (x != 0u8)
                break
        };

        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_in_range() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random = test_scenario::take_shared<Random>(scenario);

        // generate_u128_in_range
        let gen = get_generator(&mut random, scenario);
        let output1 = generate_u128_in_range(&mut gen, 11, 123454321);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 8, 0);
        let output2 = generate_u128_in_range(&mut gen, 11, 123454321);
        assert!(generator_counter(&gen) == 2, 0);
        assert!(vector::length(generator_buffer(&gen)) == 16, 0);
        assert!(output1 != output2, 0);
        let output = generate_u128_in_range(&mut gen, 123454321, 123454321 + 1);
        assert!((output == 123454321) || (output == 123454321 + 1), 0);
        // test the edge case of u128_in_range (covers also the other in_range functions)
        let _output = generate_u128_in_range(&mut gen, 0, 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF);
        let i = 0;
        while (i < 50) {
            let min = generate_u128(&mut gen);
            let max = min + (generate_u64(&mut gen) as u128);
            let output = generate_u128_in_range(&mut gen, min, max);
            assert!(output >= min, 0);
            assert!(output <= max, 0);
            i = i + 1;
        };

        // generate_u64_in_range
        gen = get_generator(&mut random, scenario);
        let output1 = generate_u64_in_range(&mut gen, 11, 123454321);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 16, 0);
        let output2 = generate_u64_in_range(&mut gen, 11, 123454321);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 0, 0);
        assert!(output1 != output2, 0);
        let output = generate_u64_in_range(&mut gen, 123454321, 123454321 + 1);
        assert!((output == 123454321) || (output == 123454321 + 1), 0);
        let i = 0;
        while (i < 50) {
            let min = generate_u64(&mut gen);
            let max = min + (generate_u32(&mut gen) as u64);
            let output = generate_u64_in_range(&mut gen, min, max);
            assert!(output >= min, 0);
            assert!(output <= max, 0);
            i = i + 1;
        };

        // generate_u32_in_range
        gen = get_generator(&mut random, scenario);
        let output1 = generate_u32_in_range(&mut gen, 11, 123454321);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 20, 0);
        let output2 = generate_u32_in_range(&mut gen, 11, 123454321);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 8, 0);
        assert!(output1 != output2, 0);
        let output = generate_u32_in_range(&mut gen, 123454321, 123454321 + 1);
        assert!((output == 123454321) || (output == 123454321 + 1), 0);
        let i = 0;
        while (i < 50) {
            let min = generate_u32(&mut gen);
            let max = min + (generate_u16(&mut gen) as u32);
            let output = generate_u32_in_range(&mut gen, min, max);
            assert!(output >= min, 0);
            assert!(output <= max, 0);
            i = i + 1;
        };

        // generate_u16_in_range
        gen = get_generator(&mut random, scenario);
        let output1 = generate_u16_in_range(&mut gen, 11, 12345);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 22, 0);
        let output2 = generate_u16_in_range(&mut gen, 11, 12345);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 12, 0);
        assert!(output1 != output2, 0);
        let output = generate_u16_in_range(&mut gen, 12345, 12345 + 1);
        assert!((output == 12345) || (output == 12345 + 1), 0);
        let i = 0;
        while (i < 50) {
            let min = generate_u16(&mut gen);
            let max = min + (generate_u8(&mut gen) as u16);
            let output = generate_u16_in_range(&mut gen, min, max);
            assert!(output >= min, 0);
            assert!(output <= max, 0);
            i = i + 1;
        };

        // generate_u8_in_range
        gen = get_generator(&mut random, scenario);
        let output1 = generate_u8_in_range(&mut gen, 11, 123);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 23, 0);
        let output2 = generate_u8_in_range(&mut gen, 11, 123);
        assert!(generator_counter(&gen) == 1, 0);
        assert!(vector::length(generator_buffer(&gen)) == 14, 0);
        assert!(output1 != output2, 0);
        let output = generate_u8_in_range(&mut gen, 123, 123 + 1);
        assert!((output == 123) || (output == 123 + 1), 0);
        let i = 0;
        while (i < 50) {
            let (min, max) = (generate_u8(&mut gen), generate_u8(&mut gen));
            let (min, max) = if (min < max) { (min, max) } else { (max, min) };
            let (min, max) = if (min == max) { (min, max + 1) } else { (min, max) };
            let output = generate_u8_in_range(&mut gen, min, max);
            assert!(output >= min, 0);
            assert!(output <= max, 0);
            i = i + 1;
        };

        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = random::EInvalidRange)]
    fun test_invalid_range() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random = test_scenario::take_shared<Random>(scenario);
        let gen = get_generator(&mut random, scenario);
        let _output = generate_u128_in_range(&mut gen, 511, 500);

        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = random::EInvalidRange)]
    fun random_tests_invalid_range_same() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random = test_scenario::take_shared<Random>(scenario);
        let gen = get_generator(&mut random, scenario);
        let _output = generate_u32_in_range(&mut gen, 123, 123);

        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = random::EInvalidRandomnessUpdate)]
    fun test_duplicate() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random = test_scenario::take_shared<Random>(scenario);
        update_random(&mut random,  1, RANDOMNESS1, test_scenario::ctx(scenario));
        update_random(&mut random,  1, RANDOMNESS1, test_scenario::ctx(scenario));

        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = random::EInvalidRandomnessUpdate)]
    fun test_out_of_order() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random = test_scenario::take_shared<Random>(scenario);
        update_random(&mut random,  1, RANDOMNESS1, test_scenario::ctx(scenario));
        update_random(&mut random,  3, RANDOMNESS1, test_scenario::ctx(scenario));

        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }
}
