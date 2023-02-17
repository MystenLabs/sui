// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::ecdsa_k1 {

    // TODO document this
    const EFailToRecoverPubKey: u64 = 0;
    const EInvalidSignature: u64 = 1;

    /// @param signature: A 65-bytes signature in form (r, s, v) that is signed using
    /// Secp256k1. Reference implementation on signature generation using RFC6979:
    /// https://github.com/MystenLabs/narwhal/blob/5d6f6df8ccee94446ff88786c0dbbc98be7cfc09/crypto/src/secp256k1.rs
    /// The accepted v values are {0, 1, 2, 3}.
    ///
    /// @param hashed_msg: the hashed 32-bytes message. The message must be hashed instead
    /// of plain text to be secure.
    ///
    /// If the signature is valid, return the corresponding recovered Secpk256k1 public
    /// key, otherwise throw error. This is similar to ecrecover in Ethereum, can only be
    /// applied to Secp256k1 signatures.
    public native fun ecrecover(signature: &vector<u8>, hashed_msg: &vector<u8>): vector<u8>;

    /// @param pubkey: A 33-bytes compressed public key, a prefix either 0x02 or 0x03 and a 256-bit integer.
    ///
    /// If the compressed public key is valid, return the 65-bytes uncompressed public key,
    /// otherwise throw error.
    public native fun decompress_pubkey(pubkey: &vector<u8>): vector<u8>;

    /// @param signature: A 64-bytes signature in form (r, s) that is signed using
    /// Secp256k1. This is an non-recoverable signature without recovery id.
    /// Reference implementation on signature generation using RFC6979:
    /// https://github.com/MystenLabs/fastcrypto/blob/74aec4886e62122a5b769464c2bea5f803cf8ecc/fastcrypto/src/secp256k1/mod.rs#L193
    ///
    /// @param public_key: The public key to verify the signature against
    /// @param hashed_msg: The hashed 32-bytes message, same as what the signature is signed against.
    ///
    /// If the signature is valid to the pubkey and hashed message, return true. Else false.
    public native fun secp256k1_verify(signature: &vector<u8>, public_key: &vector<u8>, hashed_msg: &vector<u8>): bool;

    /// @param signature: A 65-bytes signature in form (r, s, v) that is signed using
    /// Secp256k1. This is an recoverable signature with recovery id denoted as v.
    /// Reference implementation on signature generation using RFC6979:
    /// https://github.com/MystenLabs/fastcrypto/blob/74aec4886e62122a5b769464c2bea5f803cf8ecc/fastcrypto/src/secp256k1/mod.rs#L193
    ///
    /// @param public_key: The public key to verify the signature against
    /// @param hashed_msg: The hashed 32-bytes message, same as what the signature is signed against.
    ///
    /// If the signature is valid to the pubkey and hashed message, return true. Else false.
    public native fun secp256k1_verify_recoverable(signature: &vector<u8>, public_key: &vector<u8>, hashed_msg: &vector<u8>): bool;
}
