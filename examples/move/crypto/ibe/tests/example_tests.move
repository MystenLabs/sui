// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module ibe::tests;

use ibe::example;
use sui::{bcs, bls12381};

#[test_only]
use std::hash::sha2_256;
#[test_only]
use sui::test_utils::assert_eq;

// This test emulates drand based timelock encryption (using quicknet).
#[test]
fun test_ibe_decrypt_drand() {
    // Retrieved using 'curl https://api.drand.sh/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/info'
    let round = 1234;
    let pk_bytes =
        x"83cf0f2896adee7eb8b5f01fcad3912212c437e0073e911fb90022d3e760183c8c4b450b6a0a6c3ac6a5776a2d1064510d1fec758c921cc22b0e17e63aaf4bcb5ed66304de9cf809bd274ca73bab4af5a6e9c76a4bc09e76eae8991ef5ece45a";
    let pk = bls12381::g2_from_bytes(&pk_bytes);
    let msg = x"0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF";

    // Derive the 'target' for the specific round (see drand_lib.move).
    let mut round_bytes = bcs::to_bytes(&round);
    round_bytes.reverse();
    let target = sha2_256(round_bytes);

    // Retrieved with 'curl https://api.drand.sh/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/1234'.
    let sig_bytes =
        x"a81d4aad15461a0a02b43da857be1d782a2232a3c7bb370a2763e95ce1f2628460b24de2cee7453cd12e43c197ea2f23";
    let target_key = bls12381::g1_from_bytes(&sig_bytes);
    assert!(bls12381::bls12381_min_sig_verify(&sig_bytes, &pk_bytes, &target), 0);

    // Encrypt and decrypt using the insecure encryption.
    let enc = example::insecure_ibe_encrypt(
        &pk,
        &target,
        &msg,
        &x"A123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF",
    );
    let mut decrypted_msg = example::ibe_decrypt(enc, &target_key);
    assert!(option::extract(&mut decrypted_msg) == msg, 0);

    // Test an fixed output that was generated using the FastCrypto CLI.
    let enc =
        x"b598e92e55ac1a2e78b61ce4a223c3c6b17db2dc4e5c807965649d882c71f05e1a7eac110e40c7b7faae4d556d6b418c03521e351504b371e91c1e7637292e4fb9f7ad4a8b6a1fecebd2b3208e18cab594b081d11cbfb1f15b7b18b4af6876fd796026a67def0b05222aadabcf86eaace0e708f469f491483f681e184f9178236f4e749635de4478f3bf44fb9264d35d6e83d58b3e5e686414b0953e99142a62";
    let enc = example::from_bytes(enc);
    let mut decrypted_msg = example::ibe_decrypt(enc, &target_key);
    assert!(option::extract(&mut decrypted_msg) == msg, 0);
}

#[test]
fun test_try_substract_and_modulo() {
    let smaller: vector<u8> = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000";
    let res = example::try_substract(&smaller);
    assert!(option::is_none(&res), 0);

    let bigger: vector<u8> = x"8c1258acd66282b7ccc627f7f65e27faac425bfd0001a40100000000fffffff5";
    let res = example::try_substract(&bigger);
    assert!(option::is_some(&res), 0);
    let bigger_minus_order = *option::borrow(&res);
    let expected: vector<u8> = x"1824b159acc5056f998c4fefecbc4ff55884b7fa0003480200000001fffffff4";
    assert_eq(bigger_minus_order, expected);

    let larger: vector<u8> = x"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff6";
    let expected: vector<u8> = x"1824b159acc5056f998c4fefecbc4ff55884b7fa0003480200000001fffffff4";
    let modulo = example::modulo_order(&larger);
    assert!(modulo == expected, 0);
}
