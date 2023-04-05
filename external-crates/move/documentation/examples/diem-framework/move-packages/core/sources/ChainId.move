/// The chain id distinguishes between different chains (e.g., testnet and the main Diem network).
/// One important role is to prevent transactions intended for one chain from being executed on another.
/// This code provides a container for storing a chain id and functions to initialize and get it.
module CoreFramework::ChainId {
    use CoreFramework::SystemAddresses;
    use CoreFramework::DiemTimestamp;
    use std::errors;
    use std::signer;

    struct ChainId has key {
        id: u8
    }

    /// The `ChainId` resource was not in the required state
    const ECHAIN_ID: u64 = 0;

    /// Publish the chain ID `id` of this Diem instance under the SystemAddresses address
    public fun initialize(account: &signer, id: u8) {
        DiemTimestamp::assert_genesis();
        SystemAddresses::assert_core_resource(account);
        assert!(!exists<ChainId>(signer::address_of(account)), errors::already_published(ECHAIN_ID));
        move_to(account, ChainId { id })
    }

    /// Return the chain ID of this Diem instance
    public fun get(): u8 acquires ChainId {
        DiemTimestamp::assert_operating();
        borrow_global<ChainId>(@CoreResources).id
    }
}
