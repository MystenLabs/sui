// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Sui types for specifying off-chain/external resources.
///
/// The keywords "MUST", "MUST NOT", "SHOULD", "SHOULD NOT" and "MAY" below should be interpreted as described in
/// RFC 2119.
///
/// TODO(ben): Should we s/resource/object? or will it just confuse people with Sui objects?
///
module sui::external_resource {
    use std::ascii::{Self, String};
    use std::vector;

    /// Length of the vector<u8> representing a SHA256 digest.
    const HASH_VECTOR_LENGTH: u64 = 32;

    /// Error code when the length of the digest vector is not HASH_VECTOR_LENGTH.
    const EHashLengthMismatch: u64 = 0;

    /// URL: standard Uniform Resource Locator string.
    ///
    /// MUST follow RFC-3986.
    /// Clients MUST support (at least) the following schemes: ipfs, https.
    struct Url has store, copy, drop {
        url: String,
    }

    /// ImmutableExternalResource: An arbitrary, mutable URL plus an immutable digest of the resource.
    ///
    /// Represents a resource that can move but must never change. Example use cases:
    /// - NFT images.
    /// - NFT metadata.
    ///
    /// `digest` MUST be set to SHA256(content of resource at `url`).
    /// Clients of this type MUST fetch the resource at `url`, compute its digest and compare it against `digest`. If
    /// the result is false, clients SHOULD indicate that to users or ignore the resource.
    struct ImmutableExternalResource has store, copy, drop {
        url: Url,
        digest: vector<u8>,
    }


    // === constructors ===

    /// Create a `Url`.
    public fun new(url: String): Url {
        Url { url }
    }

    /// Create a `Url` from bytes.
    /// Aborts if `bytes` is not valid ASCII.
    public fun new_from_bytes(bytes: vector<u8>): Url {
        let url = ascii::string(bytes);
        Url { url }
    }

    /// Create a `ImmutableExternalResource`, and set the immutable hash.
    public fun new_immutable_external_resource(url: Url, digest: vector<u8>): ImmutableExternalResource {
        assert!(vector::length(&digest) == HASH_VECTOR_LENGTH, EHashLengthMismatch);
        ImmutableExternalResource { url, digest }
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


    // === `ImmutableExternalResource` functions ===

    /// Get the hash of the resource.
    public fun immutable_external_resource_digest(self: &ImmutableExternalResource): vector<u8> {
        self.digest
    }

    /// Get the URL of the resource.
    public fun immutable_external_resource_url(self: &ImmutableExternalResource): String{
        self.url.url
    }

    /// Update the URL, but the digest of the resource must never change.
    public fun immutable_external_resource_update(self: &mut ImmutableExternalResource, url: String) {
        update(&mut self.url, url)
    }
}
