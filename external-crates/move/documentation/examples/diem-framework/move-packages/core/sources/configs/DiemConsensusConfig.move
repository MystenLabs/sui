/// Maintains the consensus config for the Diem blockchain. The config is stored in a
/// DiemConfig, and may be updated by Diem root.
module CoreFramework::DiemConsensusConfig {
    use std::capability::Cap;
    use std::errors;
    use std::vector;
    use CoreFramework::DiemConfig;
    use CoreFramework::DiemTimestamp;
    use CoreFramework::SystemAddresses;

    /// Error with chain marker
    const ECHAIN_MARKER: u64 = 0;
    /// Error with config
    const ECONFIG: u64 = 1;

    /// Marker to be stored under @CoreResources during genesis
    struct ConsensusConfigChainMarker<phantom T> has key {}

    struct DiemConsensusConfig has key {
        config: vector<u8>,
    }

    /// Publishes the DiemConsensusConfig config.
    public fun initialize<T>(account: &signer) {
        DiemTimestamp::assert_genesis();
        SystemAddresses::assert_core_resource(account);
        assert!(
            !exists<ConsensusConfigChainMarker<T>>(@CoreResources),
            errors::already_published(ECHAIN_MARKER)
        );

        assert!(
            !exists<DiemConsensusConfig>(@CoreResources),
            errors::already_published(ECONFIG)
        );
        move_to(account, ConsensusConfigChainMarker<T>{});
        move_to(account, DiemConsensusConfig { config: vector::empty() });
    }

    /// Update the config.
    public fun set<T>(config: vector<u8>, _cap: &Cap<T>) acquires DiemConsensusConfig {
        assert!(exists<ConsensusConfigChainMarker<T>>(@CoreResources), errors::not_published(ECHAIN_MARKER));
        let config_ref = &mut borrow_global_mut<DiemConsensusConfig>(@CoreResources).config;
        *config_ref = config;
        DiemConfig::reconfigure();
    }
}
