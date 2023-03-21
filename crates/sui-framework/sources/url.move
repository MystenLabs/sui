// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// URL: standard Uniform Resource Locator string
module sui::url {
    use std::ascii::{Self, String};
    use std::option::Option;

    use sui::vec_map::VecMap;

    /// Standard Uniform Resource Locator (URL) string.
    struct Url has store, copy, drop {
        url: String
    }

    /// Parsed URL. URL split into it's component parts
    struct ParsedUrl has store, copy, drop {
        /// The scheme of the URL (e.g https, http)
        scheme: String,
        /// The hostname of the URL, empty if URL is a data url
        host: Option<String>,
        /// The path of the URL
        path: String,
        /// The port of the URL, empty if it's not available in the URL string
        port: Option<u64>,
        /// The URL query parameters
        params: VecMap<String, String>
    }

    /// Create a `Url`, with validation
    public fun new(url: String): Url {
        new_from_bytes(ascii::into_bytes(url))
    }

    /// Create a `Url` with validation from bytes
    public fun new_from_bytes(bytes: vector<u8>): Url {
       validate_url(bytes);
       Url { url: ascii::string(bytes) }
    }

    /// Create a `Url`, with no validation
    public fun new_unsafe(url: String): Url {
        Url { url }
    }

    /// Create a `Url` with no validation from bytes
    /// Note: this will abort if `bytes` is not valid ASCII
    public fun new_unsafe_from_bytes(bytes: vector<u8>): Url {
        let url = ascii::string(bytes);
        Url { url }
    }

    /// Get inner URL
    public fun inner_url(self: &Url): String {
        self.url
    }

    /// Update the inner URL
    public fun update(self: &mut Url, url: String) {
        validate_url(ascii::into_bytes(url));
        self.url = url;
    }

    /// Parse URL, split a URL into it's components
    public fun parse_url(self: &Url): ParsedUrl {
       parse_url_internal(ascii::into_bytes(self.url))
    }

    /// Returns the `scheme` of a parsed URL
    public fun parsed_scheme(parsed_url: &ParsedUrl): String {
        parsed_url.scheme
    }

    /// Returns the `host` of a parsed URL
    public fun parsed_host(parsed_url: &ParsedUrl): Option<String> {
        parsed_url.host
    }

    /// Returns the `path` of a parsed URL
    public fun parsed_path(parsed_url: &ParsedUrl): String {
        parsed_url.path
    }

    /// Returns the `port` of a parsed URL
    public fun parsed_port(parsed_url: &ParsedUrl): Option<u64> {
        parsed_url.port
    }

    /// Returns the `params` (query parameters) of a parsed URL
    public fun parsed_params(parsed_url: &ParsedUrl): VecMap<String, String> {
        parsed_url.params
    }

    /// Validates a URL, aborts if the URL invalid
    native fun validate_url(url: vector<u8>);

    /// Parses a URL into it's components
    native fun parse_url_internal(url: vector<u8>): ParsedUrl;
}
