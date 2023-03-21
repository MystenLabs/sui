// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::url_tests {
    use sui::url;
    use std::ascii::Self;
    use std::option::Self;
    // use sui::vec_map::Self;

    const EUrlStringMismatch: u64 = 1;

    #[test]
    fun test_basic_url() {
        // url strings are not currently validated
        let url_str = ascii::string(x"414243454647");

        let url = url::new_unsafe(url_str);
        assert!(url::inner_url(&url) == url_str, EUrlStringMismatch);
    }

    #[test]
    fun test_valid_url() {
        let url_str = b"https://validurl.com/move?type=valid";
        let url = url::new_from_bytes(url_str);

        assert!(url::inner_url(&url) == ascii::string(url_str), EUrlStringMismatch);
    }

    #[test]
    fun test_data_url() {
        let url_str = b"data:text/plain;base64,SGVsbG8sIFdvcmxkIQ==";
        let url = url::new_from_bytes(url_str);

        assert!(url::inner_url(&url) == ascii::string(url_str), EUrlStringMismatch);
    }

    #[test]
    fun test_parse_url() {
        let url = url::new_from_bytes(b"https://validurl.com:9000/move?type=valid&isConfirmed=true");
        let parsed_url = url::parse_url(&url);

        assert!(url::parsed_scheme(&parsed_url) == ascii::string(b"https"), 0);
        assert!(url::parsed_host(&parsed_url) == option::some(ascii::string(b"validurl.com")), 0);
        assert!(url::parsed_path(&parsed_url) == ascii::string(b"/move"), 0);
        assert!(url::parsed_port(&parsed_url) == option::some(9000), 0);
    }

    #[test]
    #[expected_failure(abort_code = 0, location = sui::url)]
    fun test_invalid_url_failure() {
        url::new_from_bytes(b"someinvalidurl");
        url::new_from_bytes(x"1274849494");
    }
}