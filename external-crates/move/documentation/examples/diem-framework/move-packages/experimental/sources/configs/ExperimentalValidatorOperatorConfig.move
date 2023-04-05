module ExperimentalFramework::ExperimentalValidatorOperatorConfig {
    use std::capability;
    use CoreFramework::ValidatorOperatorConfig;

    friend ExperimentalFramework::ExperimentalAccount;

    struct ExperimentalValidatorOperatorConfig has drop {}

    public fun initialize(account: &signer) {
        ValidatorOperatorConfig::initialize<ExperimentalValidatorOperatorConfig>(account);
        capability::create(account, &ExperimentalValidatorOperatorConfig{});
    }

    public(friend) fun publish(
        root_account: &signer,
        validator_operator_account: &signer,
        human_name: vector<u8>,
    ) {
        ValidatorOperatorConfig::publish(
            validator_operator_account,
            human_name,
            capability::acquire(root_account, &ExperimentalValidatorOperatorConfig{})
        );
    }
}
