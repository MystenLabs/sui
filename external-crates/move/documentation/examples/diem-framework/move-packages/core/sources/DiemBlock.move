/// This module defines a struct storing the metadata of the block and new block events.
module CoreFramework::DiemBlock {
    use std::errors;
    use std::event;
    use CoreFramework::DiemSystem;
    use CoreFramework::DiemTimestamp;
    use CoreFramework::SystemAddresses;

    struct BlockMetadata has key {
        /// Height of the current block
        height: u64,
        /// Handle where events with the time of new blocks are emitted
        new_block_events: event::EventHandle<Self::NewBlockEvent>,
    }

    struct NewBlockEvent has drop, store {
        round: u64,
        proposer: address,
        previous_block_votes: vector<address>,

        /// On-chain time during  he block at the given height
        time_microseconds: u64,
    }

    /// The `BlockMetadata` resource is in an invalid state
    const EBLOCK_METADATA: u64 = 0;
    /// An invalid signer was provided. Expected the signer to be the VM or a Validator.
    const EVM_OR_VALIDATOR: u64 = 1;

    /// This can only be invoked by the Association address, and only a single time.
    /// Currently, it is invoked in the genesis transaction
    public fun initialize_block_metadata(account: &signer) {
        DiemTimestamp::assert_genesis();
        // Operational constraint, only callable by the Association address
        SystemAddresses::assert_core_resource(account);

        assert!(!is_initialized(), errors::already_published(EBLOCK_METADATA));
        move_to<BlockMetadata>(
            account,
            BlockMetadata {
                height: 0,
                new_block_events: event::new_event_handle<Self::NewBlockEvent>(account),
            }
        );
    }

    /// Helper function to determine whether this module has been initialized.
    fun is_initialized(): bool {
        exists<BlockMetadata>(@CoreResources)
    }

    /// Set the metadata for the current block.
    /// The runtime always runs this before executing the transactions in a block.
    fun block_prologue(
        vm: signer,
        round: u64,
        timestamp: u64,
        previous_block_votes: vector<address>,
        proposer: address
    ) acquires BlockMetadata {
        DiemTimestamp::assert_operating();
        // Operational constraint: can only be invoked by the VM.
        SystemAddresses::assert_vm(&vm);

        // Authorization
        assert!(
            proposer == @VMReserved || DiemSystem::is_validator(proposer),
            errors::requires_address(EVM_OR_VALIDATOR)
        );

        let block_metadata_ref = borrow_global_mut<BlockMetadata>(@CoreResources);
        DiemTimestamp::update_global_time(&vm, proposer, timestamp);
        block_metadata_ref.height = block_metadata_ref.height + 1;
        event::emit_event<NewBlockEvent>(
            &mut block_metadata_ref.new_block_events,
            NewBlockEvent {
                round,
                proposer,
                previous_block_votes,
                time_microseconds: timestamp,
            }
        );
    }

    /// Get the current block height
    public fun get_current_block_height(): u64 acquires BlockMetadata {
        assert!(is_initialized(), errors::not_published(EBLOCK_METADATA));
        borrow_global<BlockMetadata>(@CoreResources).height
    }
}
