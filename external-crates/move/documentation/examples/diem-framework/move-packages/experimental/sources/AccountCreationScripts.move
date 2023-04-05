module ExperimentalFramework::AccountCreationScripts {
    use ExperimentalFramework::ExperimentalAccount;

    /// # Summary
    /// Creates a Validator Operator account. This transaction can only be sent by the Diem
    /// Root account.
    ///
    /// # Technical Description
    /// Creates an account with a Validator Operator role at `new_account_address`, with authentication key
    /// `auth_key_prefix` | `new_account_address`. It publishes a
    /// `ValidatorOperatorConfig::ValidatorOperatorConfig` resource with the specified `human_name`.
    /// This script does not assign the validator operator to any validator accounts but only creates the account.
    ///
    /// # Events
    /// Successful execution will emit:
    /// * A `ExperimentalAccount::CreateAccountEvent` with the `created` field being `new_account_address`,
    /// and the `rold_id` field being `Roles::VALIDATOR_OPERATOR_ROLE_ID`. This is emitted on the
    /// `ExperimentalAccount::AccountOperationsCapability` `creation_events` handle.
    ///
    /// # Parameters
    /// | Name                  | Type         | Description                                                                              |
    /// | ------                | ------       | -------------                                                                            |
    /// | `dr_account`          | `signer`     | The signer of the sending account of this transaction. Must be the Diem Root signer.     |
    /// | `sliding_nonce`       | `u64`        | The `sliding_nonce` (see: `SlidingNonce`) to be used for this transaction.               |
    /// | `new_account_address` | `address`    | Address of the to-be-created Validator account.                                          |
    /// | `auth_key_prefix`     | `vector<u8>` | The authentication key prefix that will be used initially for the newly created account. |
    /// | `human_name`          | `vector<u8>` | ASCII-encoded human name for the validator.                                              |
    ///
    /// # Common Abort Conditions
    /// | Error Category              | Error Reason                            | Description                                                                                |
    /// | ----------------            | --------------                          | -------------                                                                              |
    /// | `Errors::NOT_PUBLISHED`     | `SlidingNonce::ESLIDING_NONCE`          | A `SlidingNonce` resource is not published under `dr_account`.                             |
    /// | `Errors::INVALID_ARGUMENT`  | `SlidingNonce::ENONCE_TOO_OLD`          | The `sliding_nonce` is too old and it's impossible to determine if it's duplicated or not. |
    /// | `Errors::INVALID_ARGUMENT`  | `SlidingNonce::ENONCE_TOO_NEW`          | The `sliding_nonce` is too far in the future.                                              |
    /// | `Errors::INVALID_ARGUMENT`  | `SlidingNonce::ENONCE_ALREADY_RECORDED` | The `sliding_nonce` has been previously recorded.                                          |
    /// | `Errors::REQUIRES_ADDRESS`  | `CoreAddresses::EDIEM_ROOT`            | The sending account is not the Diem Root account.                                         |
    /// | `Errors::REQUIRES_ROLE`     | `Roles::EDIEM_ROOT`                    | The sending account is not the Diem Root account.                                         |
    /// | `Errors::ALREADY_PUBLISHED` | `Roles::EROLE_ID`                       | The `new_account_address` address is already taken.                                        |
    ///
    /// # Related Scripts
    /// * `AccountCreationScripts::create_validator_account`
    /// * `ValidatorAdministrationScripts::add_validator_and_reconfigure`
    /// * `ValidatorAdministrationScripts::register_validator_config`
    /// * `ValidatorAdministrationScripts::remove_validator_and_reconfigure`
    /// * `ValidatorAdministrationScripts::set_validator_operator`
    /// * `ValidatorAdministrationScripts::set_validator_operator_with_nonce_admin`
    /// * `ValidatorAdministrationScripts::set_validator_config_and_reconfigure`

    public entry fun create_validator_operator_account(
        dr_account: signer,
        new_account_address: address,
        auth_key_prefix: vector<u8>,
        human_name: vector<u8>
    ) {
        ExperimentalAccount::create_validator_operator_account(
            &dr_account,
            new_account_address,
            auth_key_prefix,
            human_name,
        );
    }

    /// # Summary
    /// Creates a Validator account. This transaction can only be sent by the Diem
    /// Root account.
    ///
    /// # Technical Description
    /// Creates an account with a Validator role at `new_account_address`, with authentication key
    /// `auth_key_prefix` | `new_account_address`. It publishes a
    /// `ValidatorConfig::ValidatorConfig` resource with empty `config`, and
    /// `operator_account` fields. The `human_name` field of the
    /// `ValidatorConfig::ValidatorConfig` is set to the passed in `human_name`.
    /// This script does not add the validator to the validator set or the system,
    /// but only creates the account.
    ///
    /// # Events
    /// Successful execution will emit:
    /// * A `ExperimentalAccount::CreateAccountEvent` with the `created` field being `new_account_address`,
    /// and the `rold_id` field being `Roles::VALIDATOR_ROLE_ID`. This is emitted on the
    /// `ExperimentalAccount::AccountOperationsCapability` `creation_events` handle.
    ///
    /// # Parameters
    /// | Name                  | Type         | Description                                                                              |
    /// | ------                | ------       | -------------                                                                            |
    /// | `dr_account`          | `signer`     | The signer of the sending account of this transaction. Must be the Diem Root signer.     |
    /// | `sliding_nonce`       | `u64`        | The `sliding_nonce` (see: `SlidingNonce`) to be used for this transaction.               |
    /// | `new_account_address` | `address`    | Address of the to-be-created Validator account.                                          |
    /// | `auth_key_prefix`     | `vector<u8>` | The authentication key prefix that will be used initially for the newly created account. |
    /// | `human_name`          | `vector<u8>` | ASCII-encoded human name for the validator.                                              |
    ///
    /// # Common Abort Conditions
    /// | Error Category              | Error Reason                            | Description                                                                                |
    /// | ----------------            | --------------                          | -------------                                                                              |
    /// | `Errors::NOT_PUBLISHED`     | `SlidingNonce::ESLIDING_NONCE`          | A `SlidingNonce` resource is not published under `dr_account`.                             |
    /// | `Errors::INVALID_ARGUMENT`  | `SlidingNonce::ENONCE_TOO_OLD`          | The `sliding_nonce` is too old and it's impossible to determine if it's duplicated or not. |
    /// | `Errors::INVALID_ARGUMENT`  | `SlidingNonce::ENONCE_TOO_NEW`          | The `sliding_nonce` is too far in the future.                                              |
    /// | `Errors::INVALID_ARGUMENT`  | `SlidingNonce::ENONCE_ALREADY_RECORDED` | The `sliding_nonce` has been previously recorded.                                          |
    /// | `Errors::REQUIRES_ADDRESS`  | `CoreAddresses::EDIEM_ROOT`            | The sending account is not the Diem Root account.                                         |
    /// | `Errors::REQUIRES_ROLE`     | `Roles::EDIEM_ROOT`                    | The sending account is not the Diem Root account.                                         |
    /// | `Errors::ALREADY_PUBLISHED` | `Roles::EROLE_ID`                       | The `new_account_address` address is already taken.                                        |
    ///
    /// # Related Scripts
    /// * `AccountCreationScripts::create_validator_operator_account`
    /// * `ValidatorAdministrationScripts::add_validator_and_reconfigure`
    /// * `ValidatorAdministrationScripts::register_validator_config`
    /// * `ValidatorAdministrationScripts::remove_validator_and_reconfigure`
    /// * `ValidatorAdministrationScripts::set_validator_operator`
    /// * `ValidatorAdministrationScripts::set_validator_operator_with_nonce_admin`
    /// * `ValidatorAdministrationScripts::set_validator_config_and_reconfigure`

    public entry fun create_validator_account(
        dr_account: signer,
        new_account_address: address,
        auth_key_prefix: vector<u8>,
        human_name: vector<u8>,
    ) {
        ExperimentalAccount::create_validator_account(
            &dr_account,
            new_account_address,
            auth_key_prefix,
            human_name,
        );
      }

    ////////////////////////////////////////////////////////////////
    // Basic account creation script.
    // No roles attached, no conditions checked.
    ////////////////////////////////////////////////////////////////
    public entry fun create_account(
        _account: signer,
        new_account_address: address,
        auth_key_prefix: vector<u8>,
    ) {
        ExperimentalAccount::create_account(
            new_account_address,
            auth_key_prefix,
        );
    }
}
