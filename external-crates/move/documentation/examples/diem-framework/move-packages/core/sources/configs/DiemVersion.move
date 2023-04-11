/// Maintains the version number for the blockchain.
module CoreFramework::DiemVersion {
    use std::capability::Cap;
    use std::errors;
    use CoreFramework::DiemConfig;
    use CoreFramework::DiemTimestamp;
    use CoreFramework::SystemAddresses;

    /// Marker to be stored under 0x1 during genesis
    struct VersionChainMarker<phantom T> has key {}

    struct DiemVersion has key, copy, drop, store {
        major: u64,
    }

    /// Error with chain marker
    const ECHAIN_MARKER: u64 = 0;
    /// Error with config
    const ECONFIG: u64 = 1;
    /// Tried to set an invalid major version for the VM. Major versions must be strictly increasing
    const EINVALID_MAJOR_VERSION_NUMBER: u64 = 2;

    /// Publishes the Version config.
    public fun initialize<T>(account: &signer, initial_version: u64) {
        DiemTimestamp::assert_genesis();

        SystemAddresses::assert_core_resource(account);

        assert!(
            !exists<VersionChainMarker<T>>(@CoreResources),
            errors::already_published(ECHAIN_MARKER)
        );

        assert!(
            !exists<DiemVersion>(@CoreResources),
            errors::already_published(ECONFIG)
        );

        move_to(
            account,
            VersionChainMarker<T> {},
        );
        move_to(
            account,
            DiemVersion { major: initial_version },
        );
    }

    /// Updates the major version to a larger version.
    public fun set<T>(major: u64, _cap: &Cap<T>) acquires DiemVersion {
        assert!(exists<VersionChainMarker<T>>(@CoreResources), errors::not_published(ECHAIN_MARKER));
        assert!(exists<DiemVersion>(@CoreResources), errors::not_published(ECONFIG));
        let old_major = *&borrow_global<DiemVersion>(@CoreResources).major;

        assert!(
            old_major < major,
            errors::invalid_argument(EINVALID_MAJOR_VERSION_NUMBER)
        );

        let config = borrow_global_mut<DiemVersion>(@CoreResources);
        config.major = major;

        DiemConfig::reconfigure();
    }
}
