/// This module defines structs and methods to initialize VM configurations,
/// including different costs of running the VM.
module CoreFramework::ParallelExecutionConfig {
    use std::capability::Cap;
    use std::errors;
    use std::option::{Self, Option};
    use CoreFramework::DiemConfig;
    use CoreFramework::DiemTimestamp;
    use CoreFramework::SystemAddresses;

    /// Error with chain marker
    const ECHAIN_MARKER: u64 = 0;
    /// Error with config
    const ECONFIG: u64 = 1;

    /// Marker to be stored under @CoreResources during genesis
    struct ParallelExecutionConfigChainMarker<phantom T> has key {}

    /// The struct to hold the read/write set analysis result for the whole Diem Framework.
    struct ParallelExecutionConfig has key {
        /// Serialized analysis result for the Diem Framework.
        /// If this payload is not None, DiemVM will use this config to execute transactions in parallel.
        read_write_analysis_result: Option<vector<u8>>
    }

    /// Enable parallel execution functionality of DiemVM by setting the read_write_set analysis result.
    public fun initialize_parallel_execution<T>(
        account: &signer,
    ) {
        DiemTimestamp::assert_genesis();
        SystemAddresses::assert_core_resource(account);

        assert!(
            !exists<ParallelExecutionConfigChainMarker<T>>(@CoreResources),
            errors::already_published(ECHAIN_MARKER)
        );

        assert!(
            !exists<ParallelExecutionConfig>(@CoreResources),
            errors::already_published(ECONFIG)
        );

        move_to(account, ParallelExecutionConfigChainMarker<T>{});

        move_to(
            account,
            ParallelExecutionConfig {
                read_write_analysis_result: option::none(),
            },
        );
    }

    public fun enable_parallel_execution_with_config<T>(
        read_write_inference_result: vector<u8>,
        _cap: &Cap<T>
    ) acquires ParallelExecutionConfig {
        DiemTimestamp::assert_operating();
        assert!(
            exists<ParallelExecutionConfigChainMarker<T>>(@CoreResources),
            errors::not_published(ECHAIN_MARKER)
        );
        let result_ref = &mut borrow_global_mut<ParallelExecutionConfig>(@CoreResources).read_write_analysis_result;
        *result_ref = option::some(read_write_inference_result);
        DiemConfig::reconfigure();
    }

    public fun disable_parallel_execution<T>(
        _cap: &Cap<T>
    ) acquires ParallelExecutionConfig {
        DiemTimestamp::assert_operating();
        assert!(
            exists<ParallelExecutionConfigChainMarker<T>>(@CoreResources),
            errors::not_published(ECHAIN_MARKER)
        );
        let result_ref = &mut borrow_global_mut<ParallelExecutionConfig>(@CoreResources).read_write_analysis_result;
        *result_ref = option::none();
        DiemConfig::reconfigure();
    }
}
