module FastX::Authenticator {
    /// Authenticator for an end-user (e.g., a public key)
    // TODO: ideally, we would use the Move `address`` type here,
    // but Move forces `address`'s to be 16 bytes
    struct Authenticator has copy, drop, store {
        bytes: vector<u8>
    }

    /// Unforgeable token representing the authority of a particular
    /// `Authenticator`. Can only be created by the VM
    struct Signer has drop {
        inner: Authenticator,
    }

    // TODO: validation of bytes once we settle on authenticator format
    public fun new(bytes: vector<u8>): Authenticator {
        Authenticator { bytes }
    }

    /// Derive an `Authenticator` from a `Signer`
    public fun get(self: &Signer): &Authenticator {
        &self.inner
    }

    /// Get the raw bytes associated with `self`
    public fun bytes(self: &Authenticator): &vector<u8> {
        &self.bytes
    }

    /// Get the raw bytes associated with `self`
    public fun signer_bytes(self: &Signer): &vector<u8> {
        &self.inner.bytes
    }


    /// Return true if `a` is the underlying authenticator of `s`
    public fun is_signer(a: &Authenticator, s: &Signer): bool {
        get(s) == a
    }
}
