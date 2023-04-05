/// Maintains information about the set of validators used during consensus.
/// Provides functions to add, remove, and update validators in the
/// validator set.
///
/// > Note: When trying to understand this code, it's important to know that "config"
/// and "configuration" are used for several distinct concepts.
module CoreFramework::DiemSystem {
    use std::capability::Cap;
    use std::errors;
    use std::option::{Self, Option};
    use std::signer;
    use std::vector;
    use CoreFramework::DiemConfig;
    use CoreFramework::DiemTimestamp;
    use CoreFramework::SystemAddresses;
    use CoreFramework::ValidatorConfig;

    /// Marker to be stored under @CoreResources during genesis
    struct ValidatorSetChainMarker<phantom T> has key {}

    /// Information about a Validator Owner.
    struct ValidatorInfo has copy, drop, store {
        /// The address (account) of the Validator Owner
        addr: address,
        /// The voting power of the Validator Owner (currently always 1).
        consensus_voting_power: u64,
        /// Configuration information about the Validator, such as the
        /// Validator Operator, human name, and info such as consensus key
        /// and network addresses.
        config: ValidatorConfig::Config,
        /// The time of last reconfiguration invoked by this validator
        /// in microseconds
        last_config_update_time: u64,
    }

    /// The DiemSystem struct stores the validator set and crypto scheme in
    /// DiemConfig. The DiemSystem struct is stored by DiemConfig, which publishes a
    /// DiemConfig<DiemSystem> resource.
    struct DiemSystem has key, copy, drop {
        /// The current consensus crypto scheme.
        scheme: u8,
        /// The current validator set.
        validators: vector<ValidatorInfo>,
    }

    /// The `DiemSystem` resource was not in the required state
    const ECONFIG: u64 = 0;
    /// Tried to add a validator with an invalid state to the validator set
    const EINVALID_PROSPECTIVE_VALIDATOR: u64 = 1;
    /// Tried to add a validator to the validator set that was already in it
    const EALREADY_A_VALIDATOR: u64 = 2;
    /// An operation was attempted on an address not in the vaidator set
    const ENOT_AN_ACTIVE_VALIDATOR: u64 = 3;
    /// The validator operator is not the operator for the specified validator
    const EINVALID_TRANSACTION_SENDER: u64 = 4;
    /// An out of bounds index for the validator set was encountered
    const EVALIDATOR_INDEX: u64 = 5;
    /// Rate limited when trying to update config
    const ECONFIG_UPDATE_RATE_LIMITED: u64 = 6;
    /// Validator set already at maximum allowed size
    const EMAX_VALIDATORS: u64 = 7;
    /// Validator config update time overflows
    const ECONFIG_UPDATE_TIME_OVERFLOWS: u64 = 8;
    /// The `ValidatorSetChainMarker` resource was not in the required state
    const ECHAIN_MARKER: u64 = 9;

    /// Number of microseconds in 5 minutes
    const FIVE_MINUTES: u64 = 300000000;

    /// The maximum number of allowed validators in the validator set
    const MAX_VALIDATORS: u64 = 256;

    /// The largest possible u64 value
    const MAX_U64: u64 = 18446744073709551615;

    ///////////////////////////////////////////////////////////////////////////
    // Setup methods
    ///////////////////////////////////////////////////////////////////////////


    /// Publishes the DiemSystem struct, which contains the current validator set.
    /// Must be invoked by @CoreResources a single time in Genesis.
    public fun initialize_validator_set<T>(
        account: &signer,
    ) {
        DiemTimestamp::assert_genesis();
        SystemAddresses::assert_core_resource(account);

        assert!(!exists<ValidatorSetChainMarker<T>>(@CoreResources), errors::already_published(ECHAIN_MARKER));
        assert!(!exists<DiemSystem>(@CoreResources), errors::already_published(ECONFIG));
        move_to(account, ValidatorSetChainMarker<T>{});
        move_to(
            account,
            DiemSystem {
                scheme: 0,
                validators: vector::empty(),
            },
        );
    }

    /// Copies a DiemSystem struct into the DiemSystem resource
    /// Called by the add, remove, and update functions.
    fun set_diem_system_config(value: DiemSystem) acquires DiemSystem {
        DiemTimestamp::assert_operating();
        assert!(
            exists<DiemSystem>(@CoreResources),
            errors::not_published(ECONFIG)
        );
        // Updates the DiemSystem and emits a reconfigure event.
        let config_ref = borrow_global_mut<DiemSystem>(@CoreResources);
        *config_ref = value;
        DiemConfig::reconfigure();
    }

    ///////////////////////////////////////////////////////////////////////////
    // Methods operating the Validator Set config callable by the diem root account
    ///////////////////////////////////////////////////////////////////////////

    /// Adds a new validator to the validator set.
    public fun add_validator<T>(
        validator_addr: address,
        _cap: Cap<T>
    ) acquires DiemSystem {
        DiemTimestamp::assert_operating();
        assert_chain_marker_is_published<T>();

        // A prospective validator must have a validator config resource
        assert!(
            ValidatorConfig::is_valid(validator_addr),
            errors::invalid_argument(EINVALID_PROSPECTIVE_VALIDATOR)
        );

        // Bound the validator set size
        assert!(
            validator_set_size() < MAX_VALIDATORS,
            errors::limit_exceeded(EMAX_VALIDATORS)
        );

        let diem_system_config = get_diem_system_config();

        // Ensure that this address is not already a validator
        assert!(
            !is_validator_(validator_addr, &diem_system_config.validators),
            errors::invalid_argument(EALREADY_A_VALIDATOR)
        );

        // it is guaranteed that the config is non-empty
        let config = ValidatorConfig::get_config(validator_addr);
        vector::push_back(&mut diem_system_config.validators, ValidatorInfo {
            addr: validator_addr,
            config, // copy the config over to ValidatorSet
            consensus_voting_power: 1,
            last_config_update_time: DiemTimestamp::now_microseconds(),
        });

        set_diem_system_config(diem_system_config);
    }

    /// Removes a validator, aborts unless called by diem root account
    public fun remove_validator<T>(
        validator_addr: address,
        _cap: Cap<T>
    ) acquires DiemSystem {
        DiemTimestamp::assert_operating();
        assert_chain_marker_is_published<T>();

        let diem_system_config = get_diem_system_config();
        // Ensure that this address is an active validator
        let to_remove_index_vec = get_validator_index_(&diem_system_config.validators, validator_addr);
        assert!(option::is_some(&to_remove_index_vec), errors::invalid_argument(ENOT_AN_ACTIVE_VALIDATOR));
        let to_remove_index = *option::borrow(&to_remove_index_vec);
        // Remove corresponding ValidatorInfo from the validator set
        _  = vector::swap_remove(&mut diem_system_config.validators, to_remove_index);

        set_diem_system_config(diem_system_config);
    }

    /// Copy the information from ValidatorConfig into the validator set.
    /// This function makes no changes to the size or the members of the set.
    /// If the config in the ValidatorSet changes, it stores the new DiemSystem
    /// and emits a reconfigurationevent.
    public fun update_config_and_reconfigure(
        validator_operator_account: &signer,
        validator_addr: address,
    ) acquires DiemSystem {
        DiemTimestamp::assert_operating();
        assert!(
            ValidatorConfig::get_operator(validator_addr) == signer::address_of(validator_operator_account),
            errors::invalid_argument(EINVALID_TRANSACTION_SENDER)
        );
        let diem_system_config = get_diem_system_config();
        let to_update_index_vec = get_validator_index_(&diem_system_config.validators, validator_addr);
        assert!(option::is_some(&to_update_index_vec), errors::invalid_argument(ENOT_AN_ACTIVE_VALIDATOR));
        let to_update_index = *option::borrow(&to_update_index_vec);
        let is_validator_info_updated = update_ith_validator_info_(&mut diem_system_config.validators, to_update_index);
        if (is_validator_info_updated) {
            let validator_info = vector::borrow_mut(&mut diem_system_config.validators, to_update_index);
            assert!(
                validator_info.last_config_update_time <= MAX_U64 - FIVE_MINUTES,
                errors::limit_exceeded(ECONFIG_UPDATE_TIME_OVERFLOWS)
            );
            assert!(
                DiemTimestamp::now_microseconds() > validator_info.last_config_update_time + FIVE_MINUTES,
                errors::limit_exceeded(ECONFIG_UPDATE_RATE_LIMITED)
            );
            validator_info.last_config_update_time = DiemTimestamp::now_microseconds();
            set_diem_system_config(diem_system_config);
        }
    }

    ///////////////////////////////////////////////////////////////////////////
    // Publicly callable APIs: getters
    ///////////////////////////////////////////////////////////////////////////

    /// Get the DiemSystem configuration from DiemConfig
    public fun get_diem_system_config(): DiemSystem acquires DiemSystem {
        *borrow_global<DiemSystem>(@CoreResources)
    }

    /// Return true if `addr` is in the current validator set
    public fun is_validator(addr: address): bool acquires DiemSystem {
        is_validator_(addr, &get_diem_system_config().validators)
    }

    /// Returns validator config. Aborts if `addr` is not in the validator set.
    public fun get_validator_config(addr: address): ValidatorConfig::Config acquires DiemSystem {
        let diem_system_config = get_diem_system_config();
        let validator_index_vec = get_validator_index_(&diem_system_config.validators, addr);
        assert!(option::is_some(&validator_index_vec), errors::invalid_argument(ENOT_AN_ACTIVE_VALIDATOR));
        *&(vector::borrow(&diem_system_config.validators, *option::borrow(&validator_index_vec))).config
    }

    /// Return the size of the current validator set
    public fun validator_set_size(): u64 acquires DiemSystem {
        vector::length(&get_diem_system_config().validators)
    }

    /// Get the `i`'th validator address in the validator set.
    public fun get_ith_validator_address(i: u64): address acquires DiemSystem{
        assert!(i < validator_set_size(), errors::invalid_argument(EVALIDATOR_INDEX));
        vector::borrow(&get_diem_system_config().validators, i).addr
    }

    ///////////////////////////////////////////////////////////////////////////
    // Private functions
    ///////////////////////////////////////////////////////////////////////////

    fun assert_chain_marker_is_published<T>() {
        assert!(exists<ValidatorSetChainMarker<T>>(@CoreResources), errors::not_published(ECHAIN_MARKER));
    }


    /// Get the index of the validator by address in the `validators` vector
    /// It has a loop, so there are spec blocks in the code to assert loop invariants.
    fun get_validator_index_(validators: &vector<ValidatorInfo>, addr: address): Option<u64> {
        let size = vector::length(validators);
        let i = 0;
        while (i < size) {
            let validator_info_ref = vector::borrow(validators, i);
            if (validator_info_ref.addr == addr) {
                return option::some(i)
            };
            i = i + 1;
        };
        return option::none()
    }

    /// Updates *i*th validator info, if nothing changed, return false.
    /// This function never aborts.
    fun update_ith_validator_info_(validators: &mut vector<ValidatorInfo>, i: u64): bool {
        let size = vector::length(validators);
        // This provably cannot happen, but left it here for safety.
        if (i >= size) {
            return false
        };
        let validator_info = vector::borrow_mut(validators, i);
        // "is_valid" below should always hold based on a global invariant later
        // in the file (which proves if we comment out some other specifications),
        // but it is left here for safety.
        if (!ValidatorConfig::is_valid(validator_info.addr)) {
            return false
        };
        let new_validator_config = ValidatorConfig::get_config(validator_info.addr);
        // check if information is the same
        let config_ref = &mut validator_info.config;
        if (config_ref == &new_validator_config) {
            return false
        };
        *config_ref = new_validator_config;
        true
    }

    /// Private function checks for membership of `addr` in validator set.
    fun is_validator_(addr: address, validators_vec_ref: &vector<ValidatorInfo>): bool {
        option::is_some(&get_validator_index_(validators_vec_ref, addr))
    }
}
