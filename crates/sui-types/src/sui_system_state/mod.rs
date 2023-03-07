// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{AuthorityName, SuiAddress};
use crate::committee::{CommitteeWithNetworkMetadata, EpochId, ProtocolVersion};
use crate::dynamic_field::{derive_dynamic_field_id, Field};
use crate::error::SuiError;
use crate::storage::ObjectStore;
use crate::{id::UID, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID};
use anemo::PeerId;
use anyhow::Result;
use enum_dispatch::enum_dispatch;
use move_core_types::language_storage::TypeTag;
use move_core_types::value::MoveTypeLayout;
use move_core_types::{ident_str, identifier::IdentStr, language_storage::StructTag};
use move_vm_types::values::Value;
use multiaddr::Multiaddr;
use narwhal_config::{Committee as NarwhalCommittee, WorkerCache};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use tracing::error;

use self::sui_system_state_inner_v1::{SuiSystemStateInnerV1, ValidatorMetadata};
use self::sui_system_state_summary::SuiSystemStateSummary;

pub mod sui_system_state_inner_v1;
pub mod sui_system_state_summary;

const SUI_SYSTEM_STATE_WRAPPER_STRUCT_NAME: &IdentStr = ident_str!("SuiSystemState");

pub const SUI_SYSTEM_MODULE_NAME: &IdentStr = ident_str!("sui_system");
pub const ADVANCE_EPOCH_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch");
pub const ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch_safe_mode");
pub const CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME: &IdentStr =
    ident_str!("consensus_commit_prologue");

pub const INIT_SYSTEM_STATE_VERSION: u64 = 1;

/// Rust version of the Move sui::sui_system::SuiSystemState type
/// This repreents the object with 0x5 ID.
/// In Rust, this type should be rarely used since it's just a thin
/// wrapper used to access the inner object.
/// Within this module, we use it to determine the current version of the system state inner object type,
/// so that we could deserialize the inner object correctly.
/// Outside of this module, we only use it in genesis snapshot and testing.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SuiSystemStateWrapper {
    pub id: UID,
    pub version: u64,
}

impl SuiSystemStateWrapper {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: SUI_SYSTEM_STATE_WRAPPER_STRUCT_NAME.to_owned(),
            module: SUI_SYSTEM_MODULE_NAME.to_owned(),
            type_params: vec![],
        }
    }
}

/// This is the standard API that all inner system state object type should implement.
#[enum_dispatch]
pub trait SuiSystemStateTrait {
    fn epoch(&self) -> u64;
    fn reference_gas_price(&self) -> u64;
    fn protocol_version(&self) -> u64;
    fn epoch_start_timestamp_ms(&self) -> u64;
    fn safe_mode(&self) -> bool;
    fn get_current_epoch_committee(&self) -> CommitteeWithNetworkMetadata;
    fn get_current_epoch_narwhal_committee(&self) -> NarwhalCommittee;
    fn get_current_epoch_narwhal_worker_cache(
        &self,
        transactions_address: &Multiaddr,
    ) -> WorkerCache;
    fn get_validator_metadata_vec(&self) -> Vec<ValidatorMetadata>;
    fn get_current_epoch_authority_names_to_peer_ids(&self) -> HashMap<AuthorityName, PeerId>;
    fn get_staking_pool_info(&self) -> BTreeMap<SuiAddress, (Vec<u8>, u64)>;
    fn into_sui_system_state_summary(self) -> SuiSystemStateSummary;
}

/// SuiSystemState provides an abstraction over multiple versions of the inner SuiSystemStateInner object.
/// This should be the primary interface to the system state object in Rust.
/// We use enum dispatch to dispatch all methods defined in SuiSystemStateTrait to the actual
/// implementation in the inner types.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[enum_dispatch(SuiSystemStateTrait)]
pub enum SuiSystemState {
    V1(SuiSystemStateInnerV1),
}

/// This is the fixed type used by genesis.
pub type SuiSystemStateInnerGenesis = SuiSystemStateInnerV1;

/// This is the fixed type used by benchmarking.
pub type SuiSystemStateInnerBenchmark = SuiSystemStateInnerV1;

impl SuiSystemState {
    pub fn new_genesis(inner: SuiSystemStateInnerGenesis) -> Self {
        Self::V1(inner)
    }

    /// Always return the version that we will be using for genesis.
    /// Genesis always uses this version regardless of the current version.
    pub fn into_genesis_version(self) -> SuiSystemStateInnerGenesis {
        match self {
            SuiSystemState::V1(inner) => inner,
        }
    }

    pub fn into_benchmark_version(self) -> SuiSystemStateInnerBenchmark {
        match self {
            SuiSystemState::V1(inner) => inner,
        }
    }

    pub fn new_for_benchmarking(inner: SuiSystemStateInnerBenchmark) -> Self {
        Self::V1(inner)
    }

    pub fn new_for_testing(epoch: EpochId) -> Self {
        SuiSystemState::V1(SuiSystemStateInnerV1 {
            epoch,
            ..Default::default()
        })
    }
}

impl Default for SuiSystemState {
    fn default() -> Self {
        SuiSystemState::V1(SuiSystemStateInnerV1::default())
    }
}

pub fn get_sui_system_state_wrapper<S>(object_store: &S) -> Result<SuiSystemStateWrapper, SuiError>
where
    S: ObjectStore,
{
    let sui_system_object = object_store
        .get_object(&SUI_SYSTEM_STATE_OBJECT_ID)?
        .ok_or(SuiError::SuiSystemStateNotFound)?;
    let move_object = sui_system_object
        .data
        .try_as_move()
        .ok_or(SuiError::SuiSystemStateNotFound)?;
    let result = bcs::from_bytes::<SuiSystemStateWrapper>(move_object.contents())
        .expect("Sui System State object deserialization cannot fail");
    Ok(result)
}

// This version is used to support authority_tests::test_sui_system_state_nop_upgrade.
pub const SUI_SYSTEM_STATE_TESTING_VERSION1: u64 = u64::MAX;

pub fn get_sui_system_state<S>(object_store: &S) -> Result<SuiSystemState, SuiError>
where
    S: ObjectStore,
{
    let wrapper = get_sui_system_state_wrapper(object_store)?;
    let inner_id = derive_dynamic_field_id(
        wrapper.id.id.bytes,
        &TypeTag::U64,
        &MoveTypeLayout::U64,
        &Value::u64(wrapper.version),
    )
    .expect("Sui System State object must exist");
    let inner = object_store
        .get_object(&inner_id)?
        .ok_or(SuiError::SuiSystemStateNotFound)?;
    let move_object = inner
        .data
        .try_as_move()
        .ok_or(SuiError::SuiSystemStateNotFound)?;
    match wrapper.version {
        1 => {
            let result =
                bcs::from_bytes::<Field<u64, SuiSystemStateInnerV1>>(move_object.contents())
                    .expect("Sui System State object deserialization cannot fail");
            Ok(SuiSystemState::V1(result.value))
        }
        // The following case is for sim_test only to support authority_tests::test_sui_system_state_nop_upgrade.
        #[cfg(msim)]
        SUI_SYSTEM_STATE_TESTING_VERSION1 => {
            let result =
                bcs::from_bytes::<Field<u64, SuiSystemStateInnerV1>>(move_object.contents())
                    .expect("Sui System State object deserialization cannot fail");
            Ok(SuiSystemState::V1(result.value))
        }
        _ => {
            error!("Unsupported Sui System State version: {}", wrapper.version);
            Err(SuiError::SuiSystemStateUnexpectedVersion)
        }
    }
}

pub fn get_sui_system_state_version(_protocol_version: ProtocolVersion) -> u64 {
    INIT_SYSTEM_STATE_VERSION
}
