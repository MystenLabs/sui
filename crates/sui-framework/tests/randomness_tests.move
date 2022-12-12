// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::randomness_tests {
    use sui::randomness;
    use sui::test_scenario;
    use sui::tx_context;
    use std::option;

    const TEST_USER1_ADDR: address = @0xA11CE;
    const TEST_USER2_ADDR: address = @0xA12CE;

    struct WITENESS has drop {}

    #[test]
    fun test_tbls() {
        let scenario_val = test_scenario::begin(TEST_USER1_ADDR);
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
    fun test_tbls_invalid_signature() {
        let scenario_val = test_scenario::begin(TEST_USER1_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);

        let r1 = randomness::new(WITENESS {}, ctx);
        let r2 = randomness::new(WITENESS {}, ctx);
        let sig = randomness::sign(&r2);
        // Signature should be invalid.
        randomness::set(&mut r1, sig);
        abort 42 // never reached.
    }
}
