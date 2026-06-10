// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::ecdsa_p384_tests;

use sui::ecdsa_p384;

// Deterministic vectors generated from a fixed P-384 key over `msg()`. They match the
// known-answer test in `sui-types/src/ecdsa_p384.rs`.
fun msg(): vector<u8> {
    b"Sui ecdsa_p384 native test message"
}

fun public_key(): vector<u8> {
    x"0272ccde33753762245e015da92e48fa028495522dc42356c7e3df51dcf56a5e19de742acd3a19f79af372dc9705f560d8"
}

fun sha256_sig(): vector<u8> {
    x"c8ccb0f019761836eaf7d3b1d200fc79e0330c7741a28b3fb727d376600668b6308963ecce8c2baf9021af0b0353c7e9411cd0604dfe6503330c6103adfbf80a99cdb9fd2d9e3ead25d06d03b6a8f67cb8ff8d42daa268f5ce37c3c79f0510df"
}

fun sha384_sig(): vector<u8> {
    x"b1d61cff40b11cf6963e41b820b71a44393f63cae1a91663921477dc58f416c5a9ca099a96994b7748740fae52df2eeb432176564578358107ace808077053f9bc61001e49430b153cf7eef56e327add63c007b01d618c6d219bf1355e592edf"
}

#[test]
fun test_secp384r1_verify_sha256_success() {
    assert!(ecdsa_p384::secp384r1_verify(&sha256_sig(), &public_key(), &msg(), 0));
}

#[test]
fun test_secp384r1_verify_sha384_success() {
    assert!(ecdsa_p384::secp384r1_verify(&sha384_sig(), &public_key(), &msg(), 1));
}

#[test]
fun test_secp384r1_verify_wrong_message() {
    assert!(!ecdsa_p384::secp384r1_verify(&sha256_sig(), &public_key(), &b"wrong message", 0));
}

#[test]
fun test_secp384r1_verify_mismatched_hash() {
    // A SHA-256 signature must not verify under the SHA-384 flag, and vice versa.
    assert!(!ecdsa_p384::secp384r1_verify(&sha256_sig(), &public_key(), &msg(), 1));
    assert!(!ecdsa_p384::secp384r1_verify(&sha384_sig(), &public_key(), &msg(), 0));
}

#[test]
fun test_secp384r1_verify_invalid_hash_flag() {
    assert!(!ecdsa_p384::secp384r1_verify(&sha256_sig(), &public_key(), &msg(), 2));
}

#[test]
fun test_secp384r1_verify_malformed_inputs() {
    let truncated_key = x"0272ccde33753762245e015da92e48fa028495522dc42356c7e3df51dcf56a5e19";
    let truncated_sig = x"c8ccb0f019761836eaf7d3b1d200fc79";
    assert!(!ecdsa_p384::secp384r1_verify(&sha256_sig(), &truncated_key, &msg(), 0));
    assert!(!ecdsa_p384::secp384r1_verify(&truncated_sig, &public_key(), &msg(), 0));
}
