/// Stores the string name of a ValidatorOperator account.
module CoreFramework::ValidatorOperatorConfig {
    use std::capability::Cap;
    use std::errors;
    use std::signer;
    use CoreFramework::DiemTimestamp;
    use CoreFramework::SystemAddresses;

    /// Marker to be stored under @CoreResources during genesis
    struct ValidatorOperatorConfigChainMarker<phantom T> has key {}

    struct ValidatorOperatorConfig has key {
        /// The human readable name of this entity. Immutable.
        human_name: vector<u8>,
    }

    /// The `ValidatorOperatorConfig` was not in the required state
    const EVALIDATOR_OPERATOR_CONFIG: u64 = 0;
    /// The `ValidatorOperatorConfigChainMarker` resource was not in the required state
    const ECHAIN_MARKER: u64 = 9;

    public fun initialize<T>(account: &signer) {
        DiemTimestamp::assert_genesis();
        SystemAddresses::assert_core_resource(account);

        assert!(
            !exists<ValidatorOperatorConfigChainMarker<T>>(@CoreResources),
            errors::already_published(ECHAIN_MARKER)
        );
        move_to(account, ValidatorOperatorConfigChainMarker<T>{});
    }

    public fun publish<T>(
        validator_operator_account: &signer,
        human_name: vector<u8>,
        _cap: Cap<T>
    ) {
        DiemTimestamp::assert_operating();
        assert!(
            exists<ValidatorOperatorConfigChainMarker<T>>(@CoreResources),
            errors::not_published(ECHAIN_MARKER)
        );

        assert!(
            !has_validator_operator_config(signer::address_of(validator_operator_account)),
            errors::already_published(EVALIDATOR_OPERATOR_CONFIG)
        );

        move_to(validator_operator_account, ValidatorOperatorConfig {
            human_name,
        });
    }

    /// Get validator's account human name
    /// Aborts if there is no ValidatorOperatorConfig resource
    public fun get_human_name(validator_operator_addr: address): vector<u8> acquires ValidatorOperatorConfig {
        assert!(has_validator_operator_config(validator_operator_addr), errors::not_published(EVALIDATOR_OPERATOR_CONFIG));
        *&borrow_global<ValidatorOperatorConfig>(validator_operator_addr).human_name
    }

    public fun has_validator_operator_config(validator_operator_addr: address): bool {
        exists<ValidatorOperatorConfig>(validator_operator_addr)
    }
}
