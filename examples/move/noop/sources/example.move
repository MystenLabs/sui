// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module noop::example{
 
    // Sui imports
    use sui::object::{Self, UID};
    use sui::event::emit;
    use sui::tx_context;
    use sui::tx_context::TxContext;
    use sui::clock::{Self, Clock};
 
    /// Struct to be used to emit an event with metadata.
    struct MetadataEvent has copy, drop{
        creator: address,
        metadata: vector<u8>
    }

    struct Metadata has key, store {
        id: UID,
        metadata: vector<u8>,
        created_at: u64
    }
    
    /// Empty heartbeat call.
    public fun noop(){}
 
    /// Empty heartbeat call with metadata input.
    public fun noop_w_metadata(_metadata: vector<u8>){}
 
    /// Heartbeat with metadata that emits an event.
    public fun noop_w_metadata_event(metadata:vector<u8>, ctx: &mut TxContext){
        // Create and emit a MetadataEvent
        let metadata_event = MetadataEvent {
            creator: tx_context::sender(ctx),
            metadata
        };

        emit(metadata_event);
    }

    /// Function to store metadata in an NFT.
    /// Returns the newly created metadata object.
    public fun add_metadata(metadata: vector<u8>, clock: &Clock, ctx: &mut TxContext): Metadata {
        let created_at = clock::timestamp_ms(clock);

        let metadata = Metadata {
            id: object::new(ctx),
            metadata,
            created_at
        };

        metadata
    }

    /// Function to check the time since the last heartbeat
    public fun time_since_last_heartbeat(metadata: &Metadata, clock: &Clock): u64 {
        clock::timestamp_ms(clock) - metadata.created_at
    }
}
