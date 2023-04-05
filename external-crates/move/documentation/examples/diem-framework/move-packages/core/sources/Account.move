module CoreFramework::Account {
    use std::bcs;
    use std::errors;
    use std::signer;
    use std::hash;
    use std::vector;
    use CoreFramework::ChainId;
    use CoreFramework::DiemConfig;
    use CoreFramework::SystemAddresses;

    /// Resource representing an account.
    struct Account has key, store {
        authentication_key: vector<u8>,
        sequence_number: u64,
        self_address: address,
    }

    /// A marker resource that registers the type `T` as the system marker for BasicAccount at genesis.
    struct Marker<phantom T> has key { }

    /// This holds information that will be picked up by the VM to call the
    /// correct chain-specific prologue and epilogue functions
    struct ChainSpecificAccountInfo has key {
        module_addr: address,
        module_name: vector<u8>,
        script_prologue_name: vector<u8>,
        module_prologue_name: vector<u8>,
        writeset_prologue_name: vector<u8>,
        multi_agent_prologue_name: vector<u8>,
        user_epilogue_name: vector<u8>,
        writeset_epilogue_name: vector<u8>,
        currency_code_required: bool,
    }

    const MAX_U64: u128 = 18446744073709551615;

    /// Account already existed
    const EACCOUNT: u64 = 0;
    /// Sequence number exceeded the maximum value for a u64
    const ESEQUENCE_NUMBER_TOO_BIG: u64 = 1;
    /// The address provided didn't match the `CoreFramework` address.
    const ENOT_CORE_FRAMEWORK: u64 = 2;
    /// The marker type provided is not the registered type for `Account`.
    const ENOT_MARKER_TYPE: u64 = 3;
    /// The provided authentication had an invalid length
    const EMALFORMED_AUTHENTICATION_KEY: u64 = 4;

    const PROLOGUE_EINVALID_ACCOUNT_AUTH_KEY: u64 = 1001;
    const PROLOGUE_ESEQUENCE_NUMBER_TOO_OLD: u64 = 1002;
    const PROLOGUE_ESEQUENCE_NUMBER_TOO_NEW: u64 = 1003;
    const PROLOGUE_EACCOUNT_DNE: u64 = 1004;
    const PROLOGUE_EBAD_CHAIN_ID: u64 = 1005;
    const PROLOGUE_ESEQUENCE_NUMBER_TOO_BIG: u64 = 1006;

    native fun create_signer(addr: address): signer;

    public fun initialize<T>(account: &signer,
        module_addr: address,
        module_name: vector<u8>,
        script_prologue_name: vector<u8>,
        module_prologue_name: vector<u8>,
        writeset_prologue_name: vector<u8>,
        multi_agent_prologue_name: vector<u8>,
        user_epilogue_name: vector<u8>,
        writeset_epilogue_name: vector<u8>,
        currency_code_required: bool,
    ) {
        assert!(signer::address_of(account) == @CoreResources, errors::requires_address(ENOT_CORE_FRAMEWORK));
        move_to(account, Marker<T> {});
        move_to(account, ChainSpecificAccountInfo {
            module_addr,
            module_name,
            script_prologue_name,
            module_prologue_name,
            writeset_prologue_name,
            multi_agent_prologue_name,
            user_epilogue_name,
            writeset_epilogue_name,
            currency_code_required,
        });
    }

    fun assert_is_marker<T>() {
        assert!(exists<Marker<T>>(@CoreResources), errors::invalid_argument(ENOT_MARKER_TYPE))
    }

    /// Construct an authentication key, aborting if the prefix is not valid.
    fun create_authentication_key(account: &signer, auth_key_prefix: vector<u8>): vector<u8> {
        let authentication_key = auth_key_prefix;
        vector::append(
            &mut authentication_key, bcs::to_bytes(signer::borrow_address(account))
        );
        assert!(
            vector::length(&authentication_key) == 32,
            errors::invalid_argument(EMALFORMED_AUTHENTICATION_KEY)
        );
        authentication_key
    }
    // spec create_authentication_key {
    //     /// The specification of this function is abstracted to avoid the complexity of
    //     /// vector concatenation of serialization results. The actual value of the key
    //     /// is assumed to be irrelevant for callers. Instead the uninterpreted function
    //     /// `spec_abstract_create_authentication_key` is used to represent the key value.
    //     /// The aborts behavior is, however, preserved: the caller must provide a
    //     /// key prefix of a specific length.
    //     pragma opaque;
    //     include [abstract] CreateAuthenticationKeyAbortsIf;
    //     ensures [abstract]
    //         result == spec_abstract_create_authentication_key(auth_key_prefix) &&
    //         len(result) == 32;
    // }
    // spec schema CreateAuthenticationKeyAbortsIf {
    //     auth_key_prefix: vector<u8>;
    //     aborts_if 16 + len(auth_key_prefix) != 32 with Errors::INVALID_ARGUMENT;
    // }
    // spec fun spec_abstract_create_authentication_key(auth_key_prefix: vector<u8>): vector<u8>;

    /// Publishes a new `Account` resource under `new_address`.
    /// A signer representing `new_address` is returned. This way, the caller of this function
    /// can publish additional resources under `new_address`.
    /// The `_witness` guarantees that owner the registered caller of this function can call it.
    /// authentication key returned is `auth_key_prefix` | `fresh_address`.
    public fun create_account<T>(
        new_address: address,
        authentication_key_prefix: vector<u8>,
        _witness: &T,
    ): (signer, vector<u8>) {
        assert_is_marker<T>();
        // there cannot be an Account resource under new_addr already.
        assert!(!exists<Account>(new_address), errors::already_published(EACCOUNT));

        let new_account = create_signer(new_address);
        let authentication_key = create_authentication_key(&new_account, authentication_key_prefix);
        move_to(
            &new_account,
            Account {
                authentication_key: copy authentication_key,
                sequence_number: 0,
                self_address: new_address,
            }
        );

        (new_account, authentication_key)
    }

    public fun exists_at(addr: address): bool {
        exists<Account>(addr)
    }

    public fun get_sequence_number(addr: address) : u64 acquires Account {
        borrow_global<Account>(addr).sequence_number
    }

    public fun get_authentication_key(addr: address) : vector<u8> acquires Account {
        *&borrow_global<Account>(addr).authentication_key
    }

    public fun rotate_authentication_key(
        account: &signer,
        new_auth_key: vector<u8>,
    ) acquires Account {
        let addr = signer::address_of(account);
        assert!(exists_at(addr), errors::not_published(EACCOUNT));
        assert!(
            vector::length(&new_auth_key) == 32,
            errors::invalid_argument(EMALFORMED_AUTHENTICATION_KEY)
        );
        let account_resource = borrow_global_mut<Account>(addr);
        account_resource.authentication_key = new_auth_key;
    }

    public fun prologue(
        account: &signer,
        txn_sequence_number: u64,
        txn_public_key: vector<u8>,
        chain_id: u8,
    ) acquires Account {
        let transaction_sender = signer::address_of(account);
        assert!(ChainId::get() == chain_id, errors::invalid_argument(PROLOGUE_EBAD_CHAIN_ID));
        assert!(exists<Account>(transaction_sender), errors::invalid_argument(PROLOGUE_EACCOUNT_DNE));
        let sender_account = borrow_global<Account>(transaction_sender);
        assert!(
            hash::sha3_256(txn_public_key) == *&sender_account.authentication_key,
            errors::invalid_argument(PROLOGUE_EINVALID_ACCOUNT_AUTH_KEY),
        );
        assert!(
            (txn_sequence_number as u128) < MAX_U64,
            errors::limit_exceeded(PROLOGUE_ESEQUENCE_NUMBER_TOO_BIG)
        );

        assert!(
            txn_sequence_number >= sender_account.sequence_number,
            errors::invalid_argument(PROLOGUE_ESEQUENCE_NUMBER_TOO_OLD)
        );

        // [PCA12]: Check that the transaction's sequence number matches the
        // current sequence number. Otherwise sequence number is too new by [PCA11].
        assert!(
            txn_sequence_number == sender_account.sequence_number,
            errors::invalid_argument(PROLOGUE_ESEQUENCE_NUMBER_TOO_NEW)
        );
    }

    /// Epilogue function is run after a transaction is successfully executed.
    /// Called by the Adaptor
    public fun epilogue<T>(account: &signer, _witness: &T) acquires Account {
        assert_is_marker<T>();
        let addr = signer::address_of(account);
        let old_sequence_number = get_sequence_number(addr);

        assert!(
            (old_sequence_number as u128) < MAX_U64,
            errors::limit_exceeded(ESEQUENCE_NUMBER_TOO_BIG)
        );

        // Increment sequence number
        let account_resource = borrow_global_mut<Account>(addr);
        account_resource.sequence_number = old_sequence_number + 1;
    }

    /// Epilogue function called after a successful writeset transaction, which can only be sent by @CoreResources.
    public fun writeset_epilogue<T>(
        account: &signer,
        should_trigger_reconfiguration: bool,
        witness: &T
    ) acquires Account {
        SystemAddresses::assert_core_resource(account);
        epilogue(account, witness);
        if (should_trigger_reconfiguration) DiemConfig::reconfigure();
    }
}
