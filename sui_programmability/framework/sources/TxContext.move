module FastX::TxContext {
    use FastX::ID::{Self, ID};
    use FastX::Address::{Self, Address, Signer};

    /// Information about the transaction currently being executed.
    /// This is a privileged object created by the VM and passed into `main`
    struct TxContext has drop {
        /// The signer of the current transaction
        // TODO: use vector<Signer> if we want to support multi-agent
        signer: Signer,
        /// Hash of all the input objects to this transaction
        inputs_hash: vector<u8>,
        /// Counter recording the number of fresh id's created while executing
        /// this transaction
        ids_created: u64
    }

    /// Return the signer of the current transaction
    public fun get_signer(self: &TxContext): &Signer {
        &self.signer
    }

    /// Return the address of the user that signed the current
    /// transaction
    public fun get_signer_address(self: &TxContext): Address {
        *Address::get(&self.signer)
    }

    /// Return the number of id's created by the current transaction
    public fun get_ids_created(self: &TxContext): u64 {
        self.ids_created
    }

    /// Generate a new object ID
    public fun new_id(ctx: &mut TxContext): ID {
        let ids_created = ctx.ids_created;
        let id = ID::new(fresh_id(*&ctx.inputs_hash, ids_created));
        ctx.ids_created = ids_created + 1;
        id
    }

    native fun fresh_id(inputs_hash: vector<u8>, ids_created: u64): address;

    // ==== test-only functions ====

    #[test_only]
    /// Create a `TxContext` for testing
    public fun new(signer: Signer, inputs_hash: vector<u8>, ids_created: u64): TxContext {
        TxContext { signer, inputs_hash, ids_created }
    }

    #[test_only]
    /// Create a dummy `TxContext` for testing
    public fun dummy(): TxContext {
        let inputs_hash = x"3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532";
        new(Address::dummy_signer(), inputs_hash, 0)
    }
}
