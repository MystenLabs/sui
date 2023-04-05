/// The `ExperimentalAccount` module manages experimental accounts.
/// It also defines the prolog and epilog that run before and after every
/// transaction in addition to the core prologue and epilogue.

module ExperimentalFramework::ExperimentalAccount {
    use std::event;
    use std::errors;
    use CoreFramework::Account;
    use CoreFramework::DiemTimestamp;
    use CoreFramework::SystemAddresses;

    use ExperimentalFramework::ExperimentalValidatorConfig;
    use ExperimentalFramework::ExperimentalValidatorOperatorConfig;

    /// A resource that holds the event handle for all the past WriteSet transactions that have been committed on chain.
    struct DiemWriteSetManager has key {
        upgrade_events: event::EventHandle<AdminTransactionEvent>,
    }

    /// Message for committed WriteSet transaction.
    struct AdminTransactionEvent has drop, store {
        // The block time when this WriteSet is committed.
        committed_timestamp_secs: u64,
    }

    const MAX_U64: u128 = 18446744073709551615;

    /// The `ExperimentalAccount` is not in the required state
    const EACCOUNT: u64 = 0;
    /// Tried to deposit a coin whose value was zero
    const ECOIN_DEPOSIT_IS_ZERO: u64 = 2;
    /// Tried to deposit funds that would have surpassed the account's limits
    const EDEPOSIT_EXCEEDS_LIMITS: u64 = 3;
    /// Tried to create a balance for an account whose role does not allow holding balances
    const EROLE_CANT_STORE_BALANCE: u64 = 4;
    /// The account does not hold a large enough balance in the specified currency
    const EINSUFFICIENT_BALANCE: u64 = 5;
    /// The withdrawal of funds would have exceeded the the account's limits
    const EWITHDRAWAL_EXCEEDS_LIMITS: u64 = 6;
    /// The `WithdrawCapability` for this account has already been extracted
    const EWITHDRAW_CAPABILITY_ALREADY_EXTRACTED: u64 = 7;
    /// The provided authentication had an invalid length
    const EMALFORMED_AUTHENTICATION_KEY: u64 = 8;
    /// The `KeyRotationCapability` for this account has already been extracted
    const EKEY_ROTATION_CAPABILITY_ALREADY_EXTRACTED: u64 = 9;
    /// An account cannot be created at the reserved VM address of 0x0
    const ECANNOT_CREATE_AT_VM_RESERVED: u64 = 10;
    /// The `WithdrawCapability` for this account is not extracted
    const EWITHDRAW_CAPABILITY_NOT_EXTRACTED: u64 = 11;
    /// Tried to add a balance in a currency that this account already has
    const EADD_EXISTING_CURRENCY: u64 = 15;
    /// Attempted to send funds to an account that does not exist
    const EPAYEE_DOES_NOT_EXIST: u64 = 17;
    /// Attempted to send funds in a currency that the receiving account does not hold.
    /// e.g., `Diem<XDX>` to an account that exists, but does not have a `Balance<XDX>` resource
    const EPAYEE_CANT_ACCEPT_CURRENCY_TYPE: u64 = 18;
    /// Tried to withdraw funds in a currency that the account does hold
    const EPAYER_DOESNT_HOLD_CURRENCY: u64 = 19;
    /// An invalid amount of gas units was provided for execution of the transaction
    const EGAS: u64 = 20;
    /// The `AccountOperationsCapability` was not in the required state
    const EACCOUNT_OPERATIONS_CAPABILITY: u64 = 22;
    /// The `DiemWriteSetManager` was not in the required state
    const EWRITESET_MANAGER: u64 = 23;
    /// An account cannot be created at the reserved core code address of 0x1
    const ECANNOT_CREATE_AT_CORE_CODE: u64 = 24;

    struct ExperimentalAccountMarker has drop {}

    fun create_core_account(account_address: address, auth_key_prefix: vector<u8>): (signer, vector<u8>) {
        assert!(
            account_address != @VMReserved,
            errors::invalid_argument(ECANNOT_CREATE_AT_VM_RESERVED)
        );
        assert!(
            account_address != @CoreFramework,
            errors::invalid_argument(ECANNOT_CREATE_AT_CORE_CODE)
        );
        Account::create_account(account_address, auth_key_prefix, &ExperimentalAccountMarker{})
    }

    /// Initialize this module. This is only callable from genesis.
    public fun initialize(
        dr_account: &signer,
        dummy_auth_key_prefix: vector<u8>,
    ) {
        DiemTimestamp::assert_genesis();
        // Operational constraint, not a privilege constraint.
        SystemAddresses::assert_core_resource(dr_account);
        Account::initialize<ExperimentalAccountMarker>(
            dr_account,
            @DiemFramework,
            b"ExperimentalAccount",
            b"script_prologue",
            b"module_prologue",
            b"writeset_prologue",
            b"script_prologue",
            b"epilogue",
            b"writeset_epilogue",
            false,
        );

        // TODO: For legacy reasons. Remove
        create_diem_root_account(
            copy dummy_auth_key_prefix,
        );
    }

    ///////////////////////////////////////////////////////////////////////////
    /// Basic account creation method: no roles attached, no conditions checked.
    ///////////////////////////////////////////////////////////////////////////

    public fun create_account(
        new_account_address: address,
        auth_key_prefix: vector<u8>,
    ) {
        create_core_account(new_account_address, auth_key_prefix);
        // No role attached
    }

    public fun exists_at(addr: address): bool {
        Account::exists_at(addr)
    }

    /// Creates the diem root account (during genesis). Publishes the Diem root role,
    /// Publishes a SlidingNonce resource, sets up event generator, publishes
    /// AccountOperationsCapability, WriteSetManager, and finally makes the account.
    fun create_diem_root_account(
        auth_key_prefix: vector<u8>,
    ) {
        DiemTimestamp::assert_genesis();
        let (dr_account, _) = create_core_account(@CoreResources, auth_key_prefix);
        SystemAddresses::assert_core_resource(&dr_account);
        assert!(
            !exists<DiemWriteSetManager>(@CoreResources),
            errors::already_published(EWRITESET_MANAGER)
        );
        move_to(
            &dr_account,
            DiemWriteSetManager {
                upgrade_events: event::new_event_handle<AdminTransactionEvent>(&dr_account),
            }
        );
    }

    /// Create a Validator account
    public fun create_validator_account(
        dr_account: &signer,
        new_account_address: address,
        auth_key_prefix: vector<u8>,
        human_name: vector<u8>,
    ) {
        let (new_account, _) = create_core_account(new_account_address, auth_key_prefix);
        ExperimentalValidatorConfig::publish(dr_account, &new_account, human_name);
    }

    /// Create a Validator Operator account
    public fun create_validator_operator_account(
        dr_account: &signer,
        new_account_address: address,
        auth_key_prefix: vector<u8>,
        human_name: vector<u8>,
    ) {
        let (new_account, _) = create_core_account(new_account_address, auth_key_prefix);
        ExperimentalValidatorOperatorConfig::publish(dr_account, &new_account, human_name);
    }

    /// Rotate the authentication key for the account under cap.account_address
    public fun rotate_authentication_key(
        account: &signer,
        new_authentication_key: vector<u8>,
    ) {
        Account::rotate_authentication_key(account, new_authentication_key)
    }

    ///////////////////////////////////////////////////////////////////////////
    // Prologues and epilogues
    ///////////////////////////////////////////////////////////////////////////
    fun module_prologue(
        sender: signer,
        txn_sequence_number: u64,
        txn_public_key: vector<u8>,
        _txn_gas_price: u64,
        _txn_max_gas_units: u64,
        _txn_expiration_time: u64,
        chain_id: u8,
    ) {
        Account::prologue(&sender, txn_sequence_number, txn_public_key, chain_id)
    }

    fun script_prologue(
        sender: signer,
        txn_sequence_number: u64,
        txn_public_key: vector<u8>,
        _txn_gas_price: u64,
        _txn_max_gas_units: u64,
        _txn_expiration_time: u64,
        chain_id: u8,
        _script_hash: vector<u8>,
    ) {
        Account::prologue(&sender, txn_sequence_number, txn_public_key, chain_id)
    }

    fun writeset_prologue(
        sender: signer,
        txn_sequence_number: u64,
        txn_public_key: vector<u8>,
        _txn_expiration_time: u64,
        chain_id: u8,
    ) {
        Account::prologue(&sender, txn_sequence_number, txn_public_key, chain_id)
    }

    // Might be able to combine this
    fun multi_agent_script_prologue(
        sender: signer,
        txn_sequence_number: u64,
        txn_sender_public_key: vector<u8>,
        _secondary_signer_addresses: vector<address>,
        _secondary_signer_public_key_hashes: vector<vector<u8>>,
        _txn_gas_price: u64,
        _txn_max_gas_units: u64,
        _txn_expiration_time: u64,
        chain_id: u8,
    ) {
        Account::prologue(&sender, txn_sequence_number, txn_sender_public_key, chain_id)
    }

    fun epilogue(
        account: signer,
        _txn_sequence_number: u64,
        _txn_gas_price: u64,
        _txn_max_gas_units: u64,
        _gas_units_remaining: u64
    ) {
        Account::epilogue(&account, &ExperimentalAccountMarker{});
    }

    fun writeset_epilogue(
        dr_account: signer,
        _txn_sequence_number: u64,
        should_trigger_reconfiguration: bool,
    ) {
        Account::writeset_epilogue(&dr_account, should_trigger_reconfiguration, &ExperimentalAccountMarker{});
    }
}
