module ExperimentalFramework::ExperimentalValidatorSet {
    use std::capability;
    use CoreFramework::DiemSystem;

    struct ExperimentalValidatorSet has drop {}

    public fun initialize_validator_set(
        account: &signer,
    ) {
        DiemSystem::initialize_validator_set<ExperimentalValidatorSet>(account);
        capability::create(account, &ExperimentalValidatorSet {});
    }

    public fun add_validator(
        account: &signer,
        validator_addr: address,
    ) {
        DiemSystem::add_validator(
            validator_addr,
            capability::acquire(account, &ExperimentalValidatorSet {})
        );
    }

    public fun remove_validator(
        account: &signer,
        validator_addr: address,
    ) {
        DiemSystem::remove_validator(
            validator_addr,
            capability::acquire(account, &ExperimentalValidatorSet {})
        );
    }
}
