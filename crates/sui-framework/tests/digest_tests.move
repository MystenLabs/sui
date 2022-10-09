// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::digest_tests {
    use sui::digest::{new_sha256_digest, sha256_digest};

    const EHASH_LENGTH_MISMATCH: u64 = 0;

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_too_short_hash() {
        let hash = x"badf012345";
        let _ = new_sha256_digest(hash);
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_too_long_hash() {
        let hash = x"1234567890123456789012345678901234567890abcdefabcdefabcdefabcdef123456";
        let _ = new_sha256_digest(hash);
    }

    #[test]
    fun test_good_hash() {
        let hash = x"1234567890123456789012345678901234567890abcdefabcdefabcdefabcdef";
        let digest = new_sha256_digest(hash);
        assert!(sha256_digest(&digest) == hash, EHASH_LENGTH_MISMATCH);
    }
}
