// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};

/// The minimum and maximum protocol versions supported by this build.
const MIN_PROTOCOL_VERSION: u64 = 1;
const MAX_PROTOCOL_VERSION: u64 = 4;

// Record history of protocol version allocations here:
//
// Version 1: Original version.
// Version 2: Framework changes, including advancing epoch_start_time in safemode.
// Version 3: gas model v2, including all sui conservation fixes. Fix for loaded child object
//            changes, enable package upgrades, add limits on `max_size_written_objects`,
//            `max_size_written_objects_system_tx`
// Version 4: New reward slashing rate. Framework changes to skip stake susbidy when the epoch
//            length is short.

#[derive(Copy, Clone, Debug, Hash, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProtocolVersion(u64);

impl ProtocolVersion {
    // The minimum and maximum protocol version supported by this binary. Counterintuitively, this constant may
    // change over time as support for old protocol versions is removed from the source. This
    // ensures that when a new network (such as a testnet) is created, its genesis committee will
    // use a protocol version that is actually supported by the binary.
    pub const MIN: Self = Self(MIN_PROTOCOL_VERSION);

    pub const MAX: Self = Self(MAX_PROTOCOL_VERSION);

    #[cfg(not(msim))]
    const MAX_ALLOWED: Self = Self::MAX;

    // We create one additional "fake" version in simulator builds so that we can test upgrades.
    #[cfg(msim)]
    pub const MAX_ALLOWED: Self = Self(MAX_PROTOCOL_VERSION + 1);

    pub fn new(v: u64) -> Self {
        Self(v)
    }

    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    // For serde deserialization - we don't define a Default impl because there isn't a single
    // universally appropriate default value.
    pub fn max() -> Self {
        Self::MAX
    }
}

impl From<u64> for ProtocolVersion {
    fn from(v: u64) -> Self {
        Self::new(v)
    }
}

impl std::ops::Sub<u64> for ProtocolVersion {
    type Output = Self;
    fn sub(self, rhs: u64) -> Self::Output {
        Self::new(self.0 - rhs)
    }
}

impl std::ops::Add<u64> for ProtocolVersion {
    type Output = Self;
    fn add(self, rhs: u64) -> Self::Output {
        Self::new(self.0 + rhs)
    }
}

/// Models the set of protocol versions supported by a validator.
/// The `sui-node` binary will always use the SYSTEM_DEFAULT constant, but for testing we need
/// to be able to inject arbitrary versions into SuiNode.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct SupportedProtocolVersions {
    pub min: ProtocolVersion,
    pub max: ProtocolVersion,
}

impl SupportedProtocolVersions {
    pub const SYSTEM_DEFAULT: Self = Self {
        min: ProtocolVersion::MIN,
        max: ProtocolVersion::MAX,
    };

    /// Use by VersionedProtocolMessage implementors to describe in which range of versions a
    /// message variant is supported.
    pub fn new_for_message(min: u64, max: u64) -> Self {
        let min = ProtocolVersion::new(min);
        let max = ProtocolVersion::new(max);
        Self { min, max }
    }

    pub fn new_for_testing(min: u64, max: u64) -> Self {
        let min = min.into();
        let max = max.into();
        Self { min, max }
    }

    pub fn is_version_supported(&self, v: ProtocolVersion) -> bool {
        v.0 >= self.min.0 && v.0 <= self.max.0
    }
}

pub struct Error(pub String);

/// Records on/off feature flags that may vary at each protocol version.
#[derive(Default, Clone, Serialize, Debug)]
struct FeatureFlags {
    // Add feature flags here, e.g.:
    // new_protocol_feature: bool,
    #[serde(skip_serializing_if = "is_false")]
    package_upgrades: bool,
    // If true, validators will commit to the root state digest
    // in end of epoch checkpoint proposals
    #[serde(skip_serializing_if = "is_false")]
    commit_root_state_digest: bool,
    // Pass epoch start time to advance_epoch safe mode function.
    #[serde(skip_serializing_if = "is_false")]
    advance_epoch_start_time_in_safe_mode: bool,
    // If true, apply the fix to correctly capturing loaded child object versions in execution's
    // object runtime.
    #[serde(skip_serializing_if = "is_false")]
    loaded_child_objects_fixed: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

/// Constants that change the behavior of the protocol.
///
/// The value of each constant here must be fixed for a given protocol version. To change the value
/// of a constant, advance the protocol version, and add support for it in `get_for_version` under
/// the new version number.
/// (below).
///
/// To add a new field to this struct, use the following procedure:
/// - Advance the protocol version.
/// - Add the field as a private `Option<T>` to the struct.
/// - Initialize the field to `None` in prior protocol versions.
/// - Initialize the field to `Some(val)` for your new protocol version.
/// - Add a public getter that simply unwraps the field.
///
/// This way, if the constant is accessed in a protocol version in which it is not defined, the
/// validator will crash. (Crashing is necessary because this type of error would almost always
/// result in forking if not prevented here).
#[skip_serializing_none]
#[derive(Clone, Serialize, Debug)]
pub struct ProtocolConfig {
    pub version: ProtocolVersion,

    feature_flags: FeatureFlags,

    // ==== Transaction input limits ====
    /// Maximum serialized size of a transaction (in bytes).
    // NOTE: This value should be kept in sync with the corresponding value in
    // sdk/typescript/src/builder/TransactionData.ts
    max_tx_size_bytes: Option<u64>,

    /// Maximum number of input objects to a transaction. Enforced by the transaction input checker
    max_input_objects: Option<u64>,

    /// Max size of objects a transaction can write to disk after completion. Enforce by the Sui adapter.
    max_size_written_objects: Option<u64>,
    /// Max size of objects a system transaction can write to disk after completion. Enforce by the Sui adapter.
    max_size_written_objects_system_tx: Option<u64>,

    /// Maximum size of serialized transaction effects.
    max_serialized_tx_effects_size_bytes: Option<u64>,

    /// Maximum size of serialized transaction effects for system transactions.
    max_serialized_tx_effects_size_bytes_system_tx: Option<u64>,

    /// Maximum number of gas payment objects for a transaction.
    max_gas_payment_objects: Option<u32>,

    /// Maximum number of modules in a Publish transaction.
    max_modules_in_publish: Option<u32>,

    /// Maximum number of arguments in a move call or a ProgrammableTransaction's
    /// TransferObjects command.
    max_arguments: Option<u32>,

    /// Maximum number of total type arguments, computed recursively.
    max_type_arguments: Option<u32>,

    /// Maximum depth of an individual type argument.
    max_type_argument_depth: Option<u32>,

    /// Maximum size of a Pure CallArg.
    max_pure_argument_size: Option<u32>,

    /// Maximum number of Commands in a ProgrammableTransaction.
    max_programmable_tx_commands: Option<u32>,

    // ==== Move VM, Move bytecode verifier, and execution limits ===
    /// Maximum Move bytecode version the VM understands. All older versions are accepted.
    move_binary_format_version: Option<u32>,

    /// Maximum size of the `contents` part of an object, in bytes. Enforced by the Sui adapter when effects are produced.
    max_move_object_size: Option<u64>,

    // TODO: Option<increase to 500 KB. currently, publishing a package > 500 KB exceeds the max computation gas cost
    /// Maximum size of a Move package object, in bytes. Enforced by the Sui adapter at the end of a publish transaction.
    max_move_package_size: Option<u64>,

    /// Maximum number of gas units that a single MoveCall transaction can use. Enforced by the Sui adapter.
    max_tx_gas: Option<u64>,

    /// Maximum number of nested loops. Enforced by the Move bytecode verifier.
    max_loop_depth: Option<u64>,

    /// Maximum number of type arguments that can be bound to generic type parameters. Enforced by the Move bytecode verifier.
    max_generic_instantiation_length: Option<u64>,

    /// Maximum number of parameters that a Move function can have. Enforced by the Move bytecode verifier.
    max_function_parameters: Option<u64>,

    /// Maximum number of basic blocks that a Move function can have. Enforced by the Move bytecode verifier.
    max_basic_blocks: Option<u64>,

    /// Maximum stack size value. Enforced by the Move bytecode verifier.
    max_value_stack_size: Option<u64>,

    /// Maximum number of "type nodes", a metric for how big a SignatureToken will be when expanded into a fully qualified type. Enforced by the Move bytecode verifier.
    max_type_nodes: Option<u64>,

    /// Maximum number of push instructions in one function. Enforced by the Move bytecode verifier.
    max_push_size: Option<u64>,

    /// Maximum number of struct definitions in a module. Enforced by the Move bytecode verifier.
    max_struct_definitions: Option<u64>,

    /// Maximum number of function definitions in a module. Enforced by the Move bytecode verifier.
    max_function_definitions: Option<u64>,

    /// Maximum number of fields allowed in a struct definition. Enforced by the Move bytecode verifier.
    max_fields_in_struct: Option<u64>,

    /// Maximum dependency depth. Enforced by the Move linker when loading dependent modules.
    max_dependency_depth: Option<u64>,

    /// Maximum number of Move events that a single transaction can emit. Enforced by the VM during execution.
    max_num_event_emit: Option<u64>,

    /// Maximum number of new IDs that a single transaction can create. Enforced by the VM during execution.
    max_num_new_move_object_ids: Option<u64>,

    /// Maximum number of new IDs that a single system transaction can create. Enforced by the VM during execution.
    max_num_new_move_object_ids_system_tx: Option<u64>,

    /// Maximum number of IDs that a single transaction can delete. Enforced by the VM during execution.
    max_num_deleted_move_object_ids: Option<u64>,

    /// Maximum number of IDs that a single system transaction can delete. Enforced by the VM during execution.
    max_num_deleted_move_object_ids_system_tx: Option<u64>,

    /// Maximum number of IDs that a single transaction can transfer. Enforced by the VM during execution.
    max_num_transferred_move_object_ids: Option<u64>,

    /// Maximum number of IDs that a single system transaction can transfer. Enforced by the VM during execution.
    max_num_transferred_move_object_ids_system_tx: Option<u64>,

    /// Maximum size of a Move user event. Enforced by the VM during execution.
    max_event_emit_size: Option<u64>,

    /// Maximum length of a vector in Move. Enforced by the VM during execution, and for constants, by the verifier.
    max_move_vector_len: Option<u64>,

    /// Maximum number of back edges in Move function. Enforced by the bytecode verifier at signing.
    max_back_edges_per_function: Option<u64>,

    /// Maximum number of back edges in Move module. Enforced by the bytecode verifier at signing.
    max_back_edges_per_module: Option<u64>,

    /// Maximum number of meter `ticks` spent verifying a Move function. Enforced by the bytecode verifier at signing.
    max_verifier_meter_ticks_per_function: Option<u64>,

    /// Maximum number of meter `ticks` spent verifying a Move function. Enforced by the bytecode verifier at signing.
    max_meter_ticks_per_module: Option<u64>,

    // === Object runtime internal operation limits ====
    // These affect dynamic fields
    /// Maximum number of cached objects in the object runtime ObjectStore. Enforced by object runtime during execution
    object_runtime_max_num_cached_objects: Option<u64>,

    /// Maximum number of cached objects in the object runtime ObjectStore in system transaction. Enforced by object runtime during execution
    object_runtime_max_num_cached_objects_system_tx: Option<u64>,

    /// Maximum number of stored objects accessed by object runtime ObjectStore. Enforced by object runtime during execution
    object_runtime_max_num_store_entries: Option<u64>,

    /// Maximum number of stored objects accessed by object runtime ObjectStore in system transaction. Enforced by object runtime during execution
    object_runtime_max_num_store_entries_system_tx: Option<u64>,

    // === Execution gas costs ====
    // note: Option<per-instruction and native function gas costs live in the sui-cost-tables crate
    /// Base cost for any Sui transaction
    base_tx_cost_fixed: Option<u64>,

    /// Additional cost for a transaction that publishes a package
    /// i.e., the base cost of such a transaction is base_tx_cost_fixed + package_publish_cost_fixed
    package_publish_cost_fixed: Option<u64>,

    /// Cost per byte of a Move call transaction
    /// i.e., the cost of such a transaction is base_cost + (base_tx_cost_per_byte * size)
    base_tx_cost_per_byte: Option<u64>,

    /// Cost per byte for a transaction that publishes a package
    package_publish_cost_per_byte: Option<u64>,

    // Per-byte cost of reading an object during transaction execution
    obj_access_cost_read_per_byte: Option<u64>,

    // Per-byte cost of writing an object during transaction execution
    obj_access_cost_mutate_per_byte: Option<u64>,

    // Per-byte cost of deleting an object during transaction execution
    obj_access_cost_delete_per_byte: Option<u64>,

    /// Per-byte cost charged for each input object to a transaction.
    /// Meant to approximate the cost of checking locks for each object
    // TODO: Option<I'm not sure that this cost makes sense. Checking locks is "free"
    // in the sense that an invalid tx that can never be committed/pay gas can
    // force validators to check an arbitrary number of locks. If those checks are
    // "free" for invalid transactions, why charge for them in valid transactions
    // TODO: Option<if we keep this, I think we probably want it to be a fixed cost rather
    // than a per-byte cost. checking an object lock should not require loading an
    // entire object, just consulting an ID -> tx digest map
    obj_access_cost_verify_per_byte: Option<u64>,

    /// === Gas version. gas model ===

    /// Gas model version, what code we are using to charge gas
    gas_model_version: Option<u64>,

    /// === Storage gas costs ===

    /// Per-byte cost of storing an object in the Sui global object store. Some of this cost may be refundable if the object is later freed
    obj_data_cost_refundable: Option<u64>,

    // Per-byte cost of storing an object in the Sui transaction log (e.g., in CertifiedTransactionEffects)
    // This depends on the size of various fields including the effects
    // TODO: Option<I don't fully understand this^ and more details would be useful
    obj_metadata_cost_non_refundable: Option<u64>,

    /// === Tokenomics ===

    // TODO: Option<this should be changed to u64.
    /// Sender of a txn that touches an object will get this percent of the storage rebate back.
    /// In basis point.
    storage_rebate_rate: Option<u64>,

    /// 5% of the storage fund's share of rewards are reinvested into the storage fund.
    /// In basis point.
    storage_fund_reinvest_rate: Option<u64>,

    /// The share of rewards that will be slashed and redistributed is 50%.
    /// In basis point.
    reward_slashing_rate: Option<u64>,

    /// Unit gas price, Mist per internal gas unit.
    storage_gas_price: Option<u64>,

    /// === Core Protocol ===

    /// Max number of transactions per checkpoint.
    /// Note that this is a protocol constant and not a config as validators must have this set to
    /// the same value, otherwise they *will* fork.
    max_transactions_per_checkpoint: Option<u64>,

    /// Max size of a checkpoint in bytes.
    /// Note that this is a protocol constant and not a config as validators must have this set to
    /// the same value, otherwise they *will* fork.
    max_checkpoint_size_bytes: Option<u64>,

    /// A protocol upgrade always requires 2f+1 stake to agree. We support a buffer of additional
    /// stake (as a fraction of f, expressed in basis points) that is required before an upgrade
    /// can happen automatically. 10000bps would indicate that complete unanimity is required (all
    /// 3f+1 must vote), while 0bps would indicate that 2f+1 is sufficient.
    buffer_stake_for_protocol_upgrade_bps: Option<u64>,

    // === Native Function Costs ===

    // `address` module
    // Cost params for the Move native function `address::from_bytes(bytes: vector<u8>)`
    address_from_bytes_cost_base: Option<u64>,
    // Cost params for the Move native function `address::to_u256(address): u256`
    address_to_u256_cost_base: Option<u64>,
    // Cost params for the Move native function `address::from_u256(u256): address`
    address_from_u256_cost_base: Option<u64>,

    // `dynamic_field` module
    // Cost params for the Move native function `hash_type_and_key<K: copy + drop + store>(parent: address, k: K): address`
    dynamic_field_hash_type_and_key_cost_base: Option<u64>,
    dynamic_field_hash_type_and_key_type_cost_per_byte: Option<u64>,
    dynamic_field_hash_type_and_key_value_cost_per_byte: Option<u64>,
    dynamic_field_hash_type_and_key_type_tag_cost_per_byte: Option<u64>,
    // Cost params for the Move native function `add_child_object<Child: key>(parent: address, child: Child)`
    dynamic_field_add_child_object_cost_base: Option<u64>,
    dynamic_field_add_child_object_type_cost_per_byte: Option<u64>,
    dynamic_field_add_child_object_value_cost_per_byte: Option<u64>,
    dynamic_field_add_child_object_struct_tag_cost_per_byte: Option<u64>,
    // Cost params for the Move native function `borrow_child_object_mut<Child: key>(parent: &mut UID, id: address): &mut Child`
    dynamic_field_borrow_child_object_cost_base: Option<u64>,
    dynamic_field_borrow_child_object_child_ref_cost_per_byte: Option<u64>,
    dynamic_field_borrow_child_object_type_cost_per_byte: Option<u64>,
    // Cost params for the Move native function `remove_child_object<Child: key>(parent: address, id: address): Child`
    dynamic_field_remove_child_object_cost_base: Option<u64>,
    dynamic_field_remove_child_object_child_cost_per_byte: Option<u64>,
    dynamic_field_remove_child_object_type_cost_per_byte: Option<u64>,
    // Cost params for the Move native function `has_child_object(parent: address, id: address): bool`
    dynamic_field_has_child_object_cost_base: Option<u64>,
    // Cost params for the Move native function `has_child_object_with_ty<Child: key>(parent: address, id: address): bool`
    dynamic_field_has_child_object_with_ty_cost_base: Option<u64>,
    dynamic_field_has_child_object_with_ty_type_cost_per_byte: Option<u64>,
    dynamic_field_has_child_object_with_ty_type_tag_cost_per_byte: Option<u64>,

    // `event` module
    // Cost params for the Move native function `event::emit<T: copy + drop>(event: T)`
    event_emit_cost_base: Option<u64>,
    event_emit_value_size_derivation_cost_per_byte: Option<u64>,
    event_emit_tag_size_derivation_cost_per_byte: Option<u64>,
    event_emit_output_cost_per_byte: Option<u64>,

    //  `object` module
    // Cost params for the Move native function `borrow_uid<T: key>(obj: &T): &UID`
    object_borrow_uid_cost_base: Option<u64>,
    // Cost params for the Move native function `delete_impl(id: address)`
    object_delete_impl_cost_base: Option<u64>,
    // Cost params for the Move native function `record_new_uid(id: address)`
    object_record_new_uid_cost_base: Option<u64>,

    // Transfer
    // Cost params for the Move native function `transfer_impl<T: key>(obj: T, recipient: address)`
    transfer_transfer_internal_cost_base: Option<u64>,
    // Cost params for the Move native function `freeze_object<T: key>(obj: T)`
    transfer_freeze_object_cost_base: Option<u64>,
    // Cost params for the Move native function `share_object<T: key>(obj: T)`
    transfer_share_object_cost_base: Option<u64>,

    // TxContext
    // Cost params for the Move native function `transfer_impl<T: key>(obj: T, recipient: address)`
    tx_context_derive_id_cost_base: Option<u64>,

    // Types
    // Cost params for the Move native function `is_one_time_witness<T: drop>(_: &T): bool`
    types_is_one_time_witness_cost_base: Option<u64>,
    types_is_one_time_witness_type_tag_cost_per_byte: Option<u64>,
    types_is_one_time_witness_type_cost_per_byte: Option<u64>,

    // Validator
    // Cost params for the Move native function `validate_metadata_bcs(metadata: vector<u8>)`
    validator_validate_metadata_cost_base: Option<u64>,
    validator_validate_metadata_data_cost_per_byte: Option<u64>,

    // Crypto natives
    crypto_invalid_arguments_cost: Option<u64>,
    // bls12381::bls12381_min_sig_verify
    bls12381_bls12381_min_sig_verify_cost_base: Option<u64>,
    bls12381_bls12381_min_sig_verify_msg_cost_per_byte: Option<u64>,
    bls12381_bls12381_min_sig_verify_msg_cost_per_block: Option<u64>,

    // bls12381::bls12381_min_pk_verify
    bls12381_bls12381_min_pk_verify_cost_base: Option<u64>,
    bls12381_bls12381_min_pk_verify_msg_cost_per_byte: Option<u64>,
    bls12381_bls12381_min_pk_verify_msg_cost_per_block: Option<u64>,

    // ecdsa_k1::ecrecover
    ecdsa_k1_ecrecover_keccak256_cost_base: Option<u64>,
    ecdsa_k1_ecrecover_keccak256_msg_cost_per_byte: Option<u64>,
    ecdsa_k1_ecrecover_keccak256_msg_cost_per_block: Option<u64>,
    ecdsa_k1_ecrecover_sha256_cost_base: Option<u64>,
    ecdsa_k1_ecrecover_sha256_msg_cost_per_byte: Option<u64>,
    ecdsa_k1_ecrecover_sha256_msg_cost_per_block: Option<u64>,

    // ecdsa_k1::decompress_pubkey
    ecdsa_k1_decompress_pubkey_cost_base: Option<u64>,

    // ecdsa_k1::secp256k1_verify
    ecdsa_k1_secp256k1_verify_keccak256_cost_base: Option<u64>,
    ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_byte: Option<u64>,
    ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_block: Option<u64>,
    ecdsa_k1_secp256k1_verify_sha256_cost_base: Option<u64>,
    ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_byte: Option<u64>,
    ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_block: Option<u64>,

    // ecdsa_r1::ecrecover
    ecdsa_r1_ecrecover_keccak256_cost_base: Option<u64>,
    ecdsa_r1_ecrecover_keccak256_msg_cost_per_byte: Option<u64>,
    ecdsa_r1_ecrecover_keccak256_msg_cost_per_block: Option<u64>,
    ecdsa_r1_ecrecover_sha256_cost_base: Option<u64>,
    ecdsa_r1_ecrecover_sha256_msg_cost_per_byte: Option<u64>,
    ecdsa_r1_ecrecover_sha256_msg_cost_per_block: Option<u64>,

    // ecdsa_r1::secp256k1_verify
    ecdsa_r1_secp256r1_verify_keccak256_cost_base: Option<u64>,
    ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_byte: Option<u64>,
    ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_block: Option<u64>,
    ecdsa_r1_secp256r1_verify_sha256_cost_base: Option<u64>,
    ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_byte: Option<u64>,
    ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_block: Option<u64>,

    // ecvrf::verify
    ecvrf_ecvrf_verify_cost_base: Option<u64>,
    ecvrf_ecvrf_verify_alpha_string_cost_per_byte: Option<u64>,
    ecvrf_ecvrf_verify_alpha_string_cost_per_block: Option<u64>,

    // ed25519
    ed25519_ed25519_verify_cost_base: Option<u64>,
    ed25519_ed25519_verify_msg_cost_per_byte: Option<u64>,
    ed25519_ed25519_verify_msg_cost_per_block: Option<u64>,

    // groth16::prepare_verifying_key
    groth16_prepare_verifying_key_bls12381_cost_base: Option<u64>,
    groth16_prepare_verifying_key_bn254_cost_base: Option<u64>,

    // groth16::verify_groth16_proof_internal
    groth16_verify_groth16_proof_internal_bls12381_cost_base: Option<u64>,
    groth16_verify_groth16_proof_internal_bls12381_cost_per_public_input: Option<u64>,
    groth16_verify_groth16_proof_internal_bn254_cost_base: Option<u64>,
    groth16_verify_groth16_proof_internal_bn254_cost_per_public_input: Option<u64>,
    groth16_verify_groth16_proof_internal_public_input_cost_per_byte: Option<u64>,

    // hash::blake2b256
    hash_blake2b256_cost_base: Option<u64>,
    hash_blake2b256_data_cost_per_byte: Option<u64>,
    hash_blake2b256_data_cost_per_block: Option<u64>,
    // hash::keccak256
    hash_keccak256_cost_base: Option<u64>,
    hash_keccak256_data_cost_per_byte: Option<u64>,
    hash_keccak256_data_cost_per_block: Option<u64>,

    // hmac::hmac_sha3_256
    hmac_hmac_sha3_256_cost_base: Option<u64>,
    hmac_hmac_sha3_256_input_cost_per_byte: Option<u64>,
    hmac_hmac_sha3_256_input_cost_per_block: Option<u64>,
}

const CONSTANT_ERR_MSG: &str = "protocol constant not present in current protocol version";

// feature flags
impl ProtocolConfig {
    // Add checks for feature flag support here, e.g.:
    // pub fn check_new_protocol_feature_supported(&self) -> Result<(), Error> {
    //     if self.feature_flags.new_protocol_feature_supported {
    //         Ok(())
    //     } else {
    //         Err(Error(format!(
    //             "new_protocol_feature is not supported at {:?}",
    //             self.version
    //         )))
    //     }
    // }

    pub fn check_package_upgrades_supported(&self) -> Result<(), Error> {
        if self.feature_flags.package_upgrades {
            Ok(())
        } else {
            Err(Error(format!(
                "package upgrades are not supported at {:?}",
                self.version
            )))
        }
    }

    pub fn package_upgrades_supported(&self) -> bool {
        self.feature_flags.package_upgrades
    }

    pub fn check_commit_root_state_digest_supported(&self) -> bool {
        self.feature_flags.commit_root_state_digest
    }

    pub fn get_advance_epoch_start_time_in_safe_mode(&self) -> bool {
        self.feature_flags.advance_epoch_start_time_in_safe_mode
    }

    pub fn loaded_child_objects_fixed(&self) -> bool {
        self.feature_flags.loaded_child_objects_fixed
    }
}

// getters
impl ProtocolConfig {
    pub fn max_tx_size_bytes(&self) -> u64 {
        self.max_tx_size_bytes.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_input_objects(&self) -> u64 {
        self.max_input_objects.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_serialized_tx_effects_size_bytes(&self) -> u64 {
        self.max_serialized_tx_effects_size_bytes
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn max_serialized_tx_effects_size_bytes_system_tx(&self) -> u64 {
        self.max_serialized_tx_effects_size_bytes_system_tx
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn max_gas_payment_objects(&self) -> u32 {
        self.max_gas_payment_objects.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_modules_in_publish(&self) -> u32 {
        self.max_modules_in_publish.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_arguments(&self) -> u32 {
        self.max_arguments.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_type_arguments(&self) -> u32 {
        self.max_type_arguments.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_type_argument_depth(&self) -> u32 {
        self.max_type_argument_depth.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_pure_argument_size(&self) -> u32 {
        self.max_pure_argument_size.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_programmable_tx_commands(&self) -> u32 {
        self.max_programmable_tx_commands.expect(CONSTANT_ERR_MSG)
    }
    pub fn move_binary_format_version(&self) -> u32 {
        self.move_binary_format_version.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_move_object_size(&self) -> u64 {
        self.max_move_object_size.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_move_package_size(&self) -> u64 {
        self.max_move_package_size.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_tx_gas(&self) -> u64 {
        self.max_tx_gas.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_loop_depth(&self) -> u64 {
        self.max_loop_depth.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_generic_instantiation_length(&self) -> u64 {
        self.max_generic_instantiation_length
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn max_function_parameters(&self) -> u64 {
        self.max_function_parameters.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_basic_blocks(&self) -> u64 {
        self.max_basic_blocks.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_value_stack_size(&self) -> u64 {
        self.max_value_stack_size.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_type_nodes(&self) -> u64 {
        self.max_type_nodes.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_push_size(&self) -> u64 {
        self.max_push_size.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_struct_definitions(&self) -> u64 {
        self.max_struct_definitions.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_function_definitions(&self) -> u64 {
        self.max_function_definitions.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_fields_in_struct(&self) -> u64 {
        self.max_fields_in_struct.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_dependency_depth(&self) -> u64 {
        self.max_dependency_depth.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_num_event_emit(&self) -> u64 {
        self.max_num_event_emit.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_num_new_move_object_ids(&self) -> u64 {
        self.max_num_new_move_object_ids.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_num_new_move_object_ids_system_tx(&self) -> u64 {
        self.max_num_new_move_object_ids_system_tx
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn max_num_deleted_move_object_ids(&self) -> u64 {
        self.max_num_deleted_move_object_ids
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn max_num_deleted_move_object_ids_system_tx(&self) -> u64 {
        self.max_num_deleted_move_object_ids_system_tx
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn max_num_transferred_move_object_ids(&self) -> u64 {
        self.max_num_transferred_move_object_ids
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn max_num_transferred_move_object_ids_system_tx(&self) -> u64 {
        self.max_num_transferred_move_object_ids_system_tx
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn max_event_emit_size(&self) -> u64 {
        self.max_event_emit_size.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_move_vector_len(&self) -> u64 {
        self.max_move_vector_len.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_back_edges_per_function(&self) -> u64 {
        self.max_back_edges_per_function.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_back_edges_per_module(&self) -> u64 {
        self.max_back_edges_per_module.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_verifier_meter_ticks_per_function(&self) -> u64 {
        self.max_verifier_meter_ticks_per_function
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn max_meter_ticks_per_module(&self) -> u64 {
        self.max_meter_ticks_per_module.expect(CONSTANT_ERR_MSG)
    }
    pub fn object_runtime_max_num_cached_objects(&self) -> u64 {
        self.object_runtime_max_num_cached_objects
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn object_runtime_max_num_store_entries(&self) -> u64 {
        self.object_runtime_max_num_store_entries
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn object_runtime_max_num_cached_objects_system_tx(&self) -> u64 {
        self.object_runtime_max_num_cached_objects_system_tx
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn object_runtime_max_num_store_entries_system_tx(&self) -> u64 {
        self.object_runtime_max_num_store_entries_system_tx
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn base_tx_cost_fixed(&self) -> u64 {
        self.base_tx_cost_fixed.expect(CONSTANT_ERR_MSG)
    }
    pub fn package_publish_cost_fixed(&self) -> u64 {
        self.package_publish_cost_fixed.expect(CONSTANT_ERR_MSG)
    }
    pub fn base_tx_cost_per_byte(&self) -> u64 {
        self.base_tx_cost_per_byte.expect(CONSTANT_ERR_MSG)
    }
    pub fn package_publish_cost_per_byte(&self) -> u64 {
        self.package_publish_cost_per_byte.expect(CONSTANT_ERR_MSG)
    }
    pub fn obj_access_cost_read_per_byte(&self) -> u64 {
        self.obj_access_cost_read_per_byte.expect(CONSTANT_ERR_MSG)
    }
    pub fn obj_access_cost_mutate_per_byte(&self) -> u64 {
        self.obj_access_cost_mutate_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn obj_access_cost_delete_per_byte(&self) -> u64 {
        self.obj_access_cost_delete_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn obj_access_cost_verify_per_byte(&self) -> u64 {
        self.obj_access_cost_verify_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn obj_data_cost_refundable(&self) -> u64 {
        self.obj_data_cost_refundable.expect(CONSTANT_ERR_MSG)
    }
    pub fn obj_metadata_cost_non_refundable(&self) -> u64 {
        self.obj_metadata_cost_non_refundable
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn gas_model_version(&self) -> u64 {
        self.gas_model_version.expect(CONSTANT_ERR_MSG)
    }
    pub fn storage_rebate_rate(&self) -> u64 {
        self.storage_rebate_rate.expect(CONSTANT_ERR_MSG)
    }
    pub fn storage_fund_reinvest_rate(&self) -> u64 {
        self.storage_fund_reinvest_rate.expect(CONSTANT_ERR_MSG)
    }
    pub fn reward_slashing_rate(&self) -> u64 {
        self.reward_slashing_rate.expect(CONSTANT_ERR_MSG)
    }
    pub fn storage_gas_price(&self) -> u64 {
        self.storage_gas_price.expect(CONSTANT_ERR_MSG)
    }
    pub fn max_transactions_per_checkpoint(&self) -> u64 {
        self.max_transactions_per_checkpoint
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn max_checkpoint_size_bytes(&self) -> u64 {
        self.max_checkpoint_size_bytes.expect(CONSTANT_ERR_MSG)
    }
    pub fn buffer_stake_for_protocol_upgrade_bps(&self) -> u64 {
        self.buffer_stake_for_protocol_upgrade_bps
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn address_from_bytes_cost_base(&self) -> u64 {
        self.address_from_bytes_cost_base.expect(CONSTANT_ERR_MSG)
    }
    pub fn address_to_u256_cost_base(&self) -> u64 {
        self.address_to_u256_cost_base.expect(CONSTANT_ERR_MSG)
    }
    pub fn address_from_u256_cost_base(&self) -> u64 {
        self.address_from_u256_cost_base.expect(CONSTANT_ERR_MSG)
    }

    pub fn dynamic_field_hash_type_and_key_cost_base(&self) -> u64 {
        self.dynamic_field_hash_type_and_key_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_hash_type_and_key_type_cost_per_byte(&self) -> u64 {
        self.dynamic_field_hash_type_and_key_type_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_hash_type_and_key_value_cost_per_byte(&self) -> u64 {
        self.dynamic_field_hash_type_and_key_value_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_hash_type_and_key_type_tag_cost_per_byte(&self) -> u64 {
        self.dynamic_field_hash_type_and_key_type_tag_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn dynamic_field_add_child_object_cost_base(&self) -> u64 {
        self.dynamic_field_add_child_object_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_add_child_object_type_cost_per_byte(&self) -> u64 {
        self.dynamic_field_add_child_object_type_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_add_child_object_value_cost_per_byte(&self) -> u64 {
        self.dynamic_field_add_child_object_value_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_add_child_object_struct_tag_cost_per_byte(&self) -> u64 {
        self.dynamic_field_add_child_object_struct_tag_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn dynamic_field_borrow_child_object_cost_base(&self) -> u64 {
        self.dynamic_field_borrow_child_object_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_borrow_child_object_child_ref_cost_per_byte(&self) -> u64 {
        self.dynamic_field_borrow_child_object_child_ref_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_borrow_child_object_type_cost_per_byte(&self) -> u64 {
        self.dynamic_field_borrow_child_object_type_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn dynamic_field_remove_child_object_cost_base(&self) -> u64 {
        self.dynamic_field_remove_child_object_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_remove_child_object_child_cost_per_byte(&self) -> u64 {
        self.dynamic_field_remove_child_object_child_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_remove_child_object_type_cost_per_byte(&self) -> u64 {
        self.dynamic_field_remove_child_object_type_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn dynamic_field_has_child_object_cost_base(&self) -> u64 {
        self.dynamic_field_has_child_object_cost_base
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn dynamic_field_has_child_object_with_ty_cost_base(&self) -> u64 {
        self.dynamic_field_has_child_object_with_ty_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_has_child_object_with_ty_type_cost_per_byte(&self) -> u64 {
        self.dynamic_field_has_child_object_with_ty_type_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn dynamic_field_has_child_object_with_ty_type_tag_cost_per_byte(&self) -> u64 {
        self.dynamic_field_has_child_object_with_ty_type_tag_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn event_emit_cost_base(&self) -> u64 {
        self.event_emit_cost_base.expect(CONSTANT_ERR_MSG)
    }
    pub fn event_emit_value_size_derivation_cost_per_byte(&self) -> u64 {
        self.event_emit_value_size_derivation_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn event_emit_tag_size_derivation_cost_per_byte(&self) -> u64 {
        self.event_emit_tag_size_derivation_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn event_emit_output_cost_per_byte(&self) -> u64 {
        self.event_emit_output_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn object_borrow_uid_cost_base(&self) -> u64 {
        self.object_borrow_uid_cost_base.expect(CONSTANT_ERR_MSG)
    }
    pub fn object_delete_impl_cost_base(&self) -> u64 {
        self.object_delete_impl_cost_base.expect(CONSTANT_ERR_MSG)
    }
    pub fn object_record_new_uid_cost_base(&self) -> u64 {
        self.object_record_new_uid_cost_base
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn transfer_transfer_internal_cost_base(&self) -> u64 {
        self.transfer_transfer_internal_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn transfer_freeze_object_cost_base(&self) -> u64 {
        self.transfer_freeze_object_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn transfer_share_object_cost_base(&self) -> u64 {
        self.transfer_share_object_cost_base
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn tx_context_derive_id_cost_base(&self) -> u64 {
        self.tx_context_derive_id_cost_base.expect(CONSTANT_ERR_MSG)
    }

    pub fn types_is_one_time_witness_cost_base(&self) -> u64 {
        self.types_is_one_time_witness_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn types_is_one_time_witness_type_tag_cost_per_byte(&self) -> u64 {
        self.types_is_one_time_witness_type_tag_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn types_is_one_time_witness_type_cost_per_byte(&self) -> u64 {
        self.types_is_one_time_witness_type_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn validator_validate_metadata_cost_base(&self) -> u64 {
        self.validator_validate_metadata_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn validator_validate_metadata_data_cost_per_byte(&self) -> u64 {
        self.validator_validate_metadata_data_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn crypto_invalid_arguments_cost(&self) -> u64 {
        self.crypto_invalid_arguments_cost.expect(CONSTANT_ERR_MSG)
    }
    pub fn bls12381_bls12381_min_sig_verify_cost_base(&self) -> u64 {
        self.bls12381_bls12381_min_sig_verify_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn bls12381_bls12381_min_sig_verify_msg_cost_per_byte(&self) -> u64 {
        self.bls12381_bls12381_min_sig_verify_msg_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn bls12381_bls12381_min_sig_verify_msg_cost_per_block(&self) -> u64 {
        self.bls12381_bls12381_min_sig_verify_msg_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn bls12381_bls12381_min_pk_verify_cost_base(&self) -> u64 {
        self.bls12381_bls12381_min_pk_verify_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn bls12381_bls12381_min_pk_verify_msg_cost_per_byte(&self) -> u64 {
        self.bls12381_bls12381_min_pk_verify_msg_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn bls12381_bls12381_min_pk_verify_msg_cost_per_block(&self) -> u64 {
        self.bls12381_bls12381_min_pk_verify_msg_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn ecdsa_k1_ecrecover_keccak256_cost_base(&self) -> u64 {
        self.ecdsa_k1_ecrecover_keccak256_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_k1_ecrecover_keccak256_msg_cost_per_byte(&self) -> u64 {
        self.ecdsa_k1_ecrecover_keccak256_msg_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_k1_ecrecover_keccak256_msg_cost_per_block(&self) -> u64 {
        self.ecdsa_k1_ecrecover_keccak256_msg_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_k1_ecrecover_sha256_cost_base(&self) -> u64 {
        self.ecdsa_k1_ecrecover_sha256_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_k1_ecrecover_sha256_msg_cost_per_byte(&self) -> u64 {
        self.ecdsa_k1_ecrecover_sha256_msg_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_k1_ecrecover_sha256_msg_cost_per_block(&self) -> u64 {
        self.ecdsa_k1_ecrecover_sha256_msg_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn ecdsa_k1_decompress_pubkey_cost_base(&self) -> u64 {
        self.ecdsa_k1_decompress_pubkey_cost_base
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn ecdsa_k1_secp256k1_verify_keccak256_cost_base(&self) -> u64 {
        self.ecdsa_k1_secp256k1_verify_keccak256_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_byte(&self) -> u64 {
        self.ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_block(&self) -> u64 {
        self.ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_k1_secp256k1_verify_sha256_cost_base(&self) -> u64 {
        self.ecdsa_k1_secp256k1_verify_sha256_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_byte(&self) -> u64 {
        self.ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_block(&self) -> u64 {
        self.ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn ecdsa_r1_ecrecover_keccak256_cost_base(&self) -> u64 {
        self.ecdsa_r1_ecrecover_keccak256_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_r1_ecrecover_keccak256_msg_cost_per_byte(&self) -> u64 {
        self.ecdsa_r1_ecrecover_keccak256_msg_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_r1_ecrecover_keccak256_msg_cost_per_block(&self) -> u64 {
        self.ecdsa_r1_ecrecover_keccak256_msg_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_r1_ecrecover_sha256_cost_base(&self) -> u64 {
        self.ecdsa_r1_ecrecover_sha256_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_r1_ecrecover_sha256_msg_cost_per_byte(&self) -> u64 {
        self.ecdsa_r1_ecrecover_sha256_msg_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_r1_ecrecover_sha256_msg_cost_per_block(&self) -> u64 {
        self.ecdsa_r1_ecrecover_sha256_msg_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn ecdsa_r1_secp256r1_verify_keccak256_cost_base(&self) -> u64 {
        self.ecdsa_r1_secp256r1_verify_keccak256_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_byte(&self) -> u64 {
        self.ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_block(&self) -> u64 {
        self.ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_r1_secp256r1_verify_sha256_cost_base(&self) -> u64 {
        self.ecdsa_r1_secp256r1_verify_sha256_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_byte(&self) -> u64 {
        self.ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_block(&self) -> u64 {
        self.ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn ecvrf_ecvrf_verify_cost_base(&self) -> u64 {
        self.ecvrf_ecvrf_verify_cost_base.expect(CONSTANT_ERR_MSG)
    }
    pub fn ecvrf_ecvrf_verify_alpha_string_cost_per_byte(&self) -> u64 {
        self.ecvrf_ecvrf_verify_alpha_string_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ecvrf_ecvrf_verify_alpha_string_cost_per_block(&self) -> u64 {
        self.ecvrf_ecvrf_verify_alpha_string_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn ed25519_ed25519_verify_cost_base(&self) -> u64 {
        self.ed25519_ed25519_verify_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ed25519_ed25519_verify_msg_cost_per_byte(&self) -> u64 {
        self.ed25519_ed25519_verify_msg_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn ed25519_ed25519_verify_msg_cost_per_block(&self) -> u64 {
        self.ed25519_ed25519_verify_msg_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn groth16_prepare_verifying_key_bls12381_cost_base(&self) -> u64 {
        self.groth16_prepare_verifying_key_bls12381_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn groth16_prepare_verifying_key_bn254_cost_base(&self) -> u64 {
        self.groth16_prepare_verifying_key_bn254_cost_base
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn groth16_verify_groth16_proof_internal_bls12381_cost_base(&self) -> u64 {
        self.groth16_verify_groth16_proof_internal_bls12381_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn groth16_verify_groth16_proof_internal_bls12381_cost_per_public_input(&self) -> u64 {
        self.groth16_verify_groth16_proof_internal_bls12381_cost_per_public_input
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn groth16_verify_groth16_proof_internal_bn254_cost_base(&self) -> u64 {
        self.groth16_verify_groth16_proof_internal_bn254_cost_base
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn groth16_verify_groth16_proof_internal_bn254_cost_per_public_input(&self) -> u64 {
        self.groth16_verify_groth16_proof_internal_bn254_cost_per_public_input
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn groth16_verify_groth16_proof_internal_public_input_cost_per_byte(&self) -> u64 {
        self.groth16_verify_groth16_proof_internal_public_input_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn hash_blake2b256_cost_base(&self) -> u64 {
        self.hash_blake2b256_cost_base.expect(CONSTANT_ERR_MSG)
    }
    pub fn hash_blake2b256_data_cost_per_byte(&self) -> u64 {
        self.hash_blake2b256_data_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn hash_blake2b256_data_cost_per_block(&self) -> u64 {
        self.hash_blake2b256_data_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn hash_keccak256_cost_base(&self) -> u64 {
        self.hash_keccak256_cost_base.expect(CONSTANT_ERR_MSG)
    }
    pub fn hash_keccak256_data_cost_per_byte(&self) -> u64 {
        self.hash_keccak256_data_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn hash_keccak256_data_cost_per_block(&self) -> u64 {
        self.hash_keccak256_data_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }

    pub fn hmac_hmac_sha3_256_cost_base(&self) -> u64 {
        self.hmac_hmac_sha3_256_cost_base.expect(CONSTANT_ERR_MSG)
    }
    pub fn hmac_hmac_sha3_256_input_cost_per_byte(&self) -> u64 {
        self.hmac_hmac_sha3_256_input_cost_per_byte
            .expect(CONSTANT_ERR_MSG)
    }
    pub fn hmac_hmac_sha3_256_input_cost_per_block(&self) -> u64 {
        self.hmac_hmac_sha3_256_input_cost_per_block
            .expect(CONSTANT_ERR_MSG)
    }

    /// We dont unwrap here because we want to be able to selectively fetch this valuue
    pub fn max_size_written_objects(&self) -> Option<u64> {
        self.max_size_written_objects
    }
    /// We dont unwrap here because we want to be able to selectively fetch this valuue
    pub fn max_size_written_objects_system_tx(&self) -> Option<u64> {
        self.max_size_written_objects_system_tx
    }

    // When adding a new constant, create a new getter for it as follows, so that the validator
    // will crash if the constant is accessed before the protocol in which it is defined.
    //
    // pub fn new_constant(&self) -> u64 {
    //     self.new_constant.expect(CONSTANT_ERR_MSG)
    // }
}

#[cfg(not(msim))]
static POISON_VERSION_METHODS: AtomicBool = AtomicBool::new(false);

// Use a thread local in sim tests for test isolation.
#[cfg(msim)]
thread_local! {
    static POISON_VERSION_METHODS: AtomicBool = AtomicBool::new(false);
}

// Instantiations for each protocol version.
impl ProtocolConfig {
    /// Get the value ProtocolConfig that are in effect during the given protocol version.
    pub fn get_for_version(version: ProtocolVersion) -> Self {
        // ProtocolVersion can be deserialized so we need to check it here as well.
        assert!(version.0 >= ProtocolVersion::MIN.0, "{:?}", version);
        assert!(version.0 <= ProtocolVersion::MAX_ALLOWED.0, "{:?}", version);

        let mut ret = Self::get_for_version_impl(version);
        ret.version = version;

        CONFIG_OVERRIDE.with(|ovr| {
            if let Some(override_fn) = &*ovr.borrow() {
                warn!(
                    "overriding ProtocolConfig settings with custom settings (you should not see this log outside of tests)"
                );
                override_fn(version, ret)
            } else {
                ret
            }
        })
    }

    #[cfg(not(msim))]
    pub fn poison_get_for_min_version() {
        POISON_VERSION_METHODS.store(true, Ordering::Relaxed);
    }

    #[cfg(not(msim))]
    fn load_poison_get_for_min_version() -> bool {
        POISON_VERSION_METHODS.load(Ordering::Relaxed)
    }

    #[cfg(msim)]
    pub fn poison_get_for_min_version() {
        POISON_VERSION_METHODS.with(|p| p.store(true, Ordering::Relaxed));
    }

    #[cfg(msim)]
    fn load_poison_get_for_min_version() -> bool {
        POISON_VERSION_METHODS.with(|p| p.load(Ordering::Relaxed))
    }

    /// Convenience to get the constants at the current minimum supported version.
    /// Mainly used by client code that may not yet be protocol-version aware.
    pub fn get_for_min_version() -> Self {
        if Self::load_poison_get_for_min_version() {
            panic!("get_for_min_version called on validator");
        }
        ProtocolConfig::get_for_version(ProtocolVersion::MIN)
    }

    /// Convenience to get the constants at the current maximum supported version.
    /// Mainly used by genesis.
    pub fn get_for_max_version() -> Self {
        if Self::load_poison_get_for_min_version() {
            panic!("get_for_max_version called on validator");
        }
        ProtocolConfig::get_for_version(ProtocolVersion::MAX)
    }

    fn get_for_version_impl(version: ProtocolVersion) -> Self {
        #[cfg(msim)]
        {
            // populate the fake simulator version # with a different base tx cost.
            if version == ProtocolVersion::MAX_ALLOWED {
                let mut config = Self::get_for_version_impl(version - 1);
                config.base_tx_cost_fixed = Some(config.base_tx_cost_fixed() + 1000);
                return config;
            }
        }

        // IMPORTANT: Never modify the value of any constant for a pre-existing protocol version.
        // To change the values here you must create a new protocol version with the new values!
        match version.0 {
            1 => Self {
                // will be overwritten before being returned
                version,

                // All flags are disabled in V1
                feature_flags: Default::default(),

                max_tx_size_bytes: Some(128 * 1024),
                // We need this number to be at least 100x less than `max_serialized_tx_effects_size_bytes`otherwise effects can be huge
                max_input_objects: Some(2048),
                max_serialized_tx_effects_size_bytes: Some(512 * 1024),
                max_serialized_tx_effects_size_bytes_system_tx: Some(512 * 1024 * 16),
                max_gas_payment_objects: Some(256),
                max_modules_in_publish: Some(128),
                max_arguments: Some(512),
                max_type_arguments: Some(16),
                max_type_argument_depth: Some(16),
                max_pure_argument_size: Some(16 * 1024),
                max_programmable_tx_commands: Some(1024),
                move_binary_format_version: Some(6),
                max_move_object_size: Some(250 * 1024),
                max_move_package_size: Some(100 * 1024),
                max_tx_gas: Some(10_000_000_000),
                max_loop_depth: Some(5),
                max_generic_instantiation_length: Some(32),
                max_function_parameters: Some(128),
                max_basic_blocks: Some(1024),
                max_value_stack_size: Some(1024),
                max_type_nodes: Some(256),
                max_push_size: Some(10000),
                max_struct_definitions: Some(200),
                max_function_definitions: Some(1000),
                max_fields_in_struct: Some(32),
                max_dependency_depth: Some(100),
                max_num_event_emit: Some(256),
                max_num_new_move_object_ids: Some(2048),
                max_num_new_move_object_ids_system_tx: Some(2048 * 16),
                max_num_deleted_move_object_ids: Some(2048),
                max_num_deleted_move_object_ids_system_tx: Some(2048 * 16),
                max_num_transferred_move_object_ids: Some(2048),
                max_num_transferred_move_object_ids_system_tx: Some(2048 * 16),
                max_event_emit_size: Some(250 * 1024),
                max_move_vector_len: Some(256 * 1024),

                /// TODO: Is this too low/high?
                max_back_edges_per_function: Some(10_000),

                /// TODO:  Is this too low/high?
                max_back_edges_per_module: Some(10_000),

                /// TODO: Is this too low/high?
                max_verifier_meter_ticks_per_function: Some(6_000_000),

                /// TODO: Is this too low/high?
                max_meter_ticks_per_module: Some(6_000_000),

                object_runtime_max_num_cached_objects: Some(1000),
                object_runtime_max_num_cached_objects_system_tx: Some(1000 * 16),
                object_runtime_max_num_store_entries: Some(1000),
                object_runtime_max_num_store_entries_system_tx: Some(1000 * 16),
                base_tx_cost_fixed: Some(110_000),
                package_publish_cost_fixed: Some(1_000),
                base_tx_cost_per_byte: Some(0),
                package_publish_cost_per_byte: Some(80),
                obj_access_cost_read_per_byte: Some(15),
                obj_access_cost_mutate_per_byte: Some(40),
                obj_access_cost_delete_per_byte: Some(40),
                obj_access_cost_verify_per_byte: Some(200),
                obj_data_cost_refundable: Some(100),
                obj_metadata_cost_non_refundable: Some(50),
                gas_model_version: Some(1),
                storage_rebate_rate: Some(9900),
                storage_fund_reinvest_rate: Some(500),
                reward_slashing_rate: Some(5000),
                storage_gas_price: Some(1),
                max_transactions_per_checkpoint: Some(10_000),
                max_checkpoint_size_bytes: Some(30 * 1024 * 1024),

                // For now, perform upgrades with a bare quorum of validators.
                // MUSTFIX: This number should be increased to at least 2000 (20%) for mainnet.
                buffer_stake_for_protocol_upgrade_bps: Some(0),

                /// === Native Function Costs ===
                // `address` module
                // Cost params for the Move native function `address::from_bytes(bytes: vector<u8>)`
                address_from_bytes_cost_base: Some(52),
                // Cost params for the Move native function `address::to_u256(address): u256`
                address_to_u256_cost_base: Some(52),
                // Cost params for the Move native function `address::from_u256(u256): address`
                address_from_u256_cost_base: Some(52),

                // `dynamic_field` module
                // Cost params for the Move native function `hash_type_and_key<K: copy + drop + store>(parent: address, k: K): address`
                dynamic_field_hash_type_and_key_cost_base: Some(100),
                dynamic_field_hash_type_and_key_type_cost_per_byte: Some(2),
                dynamic_field_hash_type_and_key_value_cost_per_byte: Some(2),
                dynamic_field_hash_type_and_key_type_tag_cost_per_byte: Some(2),
                // Cost params for the Move native function `add_child_object<Child: key>(parent: address, child: Child)`
                dynamic_field_add_child_object_cost_base: Some(100),
                dynamic_field_add_child_object_type_cost_per_byte: Some(10),
                dynamic_field_add_child_object_value_cost_per_byte: Some(10),
                dynamic_field_add_child_object_struct_tag_cost_per_byte: Some(10),
                // Cost params for the Move native function `borrow_child_object_mut<Child: key>(parent: &mut UID, id: address): &mut Child`
                dynamic_field_borrow_child_object_cost_base: Some(100),
                dynamic_field_borrow_child_object_child_ref_cost_per_byte: Some(10),
                dynamic_field_borrow_child_object_type_cost_per_byte: Some(10),
                 // Cost params for the Move native function `remove_child_object<Child: key>(parent: address, id: address): Child`
                dynamic_field_remove_child_object_cost_base: Some(100),
                dynamic_field_remove_child_object_child_cost_per_byte: Some(2),
                dynamic_field_remove_child_object_type_cost_per_byte: Some(2),
                // Cost params for the Move native function `has_child_object(parent: address, id: address): bool`
                dynamic_field_has_child_object_cost_base: Some(100),
                // Cost params for the Move native function `has_child_object_with_ty<Child: key>(parent: address, id: address): bool`
                dynamic_field_has_child_object_with_ty_cost_base: Some(100),
                dynamic_field_has_child_object_with_ty_type_cost_per_byte: Some(2),
                dynamic_field_has_child_object_with_ty_type_tag_cost_per_byte: Some(2),

                // `event` module
                // Cost params for the Move native function `event::emit<T: copy + drop>(event: T)`
                event_emit_cost_base: Some(52),
                event_emit_value_size_derivation_cost_per_byte: Some(2),
                event_emit_tag_size_derivation_cost_per_byte: Some(5),
                event_emit_output_cost_per_byte:Some(10),

                //  `object` module
                // Cost params for the Move native function `borrow_uid<T: key>(obj: &T): &UID`
                object_borrow_uid_cost_base: Some(52),
                // Cost params for the Move native function `delete_impl(id: address)`
                object_delete_impl_cost_base: Some(52),
                // Cost params for the Move native function `record_new_uid(id: address)`
                object_record_new_uid_cost_base: Some(52),

                // `transfer` module
                // Cost params for the Move native function `transfer_impl<T: key>(obj: T, recipient: address)`
                transfer_transfer_internal_cost_base: Some(52),
                // Cost params for the Move native function `freeze_object<T: key>(obj: T)`
                transfer_freeze_object_cost_base: Some(52),
                // Cost params for the Move native function `share_object<T: key>(obj: T)`
                transfer_share_object_cost_base: Some(52),

                // `tx_context` module
                // Cost params for the Move native function `transfer_impl<T: key>(obj: T, recipient: address)`
                tx_context_derive_id_cost_base: Some(52),

                // `types` module
                // Cost params for the Move native function `is_one_time_witness<T: drop>(_: &T): bool`
                types_is_one_time_witness_cost_base: Some(52),
                types_is_one_time_witness_type_tag_cost_per_byte: Some(2),
                types_is_one_time_witness_type_cost_per_byte: Some(2),

                // `validator` module
                // Cost params for the Move native function `validate_metadata_bcs(metadata: vector<u8>)`
                validator_validate_metadata_cost_base: Some(52),
                validator_validate_metadata_data_cost_per_byte: Some(2),

                // Crypto
                crypto_invalid_arguments_cost: Some(100),
                // bls12381::bls12381_min_pk_verify
                bls12381_bls12381_min_sig_verify_cost_base: Some(52),
                bls12381_bls12381_min_sig_verify_msg_cost_per_byte: Some(2),
                bls12381_bls12381_min_sig_verify_msg_cost_per_block: Some(2),

                // bls12381::bls12381_min_pk_verify
                bls12381_bls12381_min_pk_verify_cost_base: Some(52),
                bls12381_bls12381_min_pk_verify_msg_cost_per_byte: Some(2),
                bls12381_bls12381_min_pk_verify_msg_cost_per_block: Some(2),

                // ecdsa_k1::ecrecover
                ecdsa_k1_ecrecover_keccak256_cost_base: Some(52),
                ecdsa_k1_ecrecover_keccak256_msg_cost_per_byte: Some(2),
                ecdsa_k1_ecrecover_keccak256_msg_cost_per_block: Some(2),
                ecdsa_k1_ecrecover_sha256_cost_base: Some(52),
                ecdsa_k1_ecrecover_sha256_msg_cost_per_byte: Some(2),
                ecdsa_k1_ecrecover_sha256_msg_cost_per_block: Some(2),

                // ecdsa_k1::decompress_pubkey
                ecdsa_k1_decompress_pubkey_cost_base: Some(52),

                // ecdsa_k1::secp256k1_verify
                ecdsa_k1_secp256k1_verify_keccak256_cost_base: Some(52),
                ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_byte: Some(2),
                ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_block: Some(2),
                ecdsa_k1_secp256k1_verify_sha256_cost_base: Some(52),
                ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_byte: Some(2),
                ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_block: Some(2),

                // ecdsa_r1::ecrecover
                ecdsa_r1_ecrecover_keccak256_cost_base: Some(52),
                ecdsa_r1_ecrecover_keccak256_msg_cost_per_byte: Some(2),
                ecdsa_r1_ecrecover_keccak256_msg_cost_per_block: Some(2),
                ecdsa_r1_ecrecover_sha256_cost_base: Some(52),
                ecdsa_r1_ecrecover_sha256_msg_cost_per_byte: Some(2),
                ecdsa_r1_ecrecover_sha256_msg_cost_per_block: Some(2),

                // ecdsa_r1::secp256k1_verify
                ecdsa_r1_secp256r1_verify_keccak256_cost_base: Some(52),
                ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_byte: Some(2),
                ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_block: Some(2),
                ecdsa_r1_secp256r1_verify_sha256_cost_base: Some(52),
                ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_byte: Some(2),
                ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_block: Some(2),

                // ecvrf::verify
                ecvrf_ecvrf_verify_cost_base: Some(52),
                ecvrf_ecvrf_verify_alpha_string_cost_per_byte: Some(2),
                ecvrf_ecvrf_verify_alpha_string_cost_per_block: Some(2),

                // ed25519
                ed25519_ed25519_verify_cost_base: Some(52),
                ed25519_ed25519_verify_msg_cost_per_byte: Some(2),
                ed25519_ed25519_verify_msg_cost_per_block: Some(2),

                // groth16::prepare_verifying_key
                groth16_prepare_verifying_key_bls12381_cost_base: Some(52),
                groth16_prepare_verifying_key_bn254_cost_base: Some(52),

                // groth16::verify_groth16_proof_internal
                groth16_verify_groth16_proof_internal_bls12381_cost_base: Some(52),
                groth16_verify_groth16_proof_internal_bls12381_cost_per_public_input: Some(2),
                groth16_verify_groth16_proof_internal_bn254_cost_base: Some(52),
                groth16_verify_groth16_proof_internal_bn254_cost_per_public_input: Some(2),
                groth16_verify_groth16_proof_internal_public_input_cost_per_byte: Some(2),

                // hash::blake2b256
                hash_blake2b256_cost_base: Some(52),
                hash_blake2b256_data_cost_per_byte: Some(2),
                hash_blake2b256_data_cost_per_block: Some(2),
                // hash::keccak256
                hash_keccak256_cost_base: Some(52),
                hash_keccak256_data_cost_per_byte: Some(2),
                hash_keccak256_data_cost_per_block: Some(2),

                // hmac::hmac_sha3_256
                hmac_hmac_sha3_256_cost_base: Some(52),
                hmac_hmac_sha3_256_input_cost_per_byte: Some(2),
                hmac_hmac_sha3_256_input_cost_per_block: Some(2),


                max_size_written_objects: None,
                max_size_written_objects_system_tx: None,

                // When adding a new constant, set it to None in the earliest version, like this:
                // new_constant: None,
            },
            2 => {
                let mut cfg = Self::get_for_version_impl(version - 1);
                cfg.feature_flags.advance_epoch_start_time_in_safe_mode = true;
                cfg
            }
            3 => {
                let mut cfg = Self::get_for_version_impl(version - 1);
                // changes for gas model
                cfg.gas_model_version = Some(2);
                // max gas budget is in MIST and an absolute value 50SUI
                cfg.max_tx_gas = Some(50_000_000_000);
                // min gas budget is in MIST and an absolute value 2000MIST or 0.000002SUI
                cfg.base_tx_cost_fixed = Some(2_000);
                // storage gas price multiplier
                cfg.storage_gas_price = Some(76);
                cfg.feature_flags.loaded_child_objects_fixed = true;
                // max size of written objects during a TXn
                cfg.max_size_written_objects = Some(5 * 1000 * 1000);
                // max size of written objects during a system TXn to allow for larger writes
                cfg.max_size_written_objects_system_tx = Some(50 * 1000 * 1000);
                cfg.feature_flags.package_upgrades = true;
                cfg
            }
            4 => {
                let mut cfg = Self::get_for_version_impl(version - 1);
                // Change reward slashing rate to 100%.
                cfg.reward_slashing_rate = Some(10000);
                cfg
            }
            // Use this template when making changes:
            //
            //     // modify an existing constant.
            //     move_binary_format_version: Some(7),
            //
            //     // Add a new constant (which is set to None in prior versions).
            //     new_constant: Some(new_value),
            //
            //     // Remove a constant (ensure that it is never accessed during this version).
            //     max_move_object_size: None,
            //
            //     // Pull in everything else from the previous version to avoid unintentional
            //     // changes.
            //     ..Self::get_for_version_impl(version - 1)
            // },
            _ => panic!("unsupported version {:?}", version),
        }
    }

    /// Override one or more settings in the config, for testing.
    /// This must be called at the beginning of the test, before get_for_(min|max)_version is
    /// called, since those functions cache their return value.
    pub fn apply_overrides_for_testing(
        override_fn: impl Fn(ProtocolVersion, Self) -> Self + Send + 'static,
    ) -> OverrideGuard {
        CONFIG_OVERRIDE.with(|ovr| {
            let mut cur = ovr.borrow_mut();
            assert!(cur.is_none(), "config override already present");
            *cur = Some(Box::new(override_fn));
            OverrideGuard
        })
    }
}

// Setters for tests
impl ProtocolConfig {
    pub fn set_max_function_definitions_for_testing(&mut self, m: u64) {
        self.max_function_definitions = Some(m)
    }
    pub fn set_buffer_stake_for_protocol_upgrade_bps_for_testing(&mut self, b: u64) {
        self.buffer_stake_for_protocol_upgrade_bps = Some(b)
    }
    pub fn set_package_upgrades_for_testing(&mut self, val: bool) {
        self.feature_flags.package_upgrades = val
    }
}

type OverrideFn = dyn Fn(ProtocolVersion, ProtocolConfig) -> ProtocolConfig + Send;

thread_local! {
    static CONFIG_OVERRIDE: RefCell<Option<Box<OverrideFn>>> = RefCell::new(None);
}

#[must_use]
pub struct OverrideGuard;

impl Drop for OverrideGuard {
    fn drop(&mut self) {
        info!("restoring override fn");
        CONFIG_OVERRIDE.with(|ovr| {
            *ovr.borrow_mut() = None;
        });
    }
}

/// Defines which limit got crossed.
/// The value which crossed the limit and value of the limit crossed are embedded
#[derive(PartialEq, Eq)]
pub enum LimitThresholdCrossed {
    None,
    Soft(u128, u128),
    Hard(u128, u128),
}

/// Convenience function for comparing limit ranges
/// V::MAX must be at >= U::MAX and T::MAX
pub fn check_limit_in_range<T: Into<V>, U: Into<V>, V: PartialOrd + Into<u128>>(
    x: T,
    soft_limit: U,
    hard_limit: V,
) -> LimitThresholdCrossed {
    let x: V = x.into();
    let soft_limit: V = soft_limit.into();

    debug_assert!(soft_limit <= hard_limit);

    // It is important to preserve this comparison order because if soft_limit == hard_limit
    // we want LimitThresholdCrossed::Hard
    if x >= hard_limit {
        LimitThresholdCrossed::Hard(x.into(), hard_limit.into())
    } else if x < soft_limit {
        LimitThresholdCrossed::None
    } else {
        LimitThresholdCrossed::Soft(x.into(), soft_limit.into())
    }
}

#[macro_export]
macro_rules! check_limit {
    ($x:expr, $hard:expr) => {
        check_limit!($x, $hard, $hard)
    };
    ($x:expr, $soft:expr, $hard:expr) => {
        check_limit_in_range($x as u64, $soft, $hard)
    };
}

/// Used to check which limits were crossed if the TX is metered (not system tx)
/// Args are: is_metered, value_to_check, metered_limit, unmetered_limit
/// metered_limit is always less than or equal to unmetered_hard_limit
#[macro_export]
macro_rules! check_limit_by_meter {
    ($is_metered:expr, $x:expr, $metered_limit:expr, $unmetered_hard_limit:expr) => {{
        // If this is metered, we use the metered_limit limit as the upper bound
        let h = if $is_metered {
            $metered_limit
        } else {
            // Unmetered gets more headroom
            $unmetered_hard_limit
        };
        use sui_protocol_config::check_limit_in_range;
        check_limit_in_range($x as u64, $metered_limit, h)
    }};
}

#[cfg(all(test, not(msim)))]
mod test {
    use super::*;
    use insta::assert_yaml_snapshot;

    #[test]
    fn snaphost_tests() {
        println!("\n============================================================================");
        println!("!                                                                          !");
        println!("! IMPORTANT: never update snapshots from this test. only add new versions! !");
        println!("! (it is actually ok to update them up until mainnet launches)             !");
        println!("!                                                                          !");
        println!("============================================================================\n");
        for i in MIN_PROTOCOL_VERSION..=MAX_PROTOCOL_VERSION {
            let cur = ProtocolVersion::new(i);
            assert_yaml_snapshot!(
                format!("version_{}", cur.as_u64()),
                ProtocolConfig::get_for_version(cur)
            );
        }
    }
    #[test]
    fn limit_range_fn_test() {
        let low = 100u32;
        let high = 10000u64;

        assert!(check_limit!(1u8, low, high) == LimitThresholdCrossed::None);
        assert!(matches!(
            check_limit!(255u16, low, high),
            LimitThresholdCrossed::Soft(255u128, 100)
        ));
        // This wont compile because lossy
        //assert!(check_limit!(100000000u128, low, high) == LimitThresholdCrossed::None);
        // This wont compile because lossy
        //assert!(check_limit!(100000000usize, low, high) == LimitThresholdCrossed::None);

        assert!(matches!(
            check_limit!(2550000u64, low, high),
            LimitThresholdCrossed::Hard(2550000, 10000)
        ));

        assert!(matches!(
            check_limit!(2550000u64, high, high),
            LimitThresholdCrossed::Hard(2550000, 10000)
        ));

        assert!(matches!(
            check_limit!(1u8, high),
            LimitThresholdCrossed::None
        ));

        assert!(check_limit!(255u16, high) == LimitThresholdCrossed::None);

        assert!(matches!(
            check_limit!(2550000u64, high),
            LimitThresholdCrossed::Hard(2550000, 10000)
        ));
    }
}
