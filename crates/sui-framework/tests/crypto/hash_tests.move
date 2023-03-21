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
    }

    #[test]
    fun test_blake2b256_hash() {
        let msg = b"hello world!";
        let hashed_msg_bytes = x"4fccfb4d98d069558aa93e9565f997d81c33b080364efd586e77a433ddffc5e2";
        let hashed_msg = hash::blake2b256(&msg);
        assert!(hashed_msg == hashed_msg_bytes, 0);
    }

}