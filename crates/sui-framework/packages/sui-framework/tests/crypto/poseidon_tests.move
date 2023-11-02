// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::poseidon_tests {
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
}