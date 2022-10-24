// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::hash {
    use sui::digest;

    /// @param key: HMAC key, arbitrary bytes.
    /// @param msg: message to sign, arbitrary bytes.
    /// A native move wrapper around the HMAC-SHA3-256. Returns the digest.
    native fun native_hmac_sha3_256(key: &vector<u8>, msg: &vector<u8>): vector<u8>;

    /// @param key: HMAC key, arbitrary bytes.
    /// @param msg: message to sign, arbitrary bytes.
    /// Returns the 32 bytes digest of HMAC-SHA3-256(key, msg).
    public fun hmac_sha3_256(key: &vector<u8>, msg: &vector<u8>): digest::Sha3256Digest {
        digest::sha3_256_digest_from_bytes(native_hmac_sha3_256(key, msg))
    }
}
