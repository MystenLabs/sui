module FastX::TxContext {
    use FastX::ID::{Self, ID};
    use FastX::Authenticator::{Self, Authenticator, Signer};
    use Std::BCS;
    use Std::Hash;
    use Std::Vector;

    /// Information about the transaction currently being executed.
    /// This is a privileged object created by the VM and passed into `main`
    struct TxContext has drop {
        /// The signer of the current transaction
        // TODO: use vector<Signer> if we want to support multi-agent
        signer: Signer,
        /// Hash of all the input objects to this transaction
        inputs_hash: vector<u8>,
        /// Counter recording the number of objects created while executing
        /// this transaction
        objects_created: u64
    }

    // TODO: temporary hack; as comment above says, this should get passed in
    // by the VM
    public fun make_unsafe(
        signer: signer, inputs_hash: vector<u8>
    ): TxContext {
        TxContext {
            signer: Authenticator::new_signer(signer),
            inputs_hash,
            objects_created: 0,
        }
    }

    /// Generate a new primary key
    // TODO: can make this native for better perf
    public fun new_id(ctx: &mut TxContext): ID {
        let msg = *&ctx.inputs_hash;
        let next_object_num = ctx.objects_created;
        ctx.objects_created = next_object_num + 1;

        Vector::append(&mut msg, BCS::to_bytes(&next_object_num));
        ID::new(Hash::sha3_256(msg))
    }

    /// Return the signer of the current transaction
    public fun get_signer(self: &TxContext): &Signer {
        &self.signer
    }

    /// Return the authenticator of the user that signed the current
    /// transaction
    public fun get_authenticator(self: &TxContext): Authenticator {
        *Authenticator::get(&self.signer)
    }
}
