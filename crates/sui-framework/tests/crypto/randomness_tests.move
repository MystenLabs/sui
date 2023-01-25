// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::randomness_tests {
    use std::option;
    use sui::randomness;
    use sui::test_scenario;
    use sui::tx_context;
    use std::vector;

    const TEST_USER_ADDR: address = @0xA11CE;

    struct WITNESS has drop {}

    #[test]
    fun test_tbls_happy_flow() {
        let scenario_val = test_scenario::begin(TEST_USER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);

        // Create a new Randomness
        let r = randomness::new(WITENESS {}, ctx);
        assert!(randomness::epoch(&r) == tx_context::epoch(ctx), 0);
        assert!(option::is_none(randomness::value(&r)), 0);

        // Get a valid signature and set the object.
        let sig = randomness::sign(&r);
        randomness::set(&mut r, sig);
        assert!(option::is_some(randomness::value(&r)), 0);

        randomness::destroy(r);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = randomness::EInvalidSignature)]
    fun test_tbls_wrong_signature() {
        let scenario_val = test_scenario::begin(TEST_USER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);

        let r1 = randomness::new(WITENESS {}, ctx);
        let r2 = randomness::new(WITENESS {}, ctx);

        // Signature should be invalid since we use the the other object.
        let sig = randomness::sign(&r2);
        randomness::set(&mut r1, sig);
        abort 42 // never reached.
    }

    #[test]
    #[expected_failure(abort_code = randomness::EInvalidSignature)]
    fun test_tbls_invalid_format() {
        let scenario_val = test_scenario::begin(TEST_USER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);

        let r = randomness::new(WITENESS {}, ctx);
        let sig = randomness::sign(&r);

        // Signature should be invalid because the deserialization would fail.
        vector::remove(&mut sig, 1);
        randomness::set(&mut r, sig);
        abort 42 // never reached.
    }
}
