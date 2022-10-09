// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::external_resource_tests {
    use sui::external_resource;
    use sui::url;
    use std::ascii::Self;
    use sui::digest;

    const EHASH_LENGTH_MISMATCH: u64 = 0;
    const URL_STRING_MISMATCH: u64 = 1;

    #[test]
    fun test_init() {
        // url strings are not currently validated
        let url_str = ascii::string(x"414243454647");
        // 32 bytes
        let hash = x"1234567890123456789012345678901234567890abcdefabcdefabcdefabcdef";

        let url = url::new_unsafe(url_str);
        let digest = digest::new_sha256_digest(hash);
        let resource = external_resource::new_immutable_external_resource(url, digest);

        assert!(external_resource::immutable_external_resource_url(&resource) == url, 0);
        assert!(external_resource::immutable_external_resource_digest(&resource) == digest, 0);

        let new_url_str = ascii::string(x"37414243454647");
        let new_url = url::new_unsafe(new_url_str);

        external_resource::immutable_external_resource_update(&mut resource, new_url);
        assert!(external_resource::immutable_external_resource_url(&resource) == new_url, 0);
    }
}
