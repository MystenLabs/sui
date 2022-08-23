// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Library for cryptography onchain.
module sui::crypto {
    friend sui::validator;

    /// @param signature: A 65-bytes signature in form (r, s, v) that is signed using 
    /// Secp256k1. Reference implementation on signature generation using RFC6979: 
    /// https://github.com/MystenLabs/narwhal/blob/5d6f6df8ccee94446ff88786c0dbbc98be7cfc09/crypto/src/secp256k1.rs
    /// 
    /// @param hashed_msg: the hashed 32-bytes message. The message must be hashed instead 
    /// of plain text to be secure.
    /// 
    /// If the signature is valid, return the corresponding recovered Secpk256k1 public 
    /// key, otherwise throw error. This is similar to ecrecover in Ethereum, can only be 
    /// applied to Secp256k1 signatures.
    public native fun ecrecover(signature: vector<u8>, hashed_msg: vector<u8>): vector<u8>;

    /// @param data: arbitrary bytes data to hash
    /// Hash the input bytes using keccak256 and returns 32 bytes.
    public native fun keccak256(data: vector<u8>): vector<u8>;

    /// @param signature: A 48-bytes signature that is a point on the G1 subgroup
    /// @param public_key: A 96-bytes public key that is a point on the G2 subgroup
    /// @param msg: The message that we test the signature against.
    ///
    /// If the signature is a valid BLS12381 signature of the message and public key, return true.
    /// Otherwise, return false.
    public native fun bls12381_verify_g1_sig(signature: vector<u8>, public_key: vector<u8>, msg: vector<u8>): bool; 

    /// @param signature: A 65-bytes signature in form (r, s, v) that is signed using 
    /// Secp256k1. Reference implementation on signature generation using RFC6979: 
    /// https://github.com/MystenLabs/narwhal/blob/5d6f6df8ccee94446ff88786c0dbbc98be7cfc09/crypto/src/secp256k1.rs
    /// 
    /// @param public_key: The public key to verify the signature against
    /// @param hashed_msg: The hashed 32-bytes message, same as what the signature is signed against.
    /// 
    /// If the signature is valid to the pubkey and hashed message, return true. Else false.
    public native fun secp256k1_verify(signature: vector<u8>, public_key: vector<u8>, hashed_msg: vector<u8>): bool;

    use sui::elliptic_curve::{Self as ec, RistrettoPoint};

    /// Only bit_length = 64, 32, 16, 8 will work.
    native fun native_verify_full_range_proof(proof: vector<u8>, commitment: vector<u8>, bit_length: u64);

    /// @param proof: The bulletproof
    /// @param commitment: The commitment which we are trying to verify the range proof for
    /// @param bit_length: The bit length that we prove the committed value is whithin. Note that bit_length must be either 64, 32, 16, or 8.
    /// 
    /// If the range proof is valid, execution succeeds, else panics.
    public fun verify_full_range_proof(proof: vector<u8>, commitment: RistrettoPoint, bit_length: u64) {
        native_verify_full_range_proof(proof, ec::bytes(&commitment), bit_length)
    }

    /// @param signature: 32-byte signature that is a point on the Ed25519 elliptic curve.
    /// @param public_key: 32-byte signature that is a point on the Ed25519 elliptic curve.
    /// @param msg: The message that we test the signature against.
    /// 
    /// If the signature is a valid BLS12381 signature of the message and public key, return true.
    /// Otherwise, return false.
    public(friend) native fun ed25519_verify(signature: vector<u8>, public_key: vector<u8>, msg: vector<u8>): bool;

    /// @param signature: 32-byte signature that is a point on the Ed25519 elliptic curve.
    /// @param public_key: 32-byte signature that is a point on the Ed25519 elliptic curve.
    /// @param msg: The message that we test the signature against.
    /// @param domain: The domain that the signature is tested again. We essentially prepend this to the message.
    /// 
    /// If the signature is a valid BLS12381 signature of the message and public key, return true.
    /// Otherwise, return false.
    public(friend) fun ed25519_verify_with_domain(signature: vector<u8>, public_key: vector<u8>, msg: vector<u8>, domain: vector<u8>): bool {
        std::vector::append(&mut domain, msg);
        ed25519_verify(signature, public_key, domain)
    }
}
