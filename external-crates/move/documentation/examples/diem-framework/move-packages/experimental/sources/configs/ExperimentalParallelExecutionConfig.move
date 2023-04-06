module ExperimentalFramework::ExperimentalParallelExecutionConfig {
    use std::capability;
    use CoreFramework::ParallelExecutionConfig;

    struct ExperimentalParallelExecutionConfig has drop {}

    public fun initialize_parallel_execution(
        account: &signer,
    ) {
        ParallelExecutionConfig::initialize_parallel_execution<ExperimentalParallelExecutionConfig>(account);
        capability::create<ExperimentalParallelExecutionConfig>(
            account,
            &ExperimentalParallelExecutionConfig {}
        );
    }

    public fun enable_parallel_execution_with_config(
        account: &signer,
        read_write_inference_result: vector<u8>,
    ) {
        ParallelExecutionConfig::enable_parallel_execution_with_config(
            read_write_inference_result,
            &capability::acquire(account, &ExperimentalParallelExecutionConfig {}),
        );
    }

    public fun disable_parallel_execution(account: &signer) {
        ParallelExecutionConfig::disable_parallel_execution(
            &capability::acquire(account, &ExperimentalParallelExecutionConfig {}),
        );
    }
}
