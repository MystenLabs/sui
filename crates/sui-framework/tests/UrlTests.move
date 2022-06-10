// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::UrlTests {
    use Sui::Url;
    use std::ascii::Self;

    const EHASH_LENGTH_MISMATCH: u64 = 0;
    const URL_STRING_MISMATCH: u64 = 1;

    #[test]
    fun test_basic_url() {
        // url strings are not currently validated
        let url_str = ascii::string(x"414243454647");

        let url = Url::new_unsafe(url_str);
        assert!(Url::inner_url(&url) == url_str, URL_STRING_MISMATCH);
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_malformed_hash() {
        // url strings are not currently validated
        let url_str = ascii::string(x"414243454647");
        // length too short
        let hash = x"badf012345";

        let url = Url::new_unsafe(url_str);
        let _ = Url::new_unsafe_url_commitment(url, hash);
    }

    #[test]
    fun test_good_hash() {
        // url strings are not currently validated
        let url_str = ascii::string(x"414243454647");
        // 32 bytes
        let hash = x"1234567890123456789012345678901234567890abcdefabcdefabcdefabcdef";

        let url = Url::new_unsafe(url_str);
        let url_commit = Url::new_unsafe_url_commitment(url, hash);

        assert!(Url::url_commitment_resource_hash(&url_commit) == hash, EHASH_LENGTH_MISMATCH);
        assert!(Url::url_commitment_inner_url(&url_commit) == url_str, URL_STRING_MISMATCH);

        let url_str = ascii::string(x"37414243454647");

        Url::url_commitment_update(&mut url_commit, url_str);
        assert!(Url::url_commitment_inner_url(&url_commit) == url_str, URL_STRING_MISMATCH);
    }
}
