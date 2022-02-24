module FastX::TxContext {
    #[test_only]
    use Std::Errors;
    #[test_only]
    use Std::Vector;

    use Std::Signer;

    use FastX::ID::{Self, VersionedID};

    /// Number of bytes in an inputs_hash (which will be the transaction digest)
    const INPUTS_HASH_LENGTH: u64 = 32;

    /// Expected an inputs_hash of length 32, but found a different length
    const EBAD_INPUTS_HASH_LENGTH: u64 = 0;

    /// Information about the transaction currently being executed.
    /// This is a privileged object created by the VM and passed into `main`
    struct TxContext has drop {
        /// The signer of the current transaction
        // TODO: use vector<signer> if we want to support multi-agent
        signer: signer,
        /// Hash of all the input objects to this transaction
        inputs_hash: vector<u8>,
        /// Counter recording the number of fresh id's created while executing
        /// this transaction
        ids_created: u64
    }

    /// Return the signer of the current transaction
    public fun get_signer(self: &TxContext): &signer {
        &self.signer
    }

    /// Return the address of the user that signed the current
    /// transaction
    public fun get_signer_address(self: &TxContext): address {
        Signer::address_of(&self.signer)
    }

    /// Return the number of id's created by the current transaction
    public fun get_ids_created(self: &TxContext): u64 {
        self.ids_created
    }

    /// Generate a new object ID
    public fun new_id(ctx: &mut TxContext): VersionedID {
        let ids_created = ctx.ids_created;
        let id = ID::new(fresh_id(*&ctx.inputs_hash, ids_created));
        ctx.ids_created = ids_created + 1;
        id
    }

    native fun fresh_id(inputs_hash: vector<u8>, ids_created: u64): address;

    // ==== test-only functions ====

    #[test_only]
    /// Create a `TxContext` for testing
    public fun new(signer: signer, inputs_hash: vector<u8>, ids_created: u64): TxContext {
        assert!(
            Vector::length(&inputs_hash) == INPUTS_HASH_LENGTH,
            Errors::invalid_argument(EBAD_INPUTS_HASH_LENGTH)
        );
        TxContext { signer, inputs_hash, ids_created }
    }

    #[test_only]
    /// Create a `TxContext` with sender `a` for testing, and an inputs hash derived from `hint`
    public fun new_from_address(a: address, hint: u8): TxContext {
        new(new_signer_from_address(a), dummy_inputs_hash_with_hint(hint), 0)
    }

    #[test_only]
    /// Create a dummy `TxContext` for testing
    public fun dummy(): TxContext {
        let inputs_hash = x"3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532";
        new(new_signer_from_address(@0x0), inputs_hash, 0)
    }

    #[test_only]
    /// Utility for creating 256 unique input hashes
    fun dummy_inputs_hash_with_hint(hint: u8): vector<u8> {
        let inputs_hash = Vector::empty<u8>();
        let i = 0;
        while (i < INPUTS_HASH_LENGTH - 1) {
            Vector::push_back(&mut inputs_hash, 0u8);
            i = i + 1;
        };
        Vector::push_back(&mut inputs_hash, hint);
        inputs_hash
    }

    /// Test-only function for creating a new signer from `signer_address`.
    native fun new_signer_from_address(signer_address: address): signer;
}
