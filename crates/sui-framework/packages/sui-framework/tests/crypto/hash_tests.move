// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::hash_tests;

use sui::hash;

#[test]
fun test_keccak256_hash() {
    let msg = b"hello world!";
    let hashed_msg_bytes = x"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
    let hashed_msg = hash::keccak256(&msg);
    assert!(hashed_msg == hashed_msg_bytes);

    let empty_msg = b"";
    let _ = hash::keccak256(&empty_msg);
    let long_msg =
        b"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
    let _ = hash::keccak256(&long_msg);
}

#[test]
fun test_blake2b256_hash() {
    let msg = b"hello world!";
    let hashed_msg_bytes = x"4fccfb4d98d069558aa93e9565f997d81c33b080364efd586e77a433ddffc5e2";
    let hashed_msg = hash::blake2b256(&msg);
    assert!(hashed_msg == hashed_msg_bytes);

    let empty_msg = b"";
    let _ = hash::blake2b256(&empty_msg);
    let long_msg =
        b"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
    let _ = hash::blake2b256(&long_msg);
}

#[test]
fun test_sha3_512_hash() {
    let msg = b"hello world!";
    let hashed_msg_bytes = x"5aadcaf394961eecc2f4e65c2d82ff7cf0f6fa4574f351d0053574886ac77c961958cef64bc2bb483b4e7430964b55893a7c28a5c6efab7e24e2b7994bba5eb9";
    let hashed_msg = hash::blake2b256(&msg);
    assert!(hashed_msg == hashed_msg_bytes);

    let empty_msg = b"";
    let _ = hash::blake2b256(&empty_msg);
    let long_msg =
        b"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd657caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
    let _ = hash::blake2b256(&long_msg);
}