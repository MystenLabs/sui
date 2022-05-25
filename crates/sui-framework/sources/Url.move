// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// URL: standard Uniform Resource Locator string
/// Url: Sui type which wraps a URL
/// UrlCommitment: Sui type which wraps a Url but also includes an immutable commitment
/// to the hash of the resource at the given URL
module Sui::Url {
    use Std::ASCII::{Self, String};
    use Std::Vector;

    /// Length of the vector<u8> representing a resource hash
    const HASH_VECTOR_LENGTH: u64 = 32;

    /// Error code when the length of the hash vector is not HASH_VECTOR_LENGTH
    const EHashLengthMismatch: u64 = 0;

    /// Represents an arbitrary URL. Clients rendering values of this type should fetch the resource at `url` and render it using a to-be-defined Sui standard.
    struct Url has store, drop {
        // TODO: validate URL format
        url: String,
    }

    /// Represents an arbitrary URL plus an immutable commitment to the underlying
    /// resource hash. Clients rendering values of this type should fetch the resource at `url`, and then compare it against `resource_hash` using a to-be-defined Sui standard, and (if the two match) render the value using the `Url` standard.
    struct UrlCommitment has store, drop {
        url: Url,
        resource_hash: vector<u8>,
    }


    // === constructors ===

    /// Create a `Url`, with no validation
    public fun new_unsafe(url: String): Url {
        Url { url }
    }

    /// Create a `Url` with no validation from bytes
    /// Note: this will abort if `bytes` is not valid ASCII
    public fun new_unsafe_from_bytes(bytes: vector<u8>): Url {
        let url = ASCII::string(bytes);
        Url { url }
    }

    /// Create a `UrlCommitment`, and set the immutable hash
    public fun new_unsafe_url_commitment(url: Url, resource_hash: vector<u8>): UrlCommitment {
        // Length must be exact
        assert!(Vector::length(&resource_hash) == HASH_VECTOR_LENGTH, EHashLengthMismatch);

        UrlCommitment { url, resource_hash }
    }


    // === `Url` functions ===

    /// Get inner URL
    public fun inner_url(self: &Url): String{
        self.url
    }

    /// Update the inner URL
    public fun update(self: &mut Url, url: String) {
        self.url = url;
    }


    // === `UrlCommitment` functions ===

    /// Get the hash of the resource at the URL
    /// We enforce that the hash is immutable
    public fun url_commitment_resource_hash(self: &UrlCommitment): vector<u8> {
        self.resource_hash
    }

    /// Get inner URL
    public fun url_commitment_inner_url(self: &UrlCommitment): String{
        self.url.url
    }

    /// Update the URL, but the hash of the object at the URL must never change
    public fun url_commitment_update(self: &mut UrlCommitment, url: String) {
        update(&mut self.url, url)
    }
}
