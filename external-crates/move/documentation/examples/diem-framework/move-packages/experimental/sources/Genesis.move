module ExperimentalFramework::Genesis {
    use std::signer;
    use std::event;
    use std::vector;
    use CoreFramework::CoreGenesis;
    use ExperimentalFramework::ExperimentalAccount;

    // Config imports
    use CoreFramework::ValidatorConfig;
    use CoreFramework::ValidatorOperatorConfig;
    use ExperimentalFramework::ExperimentalConsensusConfig;
    use ExperimentalFramework::ExperimentalParallelExecutionConfig;
    use ExperimentalFramework::ExperimentalValidatorConfig;
    use ExperimentalFramework::ExperimentalValidatorOperatorConfig;
    use ExperimentalFramework::ExperimentalValidatorSet;
    use ExperimentalFramework::ExperimentalVersion;
    use ExperimentalFramework::ExperimentalVMConfig;

    // This function needs the same signature as the DPN genesis
    fun initialize(
        dr_account: signer,
        _tc_account: signer,
        dr_auth_key: vector<u8>,
        _tc_auth_key: vector<u8>,
        _initial_script_allow_list: vector<vector<u8>>,
        _is_open_module: bool,
        instruction_schedule: vector<u8>,
        native_schedule: vector<u8>,
        chain_id: u8,
        initial_diem_version: u64,
        consensus_config: vector<u8>,
    ) {
        initialize_internal(
            &dr_account,
            dr_auth_key,
            instruction_schedule,
            native_schedule,
            chain_id,
            initial_diem_version,
            consensus_config,
        )
    }

    fun initialize_internal(
        dr_account: &signer,
        dr_auth_key: vector<u8>,
        instruction_schedule: vector<u8>,
        native_schedule: vector<u8>,
        chain_id: u8,
        initial_diem_version: u64,
        consensus_config: vector<u8>,
    ) {
        ExperimentalAccount::initialize(dr_account, x"00000000000000000000000000000000");

        // Pad the event counter for the Diem Root account to match DPN. This
        // _MUST_ match the new epoch event counter otherwise all manner of
        // things start to break.
        event::destroy_handle(event::new_event_handle<u64>(dr_account));
        event::destroy_handle(event::new_event_handle<u64>(dr_account));
        event::destroy_handle(event::new_event_handle<u64>(dr_account));

        // Consensus config setup
        ExperimentalConsensusConfig::initialize(dr_account);
        // Parallel execution config setup
        ExperimentalParallelExecutionConfig::initialize_parallel_execution(dr_account);
        ExperimentalValidatorSet::initialize_validator_set(dr_account);
        ExperimentalVersion::initialize(dr_account, initial_diem_version);

        // Rotate auth keys for DiemRoot account to the given
        // values
        ExperimentalAccount::rotate_authentication_key(dr_account, dr_auth_key);
        ExperimentalVMConfig::initialize(
            dr_account,
            instruction_schedule,
            native_schedule,
        );

        ExperimentalConsensusConfig::set(dr_account, consensus_config);

        ExperimentalValidatorConfig::initialize(dr_account);
        ExperimentalValidatorOperatorConfig::initialize(dr_account);

        // this needs to be called at the very end
        CoreGenesis::init(dr_account, chain_id);
    }

    /// Sets up the initial validator set for the Diem network.
    /// The validator "owner" accounts, their UTF-8 names, and their authentication
    /// keys are encoded in the `owners`, `owner_names`, and `owner_auth_key` vectors.
    /// Each validator signs consensus messages with the private key corresponding to the Ed25519
    /// public key in `consensus_pubkeys`.
    /// Each validator owner has its operation delegated to an "operator" (which may be
    /// the owner). The operators, their names, and their authentication keys are encoded
    /// in the `operators`, `operator_names`, and `operator_auth_keys` vectors.
    /// Finally, each validator must specify the network address
    /// (see diem/types/src/network_address/mod.rs) for itself and its full nodes.
    fun create_initialize_owners_operators(
        dr_account: signer,
        owners: vector<signer>,
        owner_names: vector<vector<u8>>,
        owner_auth_keys: vector<vector<u8>>,
        consensus_pubkeys: vector<vector<u8>>,
        operators: vector<signer>,
        operator_names: vector<vector<u8>>,
        operator_auth_keys: vector<vector<u8>>,
        validator_network_addresses: vector<vector<u8>>,
        full_node_network_addresses: vector<vector<u8>>,
    ) {
        let num_owners = vector::length(&owners);
        let num_owner_names = vector::length(&owner_names);
        assert!(num_owners == num_owner_names, 0);
        let num_owner_keys = vector::length(&owner_auth_keys);
        assert!(num_owner_names == num_owner_keys, 0);
        let num_operators = vector::length(&operators);
        assert!(num_owner_keys == num_operators, 0);
        let num_operator_names = vector::length(&operator_names);
        assert!(num_operators == num_operator_names, 0);
        let num_operator_keys = vector::length(&operator_auth_keys);
        assert!(num_operator_names == num_operator_keys, 0);
        let num_validator_network_addresses = vector::length(&validator_network_addresses);
        assert!(num_operator_keys == num_validator_network_addresses, 0);
        let num_full_node_network_addresses = vector::length(&full_node_network_addresses);
        assert!(num_validator_network_addresses == num_full_node_network_addresses, 0);

        let i = 0;
        let dummy_auth_key_prefix = x"00000000000000000000000000000000";
        while (i < num_owners) {
            let owner = vector::borrow(&owners, i);
            let owner_address = signer::address_of(owner);
            let owner_name = *vector::borrow(&owner_names, i);
            // create each validator account and rotate its auth key to the correct value
            ExperimentalAccount::create_validator_account(
                &dr_account, owner_address, copy dummy_auth_key_prefix, owner_name
            );

            let owner_auth_key = *vector::borrow(&owner_auth_keys, i);
            ExperimentalAccount::rotate_authentication_key(owner, owner_auth_key);

            let operator = vector::borrow(&operators, i);
            let operator_address = signer::address_of(operator);
            let operator_name = *vector::borrow(&operator_names, i);
            // create the operator account + rotate its auth key if it does not already exist
            if (!ExperimentalAccount::exists_at(operator_address)) {
                ExperimentalAccount::create_validator_operator_account(
                    &dr_account, operator_address, copy dummy_auth_key_prefix, copy operator_name
                );
                let operator_auth_key = *vector::borrow(&operator_auth_keys, i);
                ExperimentalAccount::rotate_authentication_key(operator, operator_auth_key);
            };
            // assign the operator to its validator
            assert!(ValidatorOperatorConfig::get_human_name(operator_address) == operator_name, 0);
            ValidatorConfig::set_operator(owner, operator_address);

            // use the operator account set up the validator config
            let validator_network_address = *vector::borrow(&validator_network_addresses, i);
            let full_node_network_address = *vector::borrow(&full_node_network_addresses, i);
            let consensus_pubkey = *vector::borrow(&consensus_pubkeys, i);
            ValidatorConfig::set_config(
                operator,
                owner_address,
                consensus_pubkey,
                validator_network_address,
                full_node_network_address
            );

            // finally, add this validator to the validator set
            ExperimentalValidatorSet::add_validator(&dr_account, owner_address);

            i = i + 1;
        }
    }

    #[test_only]
    public fun setup(dr_account: &signer) {
        initialize_internal(
            dr_account,
            x"0000000000000000000000000000000000000000000000000000000000000000",
            x"", // instruction_schedule not needed for unit tests
            x"", // native schedule not needed for unit tests
            4u8, // TESTING chain ID
            0,
            x""
        )
    }

    #[test(account = @CoreResources)]
    fun test_setup(account: signer) {
        setup(&account);
    }
}
