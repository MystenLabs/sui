// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::ObjectID;
use crate::committee::CommitteeWithNetworkMetadata;
use crate::dynamic_field::{
    get_dynamic_field_from_store, get_dynamic_field_object_from_store, Field,
};
use crate::error::SuiError;
use crate::object::{MoveObject, Object};
use crate::storage::ObjectStore;
use crate::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;
use crate::sui_system_state::sui_system_state_inner_v2::SuiSystemStateInnerV2;
use crate::versioned::Versioned;
use crate::{id::UID, MoveTypeTagTrait, SUI_SYSTEM_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID};
use anyhow::Result;
use enum_dispatch::enum_dispatch;
use move_core_types::{ident_str, identifier::IdentStr, language_storage::StructTag};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};

use self::sui_system_state_inner_v1::{SuiSystemStateInnerV1, ValidatorV1};
use self::sui_system_state_summary::{SuiSystemStateSummary, SuiValidatorSummary};

pub mod epoch_start_sui_system_state;
pub mod sui_system_state_inner_v1;
pub mod sui_system_state_inner_v2;
pub mod sui_system_state_summary;

#[cfg(msim)]
mod simtest_sui_system_state_inner;
#[cfg(msim)]
use self::simtest_sui_system_state_inner::{
    SimTestSuiSystemStateInnerDeepV2, SimTestSuiSystemStateInnerShallowV2,
    SimTestSuiSystemStateInnerV1, SimTestValidatorDeepV2, SimTestValidatorV1,
};

const SUI_SYSTEM_STATE_WRAPPER_STRUCT_NAME: &IdentStr = ident_str!("SuiSystemState");

pub const SUI_SYSTEM_MODULE_NAME: &IdentStr = ident_str!("sui_system");
pub const ADVANCE_EPOCH_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch");
pub const ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch_safe_mode");

#[cfg(msim)]
pub const SUI_SYSTEM_STATE_SIM_TEST_V1: u64 = 18446744073709551605; // u64::MAX - 10
#[cfg(msim)]
pub const SUI_SYSTEM_STATE_SIM_TEST_SHALLOW_V2: u64 = 18446744073709551606; // u64::MAX - 9
#[cfg(msim)]
pub const SUI_SYSTEM_STATE_SIM_TEST_DEEP_V2: u64 = 18446744073709551607; // u64::MAX - 8

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
            address: SUI_SYSTEM_ADDRESS,
            name: SUI_SYSTEM_STATE_WRAPPER_STRUCT_NAME.to_owned(),
            module: SUI_SYSTEM_MODULE_NAME.to_owned(),
            type_params: vec![],
        }
    }

    pub fn advance_epoch_safe_mode<S>(
        &self,
        params: &AdvanceEpochParams,
        object_store: &S,
        protocol_config: &ProtocolConfig,
    ) -> Object
    where
        S: ObjectStore,
    {
        let id = self.id.id.bytes;
        let mut field_object = get_dynamic_field_object_from_store(object_store, id, &self.version)
            .expect("Dynamic field object of wrapper should always be present in the object store");
        let move_object = field_object
            .data
            .try_as_move_mut()
            .expect("Dynamic field object must be a Move object");
        match self.version {
            1 => {
                Self::advance_epoch_safe_mode_impl::<SuiSystemStateInnerV1>(
                    move_object,
                    params,
                    protocol_config,
                );
            }
            2 => {
                Self::advance_epoch_safe_mode_impl::<SuiSystemStateInnerV2>(
                    move_object,
                    params,
                    protocol_config,
                );
            }
            #[cfg(msim)]
            SUI_SYSTEM_STATE_SIM_TEST_V1 => {
                Self::advance_epoch_safe_mode_impl::<SimTestSuiSystemStateInnerV1>(
                    move_object,
                    params,
                    protocol_config,
                );
            }
            #[cfg(msim)]
            SUI_SYSTEM_STATE_SIM_TEST_SHALLOW_V2 => {
                Self::advance_epoch_safe_mode_impl::<SimTestSuiSystemStateInnerShallowV2>(
                    move_object,
                    params,
                    protocol_config,
                );
            }
            #[cfg(msim)]
            SUI_SYSTEM_STATE_SIM_TEST_DEEP_V2 => {
                Self::advance_epoch_safe_mode_impl::<SimTestSuiSystemStateInnerDeepV2>(
                    move_object,
                    params,
                    protocol_config,
                );
            }
            _ => unreachable!(),
        }
        field_object
    }

    fn advance_epoch_safe_mode_impl<T>(
        move_object: &mut MoveObject,
        params: &AdvanceEpochParams,
        protocol_config: &ProtocolConfig,
    ) where
        T: Serialize + DeserializeOwned + SuiSystemStateTrait,
    {
        let mut field: Field<u64, T> =
            bcs::from_bytes(move_object.contents()).expect("bcs deserialization should never fail");
        tracing::info!(
            "Advance epoch safe mode: current epoch: {}, protocol_version: {}, system_state_version: {}",
            field.value.epoch(),
            field.value.protocol_version(),
            field.value.system_state_version()
        );
        field.value.advance_epoch_safe_mode(params);
        tracing::info!(
            "Safe mode activated. New epoch: {}, protocol_version: {}, system_state_version: {}",
            field.value.epoch(),
            field.value.protocol_version(),
            field.value.system_state_version()
        );
        let new_contents = bcs::to_bytes(&field).expect("bcs serialization should never fail");
        move_object
            .update_contents(new_contents, protocol_config)
            .expect("Update sui system object content cannot fail since it should be small");
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
    fn advance_epoch_safe_mode(&mut self, params: &AdvanceEpochParams);
    fn get_current_epoch_committee(&self) -> CommitteeWithNetworkMetadata;
    fn get_pending_active_validators<S: ObjectStore>(
        &self,
        object_store: &S,
    ) -> Result<Vec<SuiValidatorSummary>, SuiError>;
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
    V2(SuiSystemStateInnerV2),
    #[cfg(msim)]
    SimTestV1(SimTestSuiSystemStateInnerV1),
    #[cfg(msim)]
    SimTestShallowV2(SimTestSuiSystemStateInnerShallowV2),
    #[cfg(msim)]
    SimTestDeepV2(SimTestSuiSystemStateInnerDeepV2),
}

/// This is the fixed type used by genesis.
pub type SuiSystemStateInnerGenesis = SuiSystemStateInnerV1;
pub type SuiValidatorGenesis = ValidatorV1;

impl SuiSystemState {
    /// Always return the version that we will be using for genesis.
    /// Genesis always uses this version regardless of the current version.
    /// Note that since it's possible for the actual genesis of the network to diverge from the
    /// genesis of the latest Rust code, it's important that we only use this for tooling purposes.
    pub fn into_genesis_version_for_tooling(self) -> SuiSystemStateInnerGenesis {
        match self {
            SuiSystemState::V1(inner) => inner,
            _ => unreachable!(),
        }
    }

    pub fn version(&self) -> u64 {
        self.system_state_version()
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

pub fn get_sui_system_state<S>(object_store: &S) -> Result<SuiSystemState, SuiError>
where
    S: ObjectStore,
{
    let wrapper = get_sui_system_state_wrapper(object_store)?;
    let id = wrapper.id.id.bytes;
    match wrapper.version {
        1 => {
            let result: SuiSystemStateInnerV1 =
                get_dynamic_field_from_store(object_store, id, &wrapper.version).map_err(
                    |err| {
                        SuiError::DynamicFieldReadError(format!(
                            "Failed to load sui system state inner object with ID {:?} and version {:?}: {:?}",
                            id, wrapper.version, err
                        ))
                    },
                )?;
            Ok(SuiSystemState::V1(result))
        }
        2 => {
            let result: SuiSystemStateInnerV2 =
                get_dynamic_field_from_store(object_store, id, &wrapper.version).map_err(
                    |err| {
                        SuiError::DynamicFieldReadError(format!(
                            "Failed to load sui system state inner object with ID {:?} and version {:?}: {:?}",
                            id, wrapper.version, err
                        ))
                    },
                )?;
            Ok(SuiSystemState::V2(result))
        }
        #[cfg(msim)]
        SUI_SYSTEM_STATE_SIM_TEST_V1 => {
            let result: SimTestSuiSystemStateInnerV1 =
                get_dynamic_field_from_store(object_store, id, &wrapper.version).map_err(
                    |err| {
                        SuiError::DynamicFieldReadError(format!(
                            "Failed to load sui system state inner object with ID {:?} and version {:?}: {:?}",
                            id, wrapper.version, err
                        ))
                    },
                )?;
            Ok(SuiSystemState::SimTestV1(result))
        }
        #[cfg(msim)]
        SUI_SYSTEM_STATE_SIM_TEST_SHALLOW_V2 => {
            let result: SimTestSuiSystemStateInnerShallowV2 =
                get_dynamic_field_from_store(object_store, id, &wrapper.version).map_err(
                    |err| {
                        SuiError::DynamicFieldReadError(format!(
                            "Failed to load sui system state inner object with ID {:?} and version {:?}: {:?}",
                            id, wrapper.version, err
                        ))
                    },
                )?;
            Ok(SuiSystemState::SimTestShallowV2(result))
        }
        #[cfg(msim)]
        SUI_SYSTEM_STATE_SIM_TEST_DEEP_V2 => {
            let result: SimTestSuiSystemStateInnerDeepV2 =
                get_dynamic_field_from_store(object_store, id, &wrapper.version).map_err(
                    |err| {
                        SuiError::DynamicFieldReadError(format!(
                            "Failed to load sui system state inner object with ID {:?} and version {:?}: {:?}",
                            id, wrapper.version, err
                        ))
                    },
                )?;
            Ok(SuiSystemState::SimTestDeepV2(result))
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
    object_store: &S,
    table_id: ObjectID,
    key: &K,
) -> Result<SuiValidatorSummary, SuiError>
where
    S: ObjectStore,
    K: MoveTypeTagTrait + Serialize + DeserializeOwned + fmt::Debug,
{
    let field: ValidatorWrapper = get_dynamic_field_from_store(object_store, table_id, key)
        .map_err(|err| {
            SuiError::SuiSystemStateReadError(format!(
                "Failed to load validator wrapper from table: {:?}",
                err
            ))
        })?;
    let versioned = field.inner;
    let version = versioned.version;
    match version {
        1 => {
            let validator: ValidatorV1 =
                get_dynamic_field_from_store(object_store, versioned.id.id.bytes, &version)
                    .map_err(|err| {
                        SuiError::SuiSystemStateReadError(format!(
                            "Failed to load inner validator from the wrapper: {:?}",
                            err
                        ))
                    })?;
            Ok(validator.into_sui_validator_summary())
        }
        #[cfg(msim)]
        SUI_SYSTEM_STATE_SIM_TEST_V1 => {
            let validator: SimTestValidatorV1 =
                get_dynamic_field_from_store(object_store, versioned.id.id.bytes, &version)
                    .map_err(|err| {
                        SuiError::SuiSystemStateReadError(format!(
                            "Failed to load inner validator from the wrapper: {:?}",
                            err
                        ))
                    })?;
            Ok(validator.into_sui_validator_summary())
        }
        #[cfg(msim)]
        SUI_SYSTEM_STATE_SIM_TEST_DEEP_V2 => {
            let validator: SimTestValidatorDeepV2 =
                get_dynamic_field_from_store(object_store, versioned.id.id.bytes, &version)
                    .map_err(|err| {
                        SuiError::SuiSystemStateReadError(format!(
                            "Failed to load inner validator from the wrapper: {:?}",
                            err
                        ))
                    })?;
            Ok(validator.into_sui_validator_summary())
        }
        _ => Err(SuiError::SuiSystemStateReadError(format!(
            "Unsupported Validator version: {}",
            version
        ))),
    }
}

pub fn get_validators_from_table_vec<S, ValidatorType>(
    object_store: &S,
    table_id: ObjectID,
    table_size: u64,
) -> Result<Vec<ValidatorType>, SuiError>
where
    S: ObjectStore,
    ValidatorType: Serialize + DeserializeOwned,
{
    let mut validators = vec![];
    for i in 0..table_size {
        let validator: ValidatorType = get_dynamic_field_from_store(object_store, table_id, &i)
            .map_err(|err| {
                SuiError::SuiSystemStateReadError(format!(
                    "Failed to load validator from table: {:?}",
                    err
                ))
            })?;
        validators.push(validator);
    }
    Ok(validators)
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Default)]
pub struct PoolTokenExchangeRate {
    sui_amount: u64,
    pool_token_amount: u64,
}

impl PoolTokenExchangeRate {
    /// Rate of the staking pool, pool token amount : Sui amount
    pub fn rate(&self) -> f64 {
        if self.sui_amount == 0 {
            1_f64
        } else {
            self.pool_token_amount as f64 / self.sui_amount as f64
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct ValidatorWrapper {
    pub inner: Versioned,
}

#[derive(Debug)]
pub struct AdvanceEpochParams {
    pub epoch: u64,
    pub next_protocol_version: ProtocolVersion,
    pub storage_charge: u64,
    pub computation_charge: u64,
    pub storage_rebate: u64,
    pub non_refundable_storage_fee: u64,
    pub storage_fund_reinvest_rate: u64,
    pub reward_slashing_rate: u64,
    pub epoch_start_timestamp_ms: u64,
}

#[cfg(msim)]
pub mod advance_epoch_result_injection {
    use crate::{
        committee::EpochId,
        error::{ExecutionError, ExecutionErrorKind},
    };
    use std::cell::RefCell;

    thread_local! {
        /// Override the result of advance_epoch in the range [start, end).
        static OVERRIDE: RefCell<Option<(EpochId, EpochId)>>  = RefCell::new(None);
    }

    /// Override the result of advance_epoch transaction if new epoch is in the provided range [start, end).
    pub fn set_override(value: Option<(EpochId, EpochId)>) {
        OVERRIDE.with(|o| *o.borrow_mut() = value);
    }

    /// This function is used to modify the result of advance_epoch transaction for testing.
    /// If the override is set, the result will be an execution error, otherwise the original result will be returned.
    pub fn maybe_modify_result(
        result: Result<(), ExecutionError>,
        current_epoch: EpochId,
    ) -> Result<(), ExecutionError> {
        if let Some((start, end)) = OVERRIDE.with(|o| *o.borrow()) {
            if current_epoch >= start && current_epoch < end {
                return Err::<(), ExecutionError>(ExecutionError::new(
                    ExecutionErrorKind::FunctionNotFound,
                    None,
                ));
            }
        }
        result
    }
}
