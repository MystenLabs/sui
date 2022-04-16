// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Metadata standard for fungible and non-fungible tokens on Sui
module Sui::TokenMetadata {
    use Sui::Url::{Self, Url};
    use Sui::UTF8;
    use Std::Vector;

    struct TokenMetadata has store {
        /// Name for the token
        name: UTF8::String,
        /// Description of the token
        description: UTF8::String,
        /// A list of URLs for the token. If there are multiple URLs,
        /// it's recommended that clients(e.g, explorers) will select
        /// the first one in the list for display.
        urls: vector<Url>,
    }

    /// Construct a new TokenMetadata from the given inputs. Does not perform any validation
    /// on `url` or `name` or `description`.
    // TODO: support multiple urls
    public fun new(name: vector<u8>, description: vector<u8>, url: vector<u8>): TokenMetadata {
        TokenMetadata {
            name: UTF8::string_unsafe(name),
            description: UTF8::string_unsafe(description),
            urls: Vector::singleton<Url>(Url::new_from_bytes_unsafe(url)),
        }
    }

    public fun ulrs(self: &TokenMetadata): &vector<Url> {
        &self.urls
    }

    public fun name(self: &TokenMetadata): &UTF8::String {
        &self.name
    }

    public fun description(self: &TokenMetadata): &UTF8::String {
        &self.description
    }
}
