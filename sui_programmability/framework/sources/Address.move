module FastX::Address {
    use Std::Errors;
    use Std::Vector;

    /// Number of bytes in an address
    const ADDRESS_LENGTH: u64 = 32;

    /// Expected an authenticator of length 32, but found a different length
    const EBAD_ADDRESS_LENGTH: u64 = 0;

    /// Address for an end-user (e.g., a public key)
    // TODO: ideally, we would use the Move `address`` type here,
    // but Move forces `address`'s to be 16 bytes
    struct Address has copy, drop, store {
        bytes: vector<u8>,
    }

    /// Unforgeable token representing the authority of a particular
    /// `Address`. Can only be created by the VM
    // TODO: ideally we would use the Move `signer` type here; see comment
    // on `Address` about size
    struct Signer has drop {
        inner: Address,
    }

    /// Create an authenticator from `bytes`.
    /// Aborts if the length `bytes` is not `AUTHENTICATOR_LENGTH`
    public fun new(bytes: vector<u8>): Address {
        assert!(
            Vector::length(&bytes) == ADDRESS_LENGTH,
            Errors::invalid_argument(EBAD_ADDRESS_LENGTH)
        );
        Address { bytes }
    }

    /// Derive an `Address` from a `Signer`
    public fun get(self: &Signer): &Address {
        &self.inner
    }

    /// Get the raw bytes associated with `self`
    public fun bytes(self: &Address): &vector<u8> {
        &self.bytes
    }

    /// Get the raw bytes associated with `self`
    public fun into_bytes(self: Address): vector<u8> {
        let Address { bytes } = self;
        bytes
    }

    /// Return true if `a` is the underlying authenticator of `s`
    public fun is_signer(a: &Address, s: &Signer): bool {
        get(s) == a
    }

    // ==== test-only functions ====
    

    #[test_only]
    /// Create a `Signer` from `bytes` for testing
    public fun new_signer(bytes: vector<u8>): Signer {
        assert!(
            Vector::length(&bytes) == ADDRESS_LENGTH,
            Errors::invalid_argument(EBAD_ADDRESS_LENGTH)
        );
        Signer { inner: new(bytes) }
    }

    #[test_only]
    /// Create a `Signer` from `a` for testing
    public fun new_signer_from_address(a: Address): Signer {
        Signer { inner: a }
    }

    #[test_only]
    /// Create a dummy `Signer` for testing
    public fun dummy_signer(): Signer {
        new_signer(x"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    }

    #[test_only]
    /// All bytes will be 0 except the last byte, which will be `hint`.
    fun bytes_with_hint(hint: u8): vector<u8> {
        let bytes = Vector::empty<u8>();
        let i = 0;
        while (i < ADDRESS_LENGTH - 1) {
            Vector::push_back(&mut bytes, 0u8);
            i = i + 1;
        };
        Vector::push_back(&mut bytes, hint);
        bytes
    }

    #[test_only]
    /// Create a dummy `Signer` for testing
    /// All bytes will be 0 except the last byte, which will be `hint`.
    public fun dummy_signer_with_hint(hint: u8): Signer {
        new_signer(bytes_with_hint(hint))
    }

    #[test_only]
    /// Create a dummy `Address` for testing
    /// All bytes will be 0 except the last byte, which will be `hint`.
    public fun dummy_with_hint(hint: u8): Address {
        new(bytes_with_hint(hint))
    }
}
