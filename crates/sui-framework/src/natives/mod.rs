// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod address;
mod crypto;
mod dynamic_field;
mod event;
mod object;
pub mod object_runtime;
mod test_scenario;
mod test_utils;
mod transfer;
mod tx_context;
mod types;
mod validator;

use crate::make_native;
use better_any::{Tid, TidAble};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_stdlib::natives::{GasParameters, NurseryGasParameters};
use move_vm_runtime::native_functions::{NativeFunction, NativeFunctionTable};
use move_vm_types::{
    natives::function::NativeResult,
    values::{Struct, Value},
};
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;

use self::{
    address::{AddressFromBytesCostParams, AddressFromU256CostParams, AddressToU256CostParams},
    crypto::{bls12381, ecdsa_k1, ecdsa_r1, ecvrf, ed25519, groth16, hash, hmac},
    crypto::{
        ed25519::Ed25519VerifyCostParams,
        hash::{HashBlake2b256CostParams, HashKeccak256CostParams},
    },
    dynamic_field::{
        DynamicFieldAddChildObjectCostParams, DynamicFieldBorrowChildObjectCostParams,
        DynamicFieldHasChildObjectCostParams, DynamicFieldHasChildObjectWithTyCostParams,
        DynamicFieldHashTypeAndKeyCostParams, DynamicFieldRemoveChildObjectCostParams,
    },
    event::EventEmitCostParams,
    object::{BorrowUidCostParams, DeleteImplCostParams, RecordNewIdCostParams},
    transfer::{
        TransferFreezeObjectCostParams, TransferInternalCostParams, TransferShareObjectCostParams,
    },
    tx_context::TxContextDeriveIdCostParams,
    types::TypesIsOneTimeWitnessCostParams,
    validator::ValidatorValidateMetadataBcsCostParams,
};

#[derive(Tid)]
pub struct NativesCostTable {
    // Address natives
    pub address_from_bytes_cost_params: AddressFromBytesCostParams,
    pub address_to_u256_cost_params: AddressToU256CostParams,
    pub address_from_u256_cost_params: AddressFromU256CostParams,

    // Dynamic field natives
    pub dynamic_field_hash_type_and_key_cost_params: DynamicFieldHashTypeAndKeyCostParams,
    pub dynamic_field_add_child_object_cost_params: DynamicFieldAddChildObjectCostParams,
    pub dynamic_field_borrow_child_object_cost_params: DynamicFieldBorrowChildObjectCostParams,
    pub dynamic_field_remove_child_object_cost_params: DynamicFieldRemoveChildObjectCostParams,
    pub dynamic_field_has_child_object_cost_params: DynamicFieldHasChildObjectCostParams,
    pub dynamic_field_has_child_object_with_ty_cost_params:
        DynamicFieldHasChildObjectWithTyCostParams,

    // Event natives
    pub event_emit_cost_params: EventEmitCostParams,

    // Object
    pub borrow_uid_cost_params: BorrowUidCostParams,
    pub delete_impl_cost_params: DeleteImplCostParams,
    pub record_new_id_cost_params: RecordNewIdCostParams,

    // Transfer
    pub transfer_transfer_internal_cost_params: TransferInternalCostParams,
    pub transfer_freeze_object_cost_params: TransferFreezeObjectCostParams,
    pub transfer_share_object_cost_params: TransferShareObjectCostParams,

    // TxContext
    pub tx_context_derive_id_cost_params: TxContextDeriveIdCostParams,

    // Type
    pub type_is_one_time_witness_cost_params: TypesIsOneTimeWitnessCostParams,

    // Validator
    pub validator_validate_metadata_bcs_cost_params: ValidatorValidateMetadataBcsCostParams,

    // Crypto natives
    // ed25519
    pub ed25519_verify_cost_params: Ed25519VerifyCostParams,
    // hash
    pub hash_blake2b256_cost_params: HashBlake2b256CostParams,
    pub hash_keccak256_cost_params: HashKeccak256CostParams,
}

impl NativesCostTable {
    pub fn from_protocol_config(protocol_config: &ProtocolConfig) -> NativesCostTable {
        Self {
            address_from_bytes_cost_params: AddressFromBytesCostParams {
                address_from_bytes_cost_base: protocol_config.address_from_bytes_cost_base().into(),
            },
            address_to_u256_cost_params: AddressToU256CostParams {
                address_to_u256_cost_base: protocol_config.address_to_u256_cost_base().into(),
            },
            address_from_u256_cost_params: AddressFromU256CostParams {
                address_from_u256_cost_base: protocol_config.address_from_u256_cost_base().into(),
            },

            dynamic_field_hash_type_and_key_cost_params: DynamicFieldHashTypeAndKeyCostParams {
                dynamic_field_hash_type_and_key_cost_base: protocol_config
                    .dynamic_field_hash_type_and_key_cost_base()
                    .into(),
                dynamic_field_hash_type_and_key_type_cost_per_byte: protocol_config
                    .dynamic_field_hash_type_and_key_type_cost_per_byte()
                    .into(),
                dynamic_field_hash_type_and_key_value_cost_per_byte: protocol_config
                    .dynamic_field_hash_type_and_key_value_cost_per_byte()
                    .into(),
                dynamic_field_hash_type_and_key_type_tag_cost_per_byte: protocol_config
                    .dynamic_field_hash_type_and_key_type_tag_cost_per_byte()
                    .into(),
            },
            dynamic_field_add_child_object_cost_params: DynamicFieldAddChildObjectCostParams {
                dynamic_field_add_child_object_cost_base: protocol_config
                    .dynamic_field_add_child_object_cost_base()
                    .into(),
                dynamic_field_add_child_object_type_cost_per_byte: protocol_config
                    .dynamic_field_add_child_object_type_cost_per_byte()
                    .into(),
                dynamic_field_add_child_object_value_cost_per_byte: protocol_config
                    .dynamic_field_add_child_object_value_cost_per_byte()
                    .into(),
                dynamic_field_add_child_object_struct_tag_cost_per_byte: protocol_config
                    .dynamic_field_add_child_object_struct_tag_cost_per_byte()
                    .into(),
            },
            dynamic_field_borrow_child_object_cost_params:
                DynamicFieldBorrowChildObjectCostParams {
                    dynamic_field_borrow_child_object_cost_base: protocol_config
                        .dynamic_field_borrow_child_object_cost_base()
                        .into(),
                    dynamic_field_borrow_child_object_child_ref_cost_per_byte: protocol_config
                        .dynamic_field_borrow_child_object_child_ref_cost_per_byte()
                        .into(),
                    dynamic_field_borrow_child_object_type_cost_per_byte: protocol_config
                        .dynamic_field_borrow_child_object_type_cost_per_byte()
                        .into(),
                },
            dynamic_field_remove_child_object_cost_params:
                DynamicFieldRemoveChildObjectCostParams {
                    dynamic_field_remove_child_object_cost_base: protocol_config
                        .dynamic_field_remove_child_object_cost_base()
                        .into(),
                    dynamic_field_remove_child_object_child_cost_per_byte: protocol_config
                        .dynamic_field_remove_child_object_child_cost_per_byte()
                        .into(),
                    dynamic_field_remove_child_object_type_cost_per_byte: protocol_config
                        .dynamic_field_remove_child_object_type_cost_per_byte()
                        .into(),
                },
            dynamic_field_has_child_object_cost_params: DynamicFieldHasChildObjectCostParams {
                dynamic_field_has_child_object_cost_base: protocol_config
                    .dynamic_field_has_child_object_cost_base()
                    .into(),
            },
            dynamic_field_has_child_object_with_ty_cost_params:
                DynamicFieldHasChildObjectWithTyCostParams {
                    dynamic_field_has_child_object_with_ty_cost_base: protocol_config
                        .dynamic_field_has_child_object_with_ty_cost_base()
                        .into(),
                    dynamic_field_has_child_object_with_ty_type_cost_per_byte: protocol_config
                        .dynamic_field_has_child_object_with_ty_type_cost_per_byte()
                        .into(),
                    dynamic_field_has_child_object_with_ty_type_tag_cost_per_byte: protocol_config
                        .dynamic_field_has_child_object_with_ty_type_tag_cost_per_byte()
                        .into(),
                },

            event_emit_cost_params: EventEmitCostParams {
                event_emit_value_size_derivation_cost_per_byte: protocol_config
                    .event_emit_value_size_derivation_cost_per_byte()
                    .into(),
                event_emit_tag_size_derivation_cost_per_byte: protocol_config
                    .event_emit_tag_size_derivation_cost_per_byte()
                    .into(),
                event_emit_output_cost_per_byte: protocol_config
                    .event_emit_output_cost_per_byte()
                    .into(),
                event_emit_cost_base: protocol_config.event_emit_cost_base().into(),
            },

            borrow_uid_cost_params: BorrowUidCostParams {
                object_borrow_uid_cost_base: protocol_config.object_borrow_uid_cost_base().into(),
            },
            delete_impl_cost_params: DeleteImplCostParams {
                object_delete_impl_cost_base: protocol_config.object_delete_impl_cost_base().into(),
            },
            record_new_id_cost_params: RecordNewIdCostParams {
                object_record_new_uid_cost_base: protocol_config
                    .object_record_new_uid_cost_base()
                    .into(),
            },

            // Crypto
            // ed25519
            ed25519_verify_cost_params: Ed25519VerifyCostParams {
                ed25519_ed25519_verify_cost_base: protocol_config
                    .ed25519_ed25519_verify_cost_base()
                    .into(),
                ed25519_ed25519_verify_msg_cost_per_byte: protocol_config
                    .ed25519_ed25519_verify_msg_cost_per_byte()
                    .into(),
                ed25519_ed25519_verify_msg_cost_per_block: protocol_config
                    .ed25519_ed25519_verify_msg_cost_per_block()
                    .into(),
            },
            // hash
            hash_blake2b256_cost_params: HashBlake2b256CostParams {
                hash_blake2b256_cost_base: protocol_config.hash_blake2b256_cost_base().into(),
                hash_blake2b256_data_cost_per_byte: protocol_config
                    .hash_blake2b256_data_cost_per_byte()
                    .into(),
                hash_blake2b256_data_cost_per_block: protocol_config
                    .hash_blake2b256_data_cost_per_block()
                    .into(),
            },
            hash_keccak256_cost_params: HashKeccak256CostParams {
                hash_keccak256_cost_base: protocol_config.hash_keccak256_cost_base().into(),
                hash_keccak256_data_cost_per_byte: protocol_config
                    .hash_keccak256_data_cost_per_byte()
                    .into(),
                hash_keccak256_data_cost_per_block: protocol_config
                    .hash_keccak256_data_cost_per_block()
                    .into(),
            },
            transfer_transfer_internal_cost_params: TransferInternalCostParams {
                transfer_transfer_internal_cost_base: protocol_config
                    .transfer_transfer_internal_cost_base()
                    .into(),
            },
            transfer_freeze_object_cost_params: TransferFreezeObjectCostParams {
                transfer_freeze_object_cost_base: protocol_config
                    .transfer_freeze_object_cost_base()
                    .into(),
            },
            transfer_share_object_cost_params: TransferShareObjectCostParams {
                transfer_share_object_cost_base: protocol_config
                    .transfer_share_object_cost_base()
                    .into(),
            },
            tx_context_derive_id_cost_params: TxContextDeriveIdCostParams {
                tx_context_derive_id_cost_base: protocol_config
                    .tx_context_derive_id_cost_base()
                    .into(),
            },
            type_is_one_time_witness_cost_params: TypesIsOneTimeWitnessCostParams {
                types_is_one_time_witness_cost_base: protocol_config
                    .types_is_one_time_witness_cost_base()
                    .into(),
                types_is_one_time_witness_type_tag_cost_per_byte: protocol_config
                    .types_is_one_time_witness_type_tag_cost_per_byte()
                    .into(),
                types_is_one_time_witness_type_cost_per_byte: protocol_config
                    .types_is_one_time_witness_type_cost_per_byte()
                    .into(),
            },
            validator_validate_metadata_bcs_cost_params: ValidatorValidateMetadataBcsCostParams {
                validator_validate_metadata_cost_base: protocol_config
                    .validator_validate_metadata_cost_base()
                    .into(),
                validator_validate_metadata_data_cost_per_byte: protocol_config
                    .validator_validate_metadata_data_cost_per_byte()
                    .into(),
            },
        }
    }
}

pub fn all_natives(
    move_stdlib_addr: AccountAddress,
    sui_framework_addr: AccountAddress,
) -> NativeFunctionTable {
    let sui_natives: &[(&str, &str, NativeFunction)] = &[
        ("address", "from_bytes", make_native!(address::from_bytes)),
        ("address", "to_u256", make_native!(address::to_u256)),
        ("address", "from_u256", make_native!(address::from_u256)),
        ("hash", "blake2b256", make_native!(hash::blake2b256)),
        (
            "bls12381",
            "bls12381_min_sig_verify",
            make_native!(bls12381::bls12381_min_sig_verify),
        ),
        (
            "bls12381",
            "bls12381_min_pk_verify",
            make_native!(bls12381::bls12381_min_pk_verify),
        ),
        (
            "dynamic_field",
            "hash_type_and_key",
            make_native!(dynamic_field::hash_type_and_key),
        ),
        (
            "dynamic_field",
            "add_child_object",
            make_native!(dynamic_field::add_child_object),
        ),
        (
            "dynamic_field",
            "borrow_child_object",
            make_native!(dynamic_field::borrow_child_object),
        ),
        (
            "dynamic_field",
            "borrow_child_object_mut",
            make_native!(dynamic_field::borrow_child_object),
        ),
        (
            "dynamic_field",
            "remove_child_object",
            make_native!(dynamic_field::remove_child_object),
        ),
        (
            "dynamic_field",
            "has_child_object",
            make_native!(dynamic_field::has_child_object),
        ),
        (
            "dynamic_field",
            "has_child_object_with_ty",
            make_native!(dynamic_field::has_child_object_with_ty),
        ),
        (
            "ecdsa_k1",
            "secp256k1_ecrecover",
            make_native!(ecdsa_k1::ecrecover),
        ),
        (
            "ecdsa_k1",
            "decompress_pubkey",
            make_native!(ecdsa_k1::decompress_pubkey),
        ),
        (
            "ecdsa_k1",
            "secp256k1_verify",
            make_native!(ecdsa_k1::secp256k1_verify),
        ),
        ("ecvrf", "ecvrf_verify", make_native!(ecvrf::ecvrf_verify)),
        (
            "ecdsa_r1",
            "secp256r1_ecrecover",
            make_native!(ecdsa_r1::ecrecover),
        ),
        (
            "ecdsa_r1",
            "secp256r1_verify",
            make_native!(ecdsa_r1::secp256r1_verify),
        ),
        (
            "ed25519",
            "ed25519_verify",
            make_native!(ed25519::ed25519_verify),
        ),
        ("event", "emit", make_native!(event::emit)),
        (
            "groth16",
            "verify_groth16_proof_internal",
            make_native!(groth16::verify_groth16_proof_internal),
        ),
        (
            "groth16",
            "prepare_verifying_key_internal",
            make_native!(groth16::prepare_verifying_key_internal),
        ),
        (
            "hmac",
            "native_hmac_sha3_256",
            make_native!(hmac::hmac_sha3_256),
        ),
        ("hash", "keccak256", make_native!(hash::keccak256)),
        ("object", "delete_impl", make_native!(object::delete_impl)),
        ("object", "borrow_uid", make_native!(object::borrow_uid)),
        (
            "object",
            "record_new_uid",
            make_native!(object::record_new_uid),
        ),
        (
            "test_scenario",
            "take_from_address_by_id",
            make_native!(test_scenario::take_from_address_by_id),
        ),
        (
            "test_scenario",
            "most_recent_id_for_address",
            make_native!(test_scenario::most_recent_id_for_address),
        ),
        (
            "test_scenario",
            "was_taken_from_address",
            make_native!(test_scenario::was_taken_from_address),
        ),
        (
            "test_scenario",
            "take_immutable_by_id",
            make_native!(test_scenario::take_immutable_by_id),
        ),
        (
            "test_scenario",
            "most_recent_immutable_id",
            make_native!(test_scenario::most_recent_immutable_id),
        ),
        (
            "test_scenario",
            "was_taken_immutable",
            make_native!(test_scenario::was_taken_immutable),
        ),
        (
            "test_scenario",
            "take_shared_by_id",
            make_native!(test_scenario::take_shared_by_id),
        ),
        (
            "test_scenario",
            "most_recent_id_shared",
            make_native!(test_scenario::most_recent_id_shared),
        ),
        (
            "test_scenario",
            "was_taken_shared",
            make_native!(test_scenario::was_taken_shared),
        ),
        (
            "test_scenario",
            "end_transaction",
            make_native!(test_scenario::end_transaction),
        ),
        (
            "test_scenario",
            "ids_for_address",
            make_native!(test_scenario::ids_for_address),
        ),
        (
            "transfer",
            "transfer_impl",
            make_native!(transfer::transfer_internal),
        ),
        (
            "transfer",
            "freeze_object_impl",
            make_native!(transfer::freeze_object),
        ),
        (
            "transfer",
            "share_object_impl",
            make_native!(transfer::share_object),
        ),
        (
            "tx_context",
            "derive_id",
            make_native!(tx_context::derive_id),
        ),
        (
            "types",
            "is_one_time_witness",
            make_native!(types::is_one_time_witness),
        ),
        (
            "validator",
            "validate_metadata_bcs",
            make_native!(validator::validate_metadata_bcs),
        ),
        ("test_utils", "destroy", make_native!(test_utils::destroy)),
        (
            "test_utils",
            "create_one_time_witness",
            make_native!(test_utils::create_one_time_witness),
        ),
    ];
    sui_natives
        .iter()
        .cloned()
        .map(|(module_name, func_name, func)| {
            (
                sui_framework_addr,
                Identifier::new(module_name).unwrap(),
                Identifier::new(func_name).unwrap(),
                func,
            )
        })
        .chain(move_stdlib::natives::all_natives(
            move_stdlib_addr,
            // TODO: tune gas params
            GasParameters::zeros(),
        ))
        .chain(move_stdlib::natives::nursery_natives(
            move_stdlib_addr,
            // TODO: tune gas params
            NurseryGasParameters::zeros(),
        ))
        .collect()
}

// Object { id: UID { id: ID { bytes: address } } .. }
// Extract the first field of the struct 3 times to get the id bytes.
pub fn get_object_id(object: Value) -> Result<Value, PartialVMError> {
    get_nested_struct_field(object, &[0, 0, 0])
}

// Extract a field valye that's nested inside value `v`. The offset of each nesting
// is determined by `offsets`.
pub fn get_nested_struct_field(mut v: Value, offsets: &[usize]) -> Result<Value, PartialVMError> {
    for offset in offsets {
        v = get_nth_struct_field(v, *offset)?;
    }
    Ok(v)
}

pub fn get_nth_struct_field(v: Value, n: usize) -> Result<Value, PartialVMError> {
    let mut itr = v.value_as::<Struct>()?.unpack()?;
    Ok(itr.nth(n).unwrap())
}

#[macro_export]
macro_rules! make_native {
    ($native: expr) => {
        Arc::new(
            move |context, ty_args, args| -> PartialVMResult<NativeResult> {
                $native(context, ty_args, args)
            },
        )
    };
}
