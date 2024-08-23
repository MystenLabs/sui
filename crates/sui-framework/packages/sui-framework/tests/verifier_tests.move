// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Tests if normally illegal (in terms of Sui bytecode verification) code is allowed in tests.
module sui::verifier_tests {
    public struct VERIFIER_TESTS has drop {}

    fun init(otw: VERIFIER_TESTS, _: &mut sui::tx_context::TxContext) {
        assert!(sui::types::is_one_time_witness(&otw));
    }

    #[test]
    fun test_init() {
        use sui::test_scenario;
        let admin = @0xBABE;

        let mut scenario = test_scenario::begin(admin);
        let otw = VERIFIER_TESTS{};
        init(otw, scenario.ctx());
        scenario.end();
    }

    fun is_otw(witness: VERIFIER_TESTS): bool {
        sui::types::is_one_time_witness(&witness)
    }

    #[test]
    fun test_otw() {
        // we should be able to construct otw in test code
        let otw = VERIFIER_TESTS{};
        assert!(is_otw(otw));
    }

}
