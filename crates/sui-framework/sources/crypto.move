// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Library for cryptography onchain.
module sui::crypto {
    use std::vector;

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

    /// @param proof: The bulletproof
    /// @param commitment: The commitment which we are trying to verify the range proof for
    native fun native_verify_full_range_proof(proof: vector<u8>, commitment: vector<u8>);

    /// Ristretto Point is currently stored as bytes and deserialized for every command used
    struct RistrettoPoint has copy, drop, store {
        value: vector<u8>
    }

    public fun ristretto_from_bytes(bytes: vector<u8>): RistrettoPoint {
        assert!(vector::length(&bytes) == 32, 1);
        RistrettoPoint {
            value: bytes
        }
    }

    /// Private

    /// Public
    /// @param value: The value to commit to
    /// @param blinding_factor: A random number used to ensure that the commitment is hiding.
    native fun native_create_pedersen_commitment(value: vector<u8>, blinding_factor: vector<u8>): vector<u8>;

    /// @param self: bytes representation of an EC point on the Ristretto curve
    /// @param other: bytes representation of an EC point on the Ristretto curve
    /// A native move wrapper around the addition of Ristretto points. Returns self + other.
    native fun native_add_ristretto_point(point1: vector<u8>, point2: vector<u8>): vector<u8>;

    /// @param self: bytes representation of an EC point on the Ristretto curve
    /// @param other: bytes representation of an EC point on the Ristretto curve
    /// A native move wrapper around the subtraction of Ristretto points. Returns self - other.
    native fun native_subtract_ristretto_point(point1: vector<u8>, point2: vector<u8>): vector<u8>;

    struct BigScalar has copy, drop, store {
        value: vector<u8>
    }

    // This is pretty lazy, we should ideally have a Move-native unified BigInt library - but this can be added in a future refactor.
    native fun native_big_scalar_from_u64(value: u64): vector<u8>;

    public fun big_scalar_from_u64(value: u64): BigScalar {
        BigScalar {
            value: native_big_scalar_from_u64(value)
        }
    }

    public fun create_pedersen_commitment(value: vector<u8>, blinding_factor: vector<u8>): RistrettoPoint {
        return RistrettoPoint {
            value: native_create_pedersen_commitment(value, blinding_factor)
        }
    }

    public fun value(self: &RistrettoPoint): vector<u8> {
        self.value
    }

    public fun big_scalar_from_bytes(value: vector<u8>): BigScalar {
        assert!(vector::length(&value) == 32, 1);

        BigScalar {
            value
        }
    }

    public fun big_scalar_to_vec(self: BigScalar): vector<u8> {
        self.value
    }

    public fun add_ristretto_point(self: &RistrettoPoint, other: &RistrettoPoint): RistrettoPoint {
        RistrettoPoint {
            value: native_add_ristretto_point(self.value, other.value)
        }
    }

    public fun subtract_ristretto_point(self: &RistrettoPoint, other: &RistrettoPoint): RistrettoPoint {
        RistrettoPoint {
            value: native_subtract_ristretto_point(self.value, other.value)
        }
    }

    public fun verify_full_range_proof(proof: vector<u8>, commitment: RistrettoPoint) {
        native_verify_full_range_proof(proof, commitment.value)
    }
}
