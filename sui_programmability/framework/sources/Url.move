/// An object which refers to a resource at a URL
module Sui::Url {
    use Sui::ID::{Self, VersionedID};
    use Sui::TxContext::{Self, TxContext};
    use Std::ASCII::String;
    use Std::Vector;

    /// Length of the vector<u8> representing a resource hash
    const HASH_VECTOR_LENGTH: u64 = 32;
    const HASH_LENGTH_MISMATCH: u64 = 0;

    struct Url has key, store {
        id: VersionedID,
        // TODO: validate URL format
        url: String,
        resource_hash: vector<u8>,
    }

    // === constructors ===

    /// Create a `Url`
    public fun new(url: String, resource_hash: vector<u8>, ctx: &mut TxContext): Url {
        // Length must be exact
        assert!(Vector::length(&resource_hash) == HASH_VECTOR_LENGTH, HASH_LENGTH_MISMATCH);

        Url { id: TxContext::new_id(ctx),
                url: url, resource_hash: resource_hash }
    }

    /// Get the hash of the resource at the URL
    /// We enforce that the hash is immutable
    public fun get_resource_hash(self: &Url): vector<u8> {
        self.resource_hash
    }

    /// Get URL
    public fun get_url(self: &Url): String{
        self.url
    }

    /// Update the URL, but the hash of the object at the URL must never change
    public fun update(self: &mut Url, url: String) {
        self.url = url;
    }

    /// Destroy the URL object
    public fun delete(self: Url) {
        let Url { id, url, resource_hash } = self;
        let _ = url;
        let _  = resource_hash;
        ID::delete(id);
    }
}
