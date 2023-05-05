// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Sui types for specifying off-chain/external resources.
///
/// The keywords "MUST", "MUST NOT", "SHOULD", "SHOULD NOT" and "MAY" below should be interpreted as described in
/// RFC 2119.
///
module utils::immutable_external_resource {
    use sui::url::{Url, inner_url};

    /// ImmutableExternalResource: An arbitrary, mutable URL plus an immutable digest of the resource.
    ///
    /// Represents a resource that can move but must never change. Example use cases:
    /// - NFT images.
    /// - NFT metadata.
    ///
    /// `url` MUST follow RFC-3986. Clients MUST support (at least) the following schemes: ipfs, https.
    /// `digest` MUST be set to SHA3-256(content of resource at `url`).
    ///
    /// Clients of this type MUST fetch the resource at `url`, compute its digest and compare it against `digest`. If
    /// the result is false, clients SHOULD indicate that to users or ignore the resource.
    struct ImmutableExternalResource has store, copy, drop {
        url: Url,
        digest: vector<u8>,
    }

    /// Create a `ImmutableExternalResource`, and set the immutable hash.
    public fun new(url: Url, digest: vector<u8>): ImmutableExternalResource {
        ImmutableExternalResource { url, digest }
    }

    /// Get the hash of the resource.
    public fun digest(self: &ImmutableExternalResource): vector<u8> {
        self.digest
    }

    /// Get the URL of the resource.
    public fun url(self: &ImmutableExternalResource): Url {
        self.url
    }

    /// Update the URL, but the digest of the resource must never change.
    public fun update(self: &mut ImmutableExternalResource, url: Url) {
        sui::url::update(&mut self.url, inner_url(&url))
    }
}
