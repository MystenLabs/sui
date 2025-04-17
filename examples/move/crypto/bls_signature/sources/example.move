// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Examples of BLS signature related operations.
module bls_signature::example;

use std::hash::sha2_256;
use sui::{bcs, bls12381};

/// Verification of a BLS signature using EC operations (implements bls12381::bls12381_min_sig_verify in Move).
public fun bls_min_sig_verify(msg: &vector<u8>, pk: &vector<u8>, sig: &vector<u8>): bool {
    let pk = bls12381::g2_from_bytes(pk);
    let sig = bls12381::g1_from_bytes(sig);
    let hashed_msg = bls12381::hash_to_g1(msg);
    let lhs = bls12381::pairing(&hashed_msg, &pk);
    let rhs = bls12381::pairing(&sig, &bls12381::g2_generator());
    lhs.equal(&rhs)
}

#[test]
fun test_bls_min_sig_verify() {
    let msg = x"0101010101";
    let pk =
        x"8df101606f91f3cad7f54b8aff0f0f64c41c482d9b9f9fe81d2b607bc5f611bdfa8017cf04b47b44b222c356ef555fbd11058c52c077f5a7ec6a15ccfd639fdc9bd47d005a111dd6cdb8c02fe49608df55a3c9822986ad0b86bdea3abfdfe464";
    let sig =
        x"908e345f2e2803cd941ae88c218c96194233c9053fa1bca52124787d3cca141c36429d7652435a820c72992d5eee6317";
    assert!(bls_min_sig_verify(&msg, &pk, &sig), 0);
}

////////////////////////////////
/// Using drand in Move

/// The public key of chain 52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971.
const DRAND_PK: vector<u8> =
    x"83cf0f2896adee7eb8b5f01fcad3912212c437e0073e911fb90022d3e760183c8c4b450b6a0a6c3ac6a5776a2d1064510d1fec758c921cc22b0e17e63aaf4bcb5ed66304de9cf809bd274ca73bab4af5a6e9c76a4bc09e76eae8991ef5ece45a";

/// Check a drand output.
public fun verify_drand_signature(sig: vector<u8>, round: u64): bool {
    // Convert round to a byte array in big-endian order.
    let mut round_bytes = bcs::to_bytes(&round);
    round_bytes.reverse();
    // Compute sha256(prev_sig, round_bytes).
    let digest = sha2_256(round_bytes);
    // Verify the signature on the hash.
    let drand_pk = DRAND_PK;
    bls12381::bls12381_min_sig_verify(&sig, &drand_pk, &digest)
}

/// Derive a uniform vector from a drand signature.
public fun derive_randomness(drand_sig: vector<u8>): vector<u8> {
    sha2_256(drand_sig)
}

#[test]
fun test_drand_input() {
    // curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/10
    let round = 10;
    let sig =
        x"ac415e508c484053efed1c6c330e3ae0bf20185b66ed088864dac1ff7d6f927610824986390d3239dac4dd73e6f865f5";
    assert!(verify_drand_signature(sig, round), 0);
    let _ = derive_randomness(sig);
}
