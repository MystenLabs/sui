module FastX::Authenticator {
    use Std::Signer;

    friend FastX::ID;

    /// Authenticator for an end-user (e.g., a public key)
    // TODO: ideally, we would use the Move `address`` type here,
    // but Move forces `address`'s to be 16 bytes
    struct Authenticator has copy, drop, store {
        bytes: address
    }

    /// Unforgeable token representing the authority of a particular
    /// `Authenticator`. Can only be created by the VM
    // TODO: ideally we would use the Move `signer` type here; see comment
    // on `Authenticator` about size
    struct Signer has drop {
        inner: Authenticator,
    }

    /// Create an authenticator from a Move `signer`
    // TODO: probably want to kill this; see comments above
    public fun new_signer(signer: signer): Signer {
        Signer { inner: Authenticator { bytes: Signer::address_of(&signer) } }
    }

    // TODO: validation of bytes once we settle on authenticator format
    public fun new(bytes: vector<u8>): Authenticator {
        Authenticator { bytes: bytes_to_address(bytes) }
    }

    public fun new_from_address(a: address): Authenticator {
        Authenticator { bytes: a }
    }

    /// Derive an `Authenticator` from a `Signer`
    public fun get(self: &Signer): &Authenticator {
        &self.inner
    }

    /// Get the raw bytes associated with `self`
    public fun bytes(self: &Authenticator): &address {
        &self.bytes
    }

    /// Get the raw bytes associated with `self`
    public fun into_bytes(self: Authenticator): address {
        let Authenticator { bytes } = self;
        bytes
    }

    /// Return true if `a` is the underlying authenticator of `s`
    public fun is_signer(a: &Authenticator, s: &Signer): bool {
        get(s) == a
    }

    /// Manufacture an address from these bytes
    public(friend) native fun bytes_to_address(bytes: vector<u8>): address;
}
