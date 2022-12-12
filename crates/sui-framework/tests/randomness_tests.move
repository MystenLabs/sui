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

        test_scenario::next_tx(scenario, TEST_USER1_ADDR);
        let r = randomness::new(WITENESS {}, ctx);
        assert!(randomness::epoch(&r) == tx_context::epoch(ctx));
        assert!(option::is_none(randomness::value(&r)));

        // how to set pk and sign with sk?
        // randomness::set(&mut r, []);

        randomness::destroy(r);

        test_scenario::end(scenario_val);
    }
}
