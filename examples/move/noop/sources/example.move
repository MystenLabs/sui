module noop::example{
 
    // Sui imports
    use sui::object::{Self, UID};
    use sui::event::emit;
    use sui::tx_context;
    use sui::tx_context::TxContext;
 
    /// Struct to be used to emit an event with metadata.
    struct MetadataEvent has copy, drop{
        creator: address,
        metadata: vector<u8>
    }

    struct Metadata has key, store {
        id: UID,
        metadata: vector<u8>
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
    /// Returns the newlly created metadata object.
    public fun add_metadata(metadata: vector<u8>, ctx: &mut TxContext): Metadata {
        let metadata = Metadata {
            id: object::new(ctx),
            metadata
        };

        metadata
    }
}