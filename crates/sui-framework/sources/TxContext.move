// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::TxContext {
    use Std::Signer;
    use Sui::ID::{Self, VersionedID};

    #[test_only]
    use Std::Errors;
    #[test_only]
    use Std::Vector;
    #[test_only]
    use Sui::ID::ID;

    /// Number of bytes in an tx hash (which will be the transaction digest)
    const TX_HASH_LENGTH: u64 = 32;

    /// Expected an tx hash of length 32, but found a different length
    const EBadTxHashLength: u64 = 0;

    #[test_only]
    /// Attempt to get the most recent created object ID when none has been created.
    const ENoIDsCreated: u64 = 1;

    /// Information about the transaction currently being executed.
    /// This cannot be constructed by a transaction--it is a privileged object created by
    /// the VM and passed in to the entrypoint of the transaction as `&mut TxContext`.
    struct TxContext has drop {
        /// A `signer` wrapping the address of the user that signed the current transaction
        signer: signer,
        /// Hash of the current transaction
        tx_hash: vector<u8>,
        /// The current epoch number.
        epoch: u64,
        /// Counter recording the number of fresh id's created while executing
        /// this transaction. Always 0 at the start of a transaction
        ids_created: u64
    }

    /// Return the address of the user that signed the current
    /// transaction
    public fun sender(self: &TxContext): address {
        Signer::address_of(&self.signer)
    }

    /// Return a `signer` for the user that signed the current transaction
    public fun signer_(self: &TxContext): &signer {
        &self.signer
    }

    public fun epoch(self: &TxContext): u64 {
        self.epoch
    }

    /// Generate a new, globally unique object ID with version 0
    public fun new_id(ctx: &mut TxContext): VersionedID {
        let ids_created = ctx.ids_created;
        let id = ID::new_versioned_id(derive_id(*&ctx.tx_hash, ids_created));
        ctx.ids_created = ids_created + 1;
        id
    }

    /// Return the number of id's created by the current transaction.
    /// Hidden for now, but may expose later
    fun ids_created(self: &TxContext): u64 {
        self.ids_created
    }

    /// Native function for deriving an ID via hash(tx_hash || ids_created)
    native fun derive_id(tx_hash: vector<u8>, ids_created: u64): address;

    // ==== test-only functions ====

    #[test_only]
    /// Create a `TxContext` for testing
    public fun new(signer: signer, tx_hash: vector<u8>, ids_created: u64): TxContext {
        assert!(
            Vector::length(&tx_hash) == TX_HASH_LENGTH,
            Errors::invalid_argument(EBadTxHashLength)
        );
        TxContext { signer, tx_hash, epoch: 0, ids_created }
    }

    #[test_only]
    /// Create a `TxContext` for testing, with a potentially non-zero epoch number.
    public fun new_with_epoch(signer: signer, tx_hash: vector<u8>, epoch: u64, ids_created: u64): TxContext {
        assert!(
            Vector::length(&tx_hash) == TX_HASH_LENGTH,
            Errors::invalid_argument(EBadTxHashLength)
        );
        TxContext { signer, tx_hash, epoch, ids_created }
    }

    #[test_only]
    /// Create a `TxContext` with sender `a` for testing, and a tx hash derived from `hint`
    public fun new_from_address(a: address, hint: u8): TxContext {
        new(new_signer_from_address(a), dummy_tx_hash_with_hint(hint), 0)
    }

    #[test_only]
    /// Create a dummy `TxContext` for testing
    public fun dummy(): TxContext {
        let tx_hash = x"3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532";
        new(new_signer_from_address(@0x0), tx_hash, 0)
    }

    #[test_only]
    /// Utility for creating 256 unique input hashes
    fun dummy_tx_hash_with_hint(hint: u8): vector<u8> {
        let tx_hash = Vector::empty<u8>();
        let i = 0;
        while (i < TX_HASH_LENGTH - 1) {
            Vector::push_back(&mut tx_hash, 0u8);
            i = i + 1;
        };
        Vector::push_back(&mut tx_hash, hint);
        tx_hash
    }

    #[test_only]
    public fun get_ids_created(self: &TxContext): u64 {
        ids_created(self)
    }

    #[test_only]
    /// Return the most recent created object ID.
    public fun last_created_object_id(self: &TxContext): ID {
        let ids_created = self.ids_created;
        assert!(ids_created > 0, ENoIDsCreated);
        ID::new(derive_id(*&self.tx_hash, ids_created - 1))
    }

    #[test_only]
    /// Test-only function for creating a new signer from `signer_address`.
    native fun new_signer_from_address(signer_address: address): signer;
}
