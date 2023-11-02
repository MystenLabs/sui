// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::{
    address::{AddressFromBytesCostParams, AddressFromU256CostParams, AddressToU256CostParams},
    crypto::{bls12381, ecdsa_k1, ecdsa_r1, ecvrf, ed25519, groth16, hash, hmac},
    crypto::{
        bls12381::{Bls12381Bls12381MinPkVerifyCostParams, Bls12381Bls12381MinSigVerifyCostParams},
        ecdsa_k1::{
            EcdsaK1DecompressPubkeyCostParams, EcdsaK1EcrecoverCostParams,
            EcdsaK1Secp256k1VerifyCostParams,
        },
        ecdsa_r1::{EcdsaR1EcrecoverCostParams, EcdsaR1Secp256R1VerifyCostParams},
        ecvrf::EcvrfEcvrfVerifyCostParams,
        ed25519::Ed25519VerifyCostParams,
        groth16::{
            Groth16PrepareVerifyingKeyCostParams, Groth16VerifyGroth16ProofInternalCostParams,
        },
        hash::{HashBlake2b256CostParams, HashKeccak256CostParams},
        hmac::HmacHmacSha3256CostParams,
        poseidon,
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
use crate::crypto::poseidon::PoseidonBN254CostParams;
use crate::crypto::zklogin;
use crate::crypto::zklogin::{CheckZkloginIdCostParams, CheckZkloginIssuerCostParams};
use better_any::{Tid, TidAble};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    annotated_value as A,
    gas_algebra::InternalGas,
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    runtime_value as R,
    vm_status::StatusCode,
};
use move_stdlib::natives::{GasParameters, NurseryGasParameters};
use move_vm_runtime::native_functions::{NativeContext, NativeFunction, NativeFunctionTable};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    values::{Struct, Value},
};
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_types::{MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_ADDRESS};
use transfer::TransferReceiveObjectInternalCostParams;

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
    pub crypto_invalid_arguments_cost: InternalGas,
    // bls12381
    pub bls12381_bls12381_min_sig_verify_cost_params: Bls12381Bls12381MinSigVerifyCostParams,
    pub bls12381_bls12381_min_pk_verify_cost_params: Bls12381Bls12381MinPkVerifyCostParams,

    // ecdsak1
    pub ecdsa_k1_ecrecover_cost_params: EcdsaK1EcrecoverCostParams,
    pub ecdsa_k1_decompress_pubkey_cost_params: EcdsaK1DecompressPubkeyCostParams,
    pub ecdsa_k1_secp256k1_verify_cost_params: EcdsaK1Secp256k1VerifyCostParams,

    // ecdsar1
    pub ecdsa_r1_ecrecover_cost_params: EcdsaR1EcrecoverCostParams,
    pub ecdsa_r1_secp256_r1_verify_cost_params: EcdsaR1Secp256R1VerifyCostParams,

    // ecvrf
    pub ecvrf_ecvrf_verify_cost_params: EcvrfEcvrfVerifyCostParams,

    // ed25519
    pub ed25519_verify_cost_params: Ed25519VerifyCostParams,

    // groth16
    pub groth16_prepare_verifying_key_cost_params: Groth16PrepareVerifyingKeyCostParams,
    pub groth16_verify_groth16_proof_internal_cost_params:
        Groth16VerifyGroth16ProofInternalCostParams,

    // hash
    pub hash_blake2b256_cost_params: HashBlake2b256CostParams,
    pub hash_keccak256_cost_params: HashKeccak256CostParams,

    // poseidon
    pub poseidon_bn254_cost_params: PoseidonBN254CostParams,

    // hmac
    pub hmac_hmac_sha3_256_cost_params: HmacHmacSha3256CostParams,

    // zklogin
    pub check_zklogin_id_cost_params: CheckZkloginIdCostParams,
    pub check_zklogin_issuer_cost_params: CheckZkloginIssuerCostParams,

    // Receive object
    pub transfer_receive_object_internal_cost_params: TransferReceiveObjectInternalCostParams,
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
            crypto_invalid_arguments_cost: protocol_config.crypto_invalid_arguments_cost().into(),
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
            bls12381_bls12381_min_sig_verify_cost_params: Bls12381Bls12381MinSigVerifyCostParams {
                bls12381_bls12381_min_sig_verify_cost_base: protocol_config
                    .bls12381_bls12381_min_sig_verify_cost_base()
                    .into(),
                bls12381_bls12381_min_sig_verify_msg_cost_per_byte: protocol_config
                    .bls12381_bls12381_min_sig_verify_msg_cost_per_byte()
                    .into(),
                bls12381_bls12381_min_sig_verify_msg_cost_per_block: protocol_config
                    .bls12381_bls12381_min_sig_verify_msg_cost_per_block()
                    .into(),
            },
            bls12381_bls12381_min_pk_verify_cost_params: Bls12381Bls12381MinPkVerifyCostParams {
                bls12381_bls12381_min_pk_verify_cost_base: protocol_config
                    .bls12381_bls12381_min_pk_verify_cost_base()
                    .into(),
                bls12381_bls12381_min_pk_verify_msg_cost_per_byte: protocol_config
                    .bls12381_bls12381_min_pk_verify_msg_cost_per_byte()
                    .into(),
                bls12381_bls12381_min_pk_verify_msg_cost_per_block: protocol_config
                    .bls12381_bls12381_min_pk_verify_msg_cost_per_block()
                    .into(),
            },
            ecdsa_k1_ecrecover_cost_params: EcdsaK1EcrecoverCostParams {
                ecdsa_k1_ecrecover_keccak256_cost_base: protocol_config
                    .ecdsa_k1_ecrecover_keccak256_cost_base()
                    .into(),
                ecdsa_k1_ecrecover_keccak256_msg_cost_per_byte: protocol_config
                    .ecdsa_k1_ecrecover_keccak256_msg_cost_per_byte()
                    .into(),
                ecdsa_k1_ecrecover_keccak256_msg_cost_per_block: protocol_config
                    .ecdsa_k1_ecrecover_keccak256_msg_cost_per_block()
                    .into(),
                ecdsa_k1_ecrecover_sha256_cost_base: protocol_config
                    .ecdsa_k1_ecrecover_sha256_cost_base()
                    .into(),
                ecdsa_k1_ecrecover_sha256_msg_cost_per_byte: protocol_config
                    .ecdsa_k1_ecrecover_sha256_msg_cost_per_byte()
                    .into(),
                ecdsa_k1_ecrecover_sha256_msg_cost_per_block: protocol_config
                    .ecdsa_k1_ecrecover_sha256_msg_cost_per_block()
                    .into(),
            },
            ecdsa_k1_decompress_pubkey_cost_params: EcdsaK1DecompressPubkeyCostParams {
                ecdsa_k1_decompress_pubkey_cost_base: protocol_config
                    .ecdsa_k1_decompress_pubkey_cost_base()
                    .into(),
            },
            ecdsa_k1_secp256k1_verify_cost_params: EcdsaK1Secp256k1VerifyCostParams {
                ecdsa_k1_secp256k1_verify_keccak256_cost_base: protocol_config
                    .ecdsa_k1_secp256k1_verify_keccak256_cost_base()
                    .into(),
                ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_byte: protocol_config
                    .ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_byte()
                    .into(),
                ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_block: protocol_config
                    .ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_block()
                    .into(),
                ecdsa_k1_secp256k1_verify_sha256_cost_base: protocol_config
                    .ecdsa_k1_secp256k1_verify_sha256_cost_base()
                    .into(),
                ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_byte: protocol_config
                    .ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_byte()
                    .into(),
                ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_block: protocol_config
                    .ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_block()
                    .into(),
            },
            ecdsa_r1_ecrecover_cost_params: EcdsaR1EcrecoverCostParams {
                ecdsa_r1_ecrecover_keccak256_cost_base: protocol_config
                    .ecdsa_r1_ecrecover_keccak256_cost_base()
                    .into(),
                ecdsa_r1_ecrecover_keccak256_msg_cost_per_byte: protocol_config
                    .ecdsa_r1_ecrecover_keccak256_msg_cost_per_byte()
                    .into(),
                ecdsa_r1_ecrecover_keccak256_msg_cost_per_block: protocol_config
                    .ecdsa_r1_ecrecover_keccak256_msg_cost_per_block()
                    .into(),
                ecdsa_r1_ecrecover_sha256_cost_base: protocol_config
                    .ecdsa_r1_ecrecover_sha256_cost_base()
                    .into(),
                ecdsa_r1_ecrecover_sha256_msg_cost_per_byte: protocol_config
                    .ecdsa_r1_ecrecover_sha256_msg_cost_per_byte()
                    .into(),
                ecdsa_r1_ecrecover_sha256_msg_cost_per_block: protocol_config
                    .ecdsa_r1_ecrecover_sha256_msg_cost_per_block()
                    .into(),
            },
            ecdsa_r1_secp256_r1_verify_cost_params: EcdsaR1Secp256R1VerifyCostParams {
                ecdsa_r1_secp256r1_verify_keccak256_cost_base: protocol_config
                    .ecdsa_r1_secp256r1_verify_keccak256_cost_base()
                    .into(),
                ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_byte: protocol_config
                    .ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_byte()
                    .into(),
                ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_block: protocol_config
                    .ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_block()
                    .into(),
                ecdsa_r1_secp256r1_verify_sha256_cost_base: protocol_config
                    .ecdsa_r1_secp256r1_verify_sha256_cost_base()
                    .into(),
                ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_byte: protocol_config
                    .ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_byte()
                    .into(),
                ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_block: protocol_config
                    .ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_block()
                    .into(),
            },
            ecvrf_ecvrf_verify_cost_params: EcvrfEcvrfVerifyCostParams {
                ecvrf_ecvrf_verify_cost_base: protocol_config.ecvrf_ecvrf_verify_cost_base().into(),
                ecvrf_ecvrf_verify_alpha_string_cost_per_byte: protocol_config
                    .ecvrf_ecvrf_verify_alpha_string_cost_per_byte()
                    .into(),
                ecvrf_ecvrf_verify_alpha_string_cost_per_block: protocol_config
                    .ecvrf_ecvrf_verify_alpha_string_cost_per_block()
                    .into(),
            },
            groth16_prepare_verifying_key_cost_params: Groth16PrepareVerifyingKeyCostParams {
                groth16_prepare_verifying_key_bls12381_cost_base: protocol_config
                    .groth16_prepare_verifying_key_bls12381_cost_base()
                    .into(),
                groth16_prepare_verifying_key_bn254_cost_base: protocol_config
                    .groth16_prepare_verifying_key_bn254_cost_base()
                    .into(),
            },
            groth16_verify_groth16_proof_internal_cost_params:
                Groth16VerifyGroth16ProofInternalCostParams {
                    groth16_verify_groth16_proof_internal_bls12381_cost_base: protocol_config
                        .groth16_verify_groth16_proof_internal_bls12381_cost_base()
                        .into(),
                    groth16_verify_groth16_proof_internal_bls12381_cost_per_public_input:
                        protocol_config
                            .groth16_verify_groth16_proof_internal_bls12381_cost_per_public_input()
                            .into(),
                    groth16_verify_groth16_proof_internal_bn254_cost_base: protocol_config
                        .groth16_verify_groth16_proof_internal_bn254_cost_base()
                        .into(),
                    groth16_verify_groth16_proof_internal_bn254_cost_per_public_input:
                        protocol_config
                            .groth16_verify_groth16_proof_internal_bn254_cost_per_public_input()
                            .into(),
                    groth16_verify_groth16_proof_internal_public_input_cost_per_byte:
                        protocol_config
                            .groth16_verify_groth16_proof_internal_public_input_cost_per_byte()
                            .into(),
                },
            hmac_hmac_sha3_256_cost_params: HmacHmacSha3256CostParams {
                hmac_hmac_sha3_256_cost_base: protocol_config.hmac_hmac_sha3_256_cost_base().into(),
                hmac_hmac_sha3_256_input_cost_per_byte: protocol_config
                    .hmac_hmac_sha3_256_input_cost_per_byte()
                    .into(),
                hmac_hmac_sha3_256_input_cost_per_block: protocol_config
                    .hmac_hmac_sha3_256_input_cost_per_block()
                    .into(),
            },
            transfer_receive_object_internal_cost_params: TransferReceiveObjectInternalCostParams {
                transfer_receive_object_internal_cost_base: protocol_config
                    .transfer_receive_object_cost_base_as_option()
                    .unwrap_or(0)
                    .into(),
            },
            check_zklogin_id_cost_params: CheckZkloginIdCostParams {
                check_zklogin_id_cost_base: protocol_config
                    .check_zklogin_id_cost_base_as_option()
                    .map(Into::into),
            },
            check_zklogin_issuer_cost_params: CheckZkloginIssuerCostParams {
                check_zklogin_issuer_cost_base: protocol_config
                    .check_zklogin_issuer_cost_base_as_option()
                    .map(Into::into),
            },
            poseidon_bn254_cost_params: PoseidonBN254CostParams {
                poseidon_bn254_cost_base: protocol_config
                    .poseidon_bn254_cost_base_as_option()
                    .map(Into::into),
                poseidon_bn254_data_cost_per_block: protocol_config
                    .poseidon_bn254_cost_per_block_as_option()
                    .map(Into::into),
            },
        }
    }
}

pub fn all_natives(silent: bool) -> NativeFunctionTable {
    let sui_framework_natives: &[(&str, &str, NativeFunction)] = &[
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
        ("hmac", "hmac_sha3_256", make_native!(hmac::hmac_sha3_256)),
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
            "transfer",
            "receive_impl",
            make_native!(transfer::receive_object_internal),
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
        ("test_utils", "destroy", make_native!(test_utils::destroy)),
        (
            "test_utils",
            "create_one_time_witness",
            make_native!(test_utils::create_one_time_witness),
        ),
        (
            "zklogin_verified_id",
            "check_zklogin_id_internal",
            make_native!(zklogin::check_zklogin_id_internal),
        ),
        (
            "zklogin_verified_issuer",
            "check_zklogin_issuer_internal",
            make_native!(zklogin::check_zklogin_issuer_internal),
        ),
        (
            "poseidon",
            "poseidon_bn254",
            make_native!(poseidon::poseidon_bn254),
        ),
    ];
    let sui_framework_natives_iter =
        sui_framework_natives
            .iter()
            .cloned()
            .map(|(module_name, func_name, func)| {
                (
                    SUI_FRAMEWORK_ADDRESS,
                    Identifier::new(module_name).unwrap(),
                    Identifier::new(func_name).unwrap(),
                    func,
                )
            });
    let sui_system_natives: &[(&str, &str, NativeFunction)] = &[(
        "validator",
        "validate_metadata_bcs",
        make_native!(validator::validate_metadata_bcs),
    )];
    sui_system_natives
        .iter()
        .cloned()
        .map(|(module_name, func_name, func)| {
            (
                SUI_SYSTEM_ADDRESS,
                Identifier::new(module_name).unwrap(),
                Identifier::new(func_name).unwrap(),
                func,
            )
        })
        .chain(sui_framework_natives_iter)
        .chain(move_stdlib::natives::all_natives(
            MOVE_STDLIB_ADDRESS,
            // TODO: tune gas params
            GasParameters::zeros(),
        ))
        .chain(move_stdlib::natives::nursery_natives(
            silent,
            MOVE_STDLIB_ADDRESS,
            // TODO: tune gas params
            NurseryGasParameters::zeros(),
        ))
        .collect()
}

// ID { bytes: address }
// Extract the first field of the struct to get the address bytes.
pub fn get_receiver_object_id(object: Value) -> Result<Value, PartialVMError> {
    get_nested_struct_field(object, &[0])
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

/// Returns the struct tag, non-annotated type layout, and fully annotated type layout of `ty`.
pub(crate) fn get_tag_and_layouts(
    context: &NativeContext,
    ty: &Type,
) -> PartialVMResult<Option<(StructTag, R::MoveTypeLayout, A::MoveTypeLayout)>> {
    let tag = match context.type_to_type_tag(ty)? {
        TypeTag::Struct(s) => s,
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };
    let Some(layout) = context.type_to_type_layout(ty)? else {
        return Ok(None);
    };
    let Some(annotated_layout) = context.type_to_fully_annotated_layout(ty)? else {
        return Ok(None);
    };
    Ok(Some((*tag, layout, annotated_layout)))
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

pub(crate) fn legacy_test_cost() -> InternalGas {
    InternalGas::new(0)
}
