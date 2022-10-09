// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Sui types for specifying off-chain/external resources.
///
/// The keywords "MUST", "MUST NOT", "SHOULD", "SHOULD NOT" and "MAY" below should be interpreted as described in
/// RFC 2119.
///
/// TODO(ben): Should we s/resource/object? or will it just confuse people with Sui objects?
///
module sui::external_resource {
    use sui::digest::Sha256Digest;
    use sui::url::{Url, update, inner_url};

    /// ImmutableExternalResource: An arbitrary, mutable URL plus an immutable digest of the resource.
    ///
    /// Represents a resource that can move but must never change. Example use cases:
    /// - NFT images.
    /// - NFT metadata.
    ///
    /// `url` MUST follow RFC-3986. Clients MUST support (at least) the following schemes: ipfs, https.
    /// `digest` MUST be set to SHA256(content of resource at `url`).
    ///
    /// Clients of this type MUST fetch the resource at `url`, compute its digest and compare it against `digest`. If
    /// the result is false, clients SHOULD indicate that to users or ignore the resource.
    struct ImmutableExternalResource has store, copy, drop {
        url: Url,
        digest: Sha256Digest,
    }

    /// Create a `ImmutableExternalResource`, and set the immutable hash.
    public fun new_immutable_external_resource(url: Url, digest: Sha256Digest): ImmutableExternalResource {
        ImmutableExternalResource { url, digest }
    }

    /// Get the hash of the resource.
    public fun immutable_external_resource_digest(self: &ImmutableExternalResource): Sha256Digest {
        self.digest
    }

    /// Get the URL of the resource.
    public fun immutable_external_resource_url(self: &ImmutableExternalResource): Url {
        self.url
    }

    /// Update the URL, but the digest of the resource must never change.
    public fun immutable_external_resource_update(self: &mut ImmutableExternalResource, url: Url) {
        update(&mut self.url, inner_url(&url))
    }
}
