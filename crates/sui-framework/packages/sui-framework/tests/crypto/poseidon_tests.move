// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::poseidon_tests {
    use std::vector;
    use sui::poseidon::poseidon_bn254;

    #[test]
    fun test_poseidon_bn254_hash() {
        let msg = vector[1u256];
        let expected = 18586133768512220936620570745912940619677854269274689475585506675881198879027u256;
        let actual = poseidon_bn254(&msg);
        assert!(actual == expected, 0);

        let msg = vector[1u256, 2u256];
        let expected = 7853200120776062878684798364095072458815029376092732009249414926327459813530u256;
        let actual = poseidon_bn254(&msg);
        assert!(actual == expected, 1);
    }

    #[test]
    #[expected_failure(abort_code = sui::poseidon::ETooManyInputs)]
    fun test_poseidon_bn254_too_many_inputs() {
        let msg = vector[1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256, 1u256];
        assert!(vector::length(&msg) > 32, 0);
        let _ = poseidon_bn254(&msg);
    }

    #[test]
    #[expected_failure(abort_code = sui::poseidon::ENonCanonicalInput)]
    fun test_poseidon_bn254_non_canonical_input() {
        let msg = vector[21888242871839275222246405745257275088696311157297823662689037894645226208583u256];
        let _ = poseidon_bn254(&msg);
    }
}