// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::ObjectID;
use crate::committee::{CommitteeWithNetworkMetadata, EpochId, ProtocolVersion};
use crate::dynamic_field::get_dynamic_field_from_store;
use crate::error::SuiError;
use crate::storage::ObjectStore;
use crate::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;
use crate::versioned::Versioned;
use crate::{id::UID, MoveTypeTagTrait, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID};
use anyhow::Result;
use enum_dispatch::enum_dispatch;
use move_core_types::{ident_str, identifier::IdentStr, language_storage::StructTag};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use self::sui_system_state_inner_v1::{SuiSystemStateInnerV1, ValidatorV1};
use self::sui_system_state_summary::{SuiSystemStateSummary, SuiValidatorSummary};

pub mod epoch_start_sui_system_state;
pub mod sui_system_state_inner_v1;
pub mod sui_system_state_summary;

const SUI_SYSTEM_STATE_WRAPPER_STRUCT_NAME: &IdentStr = ident_str!("SuiSystemState");

pub const SUI_SYSTEM_MODULE_NAME: &IdentStr = ident_str!("sui_system");
pub const ADVANCE_EPOCH_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch");
pub const ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch_safe_mode");

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
    fn system_state_version(&self) -> u64;
    fn epoch_start_timestamp_ms(&self) -> u64;
    fn epoch_duration_ms(&self) -> u64;
    fn safe_mode(&self) -> bool;
    fn get_current_epoch_committee(&self) -> CommitteeWithNetworkMetadata;
    fn into_epoch_start_state(self) -> EpochStartSystemState;
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
pub type SuiValidatorGenesis = ValidatorV1;

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
        SuiSystemState::V1(SuiSystemStateInnerV1::new_for_testing(epoch))
    }

    pub fn version(&self) -> u64 {
        self.system_state_version()
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
    let wrapper = object_store
        .get_object(&SUI_SYSTEM_STATE_OBJECT_ID)?
        // Don't panic here on None because object_store is a generic store.
        .ok_or_else(|| {
            SuiError::SuiSystemStateReadError("SuiSystemStateWrapper object not found".to_owned())
        })?;
    let move_object = wrapper.data.try_as_move().ok_or_else(|| {
        SuiError::SuiSystemStateReadError(
            "SuiSystemStateWrapper object must be a Move object".to_owned(),
        )
    })?;
    let result = bcs::from_bytes::<SuiSystemStateWrapper>(move_object.contents())
        .map_err(|err| SuiError::SuiSystemStateReadError(err.to_string()))?;
    Ok(result)
}

// This version is used to support authority_tests::test_sui_system_state_nop_upgrade.
pub const SUI_SYSTEM_STATE_TESTING_VERSION1: u64 = u64::MAX;

pub fn get_sui_system_state<S>(object_store: &S) -> Result<SuiSystemState, SuiError>
where
    S: ObjectStore,
{
    let wrapper = get_sui_system_state_wrapper(object_store)?;
    match wrapper.version {
        1 => {
            let result: SuiSystemStateInnerV1 =
                get_dynamic_field_from_store(object_store, wrapper.id.id.bytes, &wrapper.version)?;
            Ok(SuiSystemState::V1(result))
        }
        // The following case is for sim_test only to support authority_tests::test_sui_system_state_nop_upgrade.
        #[cfg(msim)]
        SUI_SYSTEM_STATE_TESTING_VERSION1 => {
            let result: SuiSystemStateInnerV1 =
                get_dynamic_field_from_store(object_store, wrapper.id.id.bytes, &wrapper.version)?;
            Ok(SuiSystemState::V1(result))
        }
        _ => Err(SuiError::SuiSystemStateReadError(format!(
            "Unsupported SuiSystemState version: {}",
            wrapper.version
        ))),
    }
}

/// Given a system state type version, and the ID of the table, along with a key, retrieve the
/// dynamic field as a Validator type. We need the version to determine which inner type to use for
/// the Validator type. This is assuming that the validator is stored in the table as
/// ValidatorWrapper type.
pub fn get_validator_from_table<S, K>(
    system_state_version: u64,
    object_store: &S,
    table_id: ObjectID,
    key: &K,
) -> Result<SuiValidatorSummary, SuiError>
where
    S: ObjectStore,
    K: MoveTypeTagTrait + Serialize + DeserializeOwned,
{
    let field: ValidatorWrapper = get_dynamic_field_from_store(object_store, table_id, key)?;
    let versioned = field.inner;
    match system_state_version {
        1 => {
            let validator: ValidatorV1 = get_dynamic_field_from_store(
                object_store,
                versioned.id.id.bytes,
                &system_state_version,
            )?;
            Ok(validator.into_sui_validator_summary())
        }
        _ => Err(SuiError::SuiSystemStateReadError(format!(
            "Unsupported SuiSystemState version: {}",
            system_state_version
        ))),
    }
}

pub fn get_sui_system_state_version(_protocol_version: ProtocolVersion) -> u64 {
    INIT_SYSTEM_STATE_VERSION
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct PoolTokenExchangeRate {
    sui_amount: u64,
    pool_token_amount: u64,
}

impl PoolTokenExchangeRate {
    /// Rate of the staking pool, pool token amount : Sui amount
    pub fn rate(&self) -> f64 {
        if self.sui_amount == 0 {
            0 as f64
        } else {
            self.pool_token_amount as f64 / self.sui_amount as f64
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct ValidatorWrapper {
    pub inner: Versioned,
}
