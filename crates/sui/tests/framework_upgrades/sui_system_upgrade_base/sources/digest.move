// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Sui types for message digests.
module sui::digest {
    use std::vector;

    /// Length of the vector<u8> representing a SHA3-256 digest.
    const SHA3_256_DIGEST_VECTOR_LENGTH: u64 = 32;

    /// Error code when the length of the digest vector is invalid.
    const EHashLengthMismatch: u64 = 0;

    /// Sha3256Digest: An immutable wrapper of SHA3_256_DIGEST_VECTOR_LENGTH bytes.
    struct Sha3256Digest has store, copy, drop {
        digest: vector<u8>,
    }

    /// Create a `Sha3256Digest` from bytes. Aborts if `bytes` is not of length 32.
    public fun sha3_256_digest_from_bytes(digest: vector<u8>): Sha3256Digest {
        assert!(vector::length(&digest) == SHA3_256_DIGEST_VECTOR_LENGTH, EHashLengthMismatch);
        Sha3256Digest { digest }
    }

    /// Get the digest.
    public fun sha3_256_digest_to_bytes(self: &Sha3256Digest): vector<u8> {
        self.digest
    }
}
