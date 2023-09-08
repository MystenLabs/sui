// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::hash_tests {
    use sui::hash;

    #[test]
    fun test_keccak256_hash() {
        let msg = b"hello world!";
        let hashed_msg_bytes = x"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
        let hashed_msg = hash::keccak256(&msg);
        assert!(hashed_msg == hashed_msg_bytes, 0);

        let empty_msg = b"";
        let _ = hash::keccak256(&empty_msg);
        let long_msg = b"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
        let _ = hash::keccak256(&long_msg);
    }

    #[test]
    fun test_blake2b256_hash() {
        let msg = b"hello world!";
        let hashed_msg_bytes = x"4fccfb4d98d069558aa93e9565f997d81c33b080364efd586e77a433ddffc5e2";
        let hashed_msg = hash::blake2b256(&msg);
        assert!(hashed_msg == hashed_msg_bytes, 0);

        let empty_msg = b"";
        let _ = hash::blake2b256(&empty_msg);
        let long_msg = b"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
        let _ = hash::blake2b256(&long_msg);
    }

    #[test]
    fun test_poseidon_bn254_hash() {
        let msg = vector[1u256];
        let expected = 18586133768512220936620570745912940619677854269274689475585506675881198879027u256;
        let actual = hash::poseidon_bn254(&msg);
        assert!(actual == expected, 0);

        let msg = vector[1u256, 2u256];
        let expected = 7853200120776062878684798364095072458815029376092732009249414926327459813530u256;
        let actual = hash::poseidon_bn254(&msg);
        assert!(actual == expected, 1);
    }
}