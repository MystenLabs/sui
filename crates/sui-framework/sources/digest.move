// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Sui types for message digests.
module sui::digest {
    use std::vector;

    /// Length of the vector<u8> representing a SHA256 digest.
    const SHA256_DIGEST_VECTOR_LENGTH: u64 = 32;

    /// Error code when the length of the digest vector is invalid.
    const EHashLengthMismatch: u64 = 0;

    /// Sha256Digest: An immutable wrapper of SHA256_DIGEST_VECTOR_LENGTH bytes.
    struct Sha256Digest has store, copy, drop {
        digest: vector<u8>,
    }

    /// Create a `Sha256Digest` from bytes. Aborts if `bytes` is not of length 32.
    public fun new_sha256_digest(digest: vector<u8>): Sha256Digest {
        assert!(vector::length(&digest) == SHA256_DIGEST_VECTOR_LENGTH, EHashLengthMismatch);
        Sha256Digest { digest }
    }

    /// Get the digest.
    public fun sha256_digest(self: &Sha256Digest): vector<u8> {
        self.digest
    }
}
