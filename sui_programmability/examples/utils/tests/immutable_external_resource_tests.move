// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module utils::immutable_external_resource_tests {
    use utils::immutable_external_resource;
    use sui::url;
    use std::ascii::Self;
    use std::hash::sha3_256;

    const EHashStringMisMatch: u64 = 0;
    const EUrlStringMisMatch: u64 = 1;

    #[test]
    fun test_init() {
        // url strings are not currently validated
        let url_str = ascii::string(x"414243454647");
        // 32 bytes
        let hash = x"1234567890123456789012345678901234567890abcdefabcdefabcdefabcdef";

        let url = url::new_unsafe(url_str);
        let digest = sha3_256(hash);
        let mut resource = immutable_external_resource::new(url, digest);

        assert!(immutable_external_resource::url(&resource) == url, EUrlStringMisMatch);
        assert!(immutable_external_resource::digest(&resource) == digest, EHashStringMisMatch);

        let new_url_str = ascii::string(x"37414243454647");
        let new_url = url::new_unsafe(new_url_str);

        immutable_external_resource::update(&mut resource, new_url);
        assert!(immutable_external_resource::url(&resource) == new_url, EUrlStringMisMatch);
    }
}
