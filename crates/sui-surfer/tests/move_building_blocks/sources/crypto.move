// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Wraps the cryptographic native functions in monomorphic entry functions so the
/// surfer can fuzz them with random byte vectors. Failures (aborts / `false`
/// results) are expected and intentionally exercise the natives' error paths.
module move_building_blocks::crypto {
    use sui::bls12381;
    use sui::ecdsa_k1;
    use sui::ecdsa_r1;
    use sui::ecvrf;
    use sui::ed25519;
    use sui::hash;
    use sui::hmac;
    use sui::poseidon;
    use std::hash as std_hash;

    public fun do_blake2b256(data: vector<u8>) {
        let _ = hash::blake2b256(&data);
    }

    public fun do_keccak256(data: vector<u8>) {
        let _ = hash::keccak256(&data);
    }

    public fun do_sha2_256(data: vector<u8>) {
        let _ = std_hash::sha2_256(data);
    }

    public fun do_sha3_256(data: vector<u8>) {
        let _ = std_hash::sha3_256(data);
    }

    public fun do_hmac_sha3_256(key: vector<u8>, msg: vector<u8>) {
        let _ = hmac::hmac_sha3_256(&key, &msg);
    }

    public fun do_ed25519_verify(signature: vector<u8>, public_key: vector<u8>, msg: vector<u8>) {
        let _ = ed25519::ed25519_verify(&signature, &public_key, &msg);
    }

    public fun do_secp256k1_verify(
        signature: vector<u8>,
        public_key: vector<u8>,
        msg: vector<u8>,
        hash_kind: u8,
    ) {
        let _ = ecdsa_k1::secp256k1_verify(&signature, &public_key, &msg, hash_kind);
    }

    public fun do_secp256r1_verify(
        signature: vector<u8>,
        public_key: vector<u8>,
        msg: vector<u8>,
        hash_kind: u8,
    ) {
        let _ = ecdsa_r1::secp256r1_verify(&signature, &public_key, &msg, hash_kind);
    }

    public fun do_ecvrf_verify(
        hash_bytes: vector<u8>,
        alpha_string: vector<u8>,
        public_key: vector<u8>,
        proof: vector<u8>,
    ) {
        let _ = ecvrf::ecvrf_verify(&hash_bytes, &alpha_string, &public_key, &proof);
    }

    public fun do_bls12381_min_sig_verify(
        signature: vector<u8>,
        public_key: vector<u8>,
        msg: vector<u8>,
    ) {
        let _ = bls12381::bls12381_min_sig_verify(&signature, &public_key, &msg);
    }

    public fun do_bls12381_min_pk_verify(
        signature: vector<u8>,
        public_key: vector<u8>,
        msg: vector<u8>,
    ) {
        let _ = bls12381::bls12381_min_pk_verify(&signature, &public_key, &msg);
    }

    public fun do_poseidon_bn254(data: vector<u256>) {
        // poseidon_bn254 aborts on empty input, so only call it with a payload.
        if (!data.is_empty()) {
            let _ = poseidon::poseidon_bn254(&data);
        }
    }
}
