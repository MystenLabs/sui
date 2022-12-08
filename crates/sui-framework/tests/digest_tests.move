// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::digest_tests {
    use sui::digest::{sha3_256_digest_from_bytes, sha3_256_digest_to_bytes};

    const EHASH_LENGTH_MISMATCH: u64 = 0;

    #[test]
    #[expected_failure(abort_code = sui::digest::EHashLengthMismatch)]
    fun test_too_short_hash() {
        let hash = x"badf012345";
        let _ = sha3_256_digest_from_bytes(hash);
    }

    #[test]
    #[expected_failure(abort_code = sui::digest::EHashLengthMismatch)]
    fun test_too_long_hash() {
        let hash = x"1234567890123456789012345678901234567890abcdefabcdefabcdefabcdef123456";
        let _ = sha3_256_digest_from_bytes(hash);
    }

    #[test]
    fun test_good_hash() {
        let hash = x"1234567890123456789012345678901234567890abcdefabcdefabcdefabcdef";
        let digest = sha3_256_digest_from_bytes(hash);
        assert!(sha3_256_digest_to_bytes(&digest) == hash, EHASH_LENGTH_MISMATCH);
    }
}
