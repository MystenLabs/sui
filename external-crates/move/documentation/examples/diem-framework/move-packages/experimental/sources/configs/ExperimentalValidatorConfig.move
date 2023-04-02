module ExperimentalFramework::ExperimentalValidatorConfig {
    use std::capability;
    use CoreFramework::ValidatorConfig;

    friend ExperimentalFramework::ExperimentalAccount;

    struct ExperimentalValidatorConfig has drop {}

    public fun initialize(account: &signer) {
        ValidatorConfig::initialize<ExperimentalValidatorConfig>(account);
        capability::create(account, &ExperimentalValidatorConfig{});
    }

    public(friend) fun publish(
        root_account: &signer,
        validator_account: &signer,
        human_name: vector<u8>,
    ) {
        ValidatorConfig::publish(
            validator_account,
            human_name,
            capability::acquire(root_account, &ExperimentalValidatorConfig{})
        );
    }
}
