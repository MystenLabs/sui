module ExperimentalFramework::ExperimentalConsensusConfig {
    use std::capability;
    use CoreFramework::DiemConsensusConfig;

    struct ExperimentalConsensusConfig has drop {}

    public fun initialize(account: &signer) {
        DiemConsensusConfig::initialize<ExperimentalConsensusConfig>(account);
        capability::create<ExperimentalConsensusConfig>(account, &ExperimentalConsensusConfig {});
    }

    public fun set(account: &signer, config: vector<u8>) {
        DiemConsensusConfig::set(
            config, &capability::acquire(account, &ExperimentalConsensusConfig {})
        );
    }
}
