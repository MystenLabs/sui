// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use sui_config::{ExecutionCacheConfig, NodeConfig};

use std::fmt;
use sui_types::authenticator_state::get_authenticator_state_obj_initial_shared_version;
use sui_types::base_types::SequenceNumber;
use sui_types::bridge::{get_bridge_obj_initial_shared_version, is_bridge_committee_initiated};
use sui_types::deny_list_v1::get_deny_list_obj_initial_shared_version;
use sui_types::epoch_data::EpochData;
use sui_types::error::SuiResult;
use sui_types::messages_checkpoint::{CheckpointDigest, CheckpointTimestamp};
use sui_types::randomness_state::get_randomness_state_obj_initial_shared_version;
use sui_types::storage::ObjectStore;
use sui_types::sui_system_state::epoch_start_sui_system_state::{
    EpochStartSystemState, EpochStartSystemStateTrait,
};

use crate::execution_cache::{choose_execution_cache, ExecutionCacheConfigType};

#[enum_dispatch]
pub trait EpochStartConfigTrait {
    fn epoch_digest(&self) -> CheckpointDigest;
    fn epoch_start_state(&self) -> &EpochStartSystemState;
    fn flags(&self) -> &[EpochFlag];
    fn authenticator_obj_initial_shared_version(&self) -> Option<SequenceNumber>;
    fn randomness_obj_initial_shared_version(&self) -> Option<SequenceNumber>;
    fn coin_deny_list_obj_initial_shared_version(&self) -> Option<SequenceNumber>;
    fn bridge_obj_initial_shared_version(&self) -> Option<SequenceNumber>;
    fn bridge_committee_initiated(&self) -> bool;

    fn execution_cache_type(&self) -> ExecutionCacheConfigType {
        if self.flags().contains(&EpochFlag::WritebackCacheEnabled) {
            ExecutionCacheConfigType::WritebackCache
        } else {
            ExecutionCacheConfigType::PassthroughCache
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum EpochFlag {
    // The deprecated flags have all been in production for long enough that
    // we can have deleted the old code paths they were guarding.
    // We retain them here in order not to break deserialization.
    _InMemoryCheckpointRootsDeprecated,
    _PerEpochFinalizedTransactionsDeprecated,
    _ObjectLockSplitTablesDeprecated,

    WritebackCacheEnabled,
    StateAccumulatorV2Enabled,
}

impl EpochFlag {
    pub fn default_flags_for_new_epoch(config: &NodeConfig) -> Vec<Self> {
        Self::default_flags_impl(&config.execution_cache, config.state_accumulator_v2)
    }

    /// For situations in which there is no config available (e.g. setting up a downloaded snapshot).
    pub fn default_for_no_config() -> Vec<Self> {
        Self::default_flags_impl(&Default::default(), false)
    }

    fn default_flags_impl(
        cache_config: &ExecutionCacheConfig,
        enable_state_accumulator_v2: bool,
    ) -> Vec<Self> {
        let mut new_flags = vec![];

        if matches!(
            choose_execution_cache(cache_config),
            ExecutionCacheConfigType::WritebackCache
        ) {
            new_flags.push(EpochFlag::WritebackCacheEnabled);
        }

        if enable_state_accumulator_v2 {
            new_flags.push(EpochFlag::StateAccumulatorV2Enabled);
        }

        new_flags
    }
}

impl fmt::Display for EpochFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Important - implementation should return low cardinality values because this is used as metric key
        match self {
            EpochFlag::_InMemoryCheckpointRootsDeprecated => {
                write!(f, "InMemoryCheckpointRoots (DEPRECATED)")
            }
            EpochFlag::_PerEpochFinalizedTransactionsDeprecated => {
                write!(f, "PerEpochFinalizedTransactions (DEPRECATED)")
            }
            EpochFlag::_ObjectLockSplitTablesDeprecated => {
                write!(f, "ObjectLockSplitTables (DEPRECATED)")
            }
            EpochFlag::WritebackCacheEnabled => write!(f, "WritebackCacheEnabled"),
            EpochFlag::StateAccumulatorV2Enabled => write!(f, "StateAccumulatorV2Enabled"),
        }
    }
}

/// Parameters of the epoch fixed at epoch start.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[enum_dispatch(EpochStartConfigTrait)]
pub enum EpochStartConfiguration {
    V1(EpochStartConfigurationV1),
    V2(EpochStartConfigurationV2),
    V3(EpochStartConfigurationV3),
    V4(EpochStartConfigurationV4),
    V5(EpochStartConfigurationV5),
    V6(EpochStartConfigurationV6),
}

impl EpochStartConfiguration {
    pub fn new(
        system_state: EpochStartSystemState,
        epoch_digest: CheckpointDigest,
        object_store: &dyn ObjectStore,
        initial_epoch_flags: Vec<EpochFlag>,
    ) -> SuiResult<Self> {
        let authenticator_obj_initial_shared_version =
            get_authenticator_state_obj_initial_shared_version(object_store)?;
        let randomness_obj_initial_shared_version =
            get_randomness_state_obj_initial_shared_version(object_store)?;
        let coin_deny_list_obj_initial_shared_version =
            get_deny_list_obj_initial_shared_version(object_store);
        let bridge_obj_initial_shared_version =
            get_bridge_obj_initial_shared_version(object_store)?;
        let bridge_committee_initiated = is_bridge_committee_initiated(object_store)?;
        Ok(Self::V6(EpochStartConfigurationV6 {
            system_state,
            epoch_digest,
            flags: initial_epoch_flags,
            authenticator_obj_initial_shared_version,
            randomness_obj_initial_shared_version,
            coin_deny_list_obj_initial_shared_version,
            bridge_obj_initial_shared_version,
            bridge_committee_initiated,
        }))
    }

    pub fn new_at_next_epoch_for_testing(&self) -> Self {
        // We only need to implement this function for the latest version.
        // When a new version is introduced, this function should be updated.
        match self {
            Self::V6(config) => {
                Self::V6(EpochStartConfigurationV6 {
                    system_state: config.system_state.new_at_next_epoch_for_testing(),
                    epoch_digest: config.epoch_digest,
                    flags: config.flags.clone(),
                    authenticator_obj_initial_shared_version: config.authenticator_obj_initial_shared_version,
                    randomness_obj_initial_shared_version: config.randomness_obj_initial_shared_version,
                    coin_deny_list_obj_initial_shared_version: config.coin_deny_list_obj_initial_shared_version,
                    bridge_obj_initial_shared_version: config.bridge_obj_initial_shared_version,
                    bridge_committee_initiated: config.bridge_committee_initiated,
                })
            }
            _ => panic!("This function is only implemented for the latest version of EpochStartConfiguration"),
        }
    }

    pub fn epoch_data(&self) -> EpochData {
        EpochData::new(
            self.epoch_start_state().epoch(),
            self.epoch_start_state().epoch_start_timestamp_ms(),
            self.epoch_digest(),
        )
    }

    pub fn epoch_start_timestamp_ms(&self) -> CheckpointTimestamp {
        self.epoch_start_state().epoch_start_timestamp_ms()
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV1 {
    system_state: EpochStartSystemState,
    /// epoch_digest is defined as following
    /// (1) For the genesis epoch it is set to 0
    /// (2) For all other epochs it is a digest of the last checkpoint of a previous epoch
    /// Note that this is in line with how epoch start timestamp is defined
    epoch_digest: CheckpointDigest,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV2 {
    system_state: EpochStartSystemState,
    epoch_digest: CheckpointDigest,
    flags: Vec<EpochFlag>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV3 {
    system_state: EpochStartSystemState,
    epoch_digest: CheckpointDigest,
    flags: Vec<EpochFlag>,
    /// Does the authenticator state object exist at the beginning of the epoch?
    authenticator_obj_initial_shared_version: Option<SequenceNumber>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV4 {
    system_state: EpochStartSystemState,
    epoch_digest: CheckpointDigest,
    flags: Vec<EpochFlag>,
    /// Do the state objects exist at the beginning of the epoch?
    authenticator_obj_initial_shared_version: Option<SequenceNumber>,
    randomness_obj_initial_shared_version: Option<SequenceNumber>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV5 {
    system_state: EpochStartSystemState,
    epoch_digest: CheckpointDigest,
    flags: Vec<EpochFlag>,
    /// Do the state objects exist at the beginning of the epoch?
    authenticator_obj_initial_shared_version: Option<SequenceNumber>,
    randomness_obj_initial_shared_version: Option<SequenceNumber>,
    coin_deny_list_obj_initial_shared_version: Option<SequenceNumber>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV6 {
    system_state: EpochStartSystemState,
    epoch_digest: CheckpointDigest,
    flags: Vec<EpochFlag>,
    /// Do the state objects exist at the beginning of the epoch?
    authenticator_obj_initial_shared_version: Option<SequenceNumber>,
    randomness_obj_initial_shared_version: Option<SequenceNumber>,
    coin_deny_list_obj_initial_shared_version: Option<SequenceNumber>,
    bridge_obj_initial_shared_version: Option<SequenceNumber>,
    bridge_committee_initiated: bool,
}

impl EpochStartConfigurationV1 {
    pub fn new(system_state: EpochStartSystemState, epoch_digest: CheckpointDigest) -> Self {
        Self {
            system_state,
            epoch_digest,
        }
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV1 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &[]
    }

    fn authenticator_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn randomness_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn coin_deny_list_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn bridge_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn bridge_committee_initiated(&self) -> bool {
        false
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV2 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &self.flags
    }

    fn authenticator_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn randomness_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn coin_deny_list_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn bridge_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn bridge_committee_initiated(&self) -> bool {
        false
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV3 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &self.flags
    }

    fn authenticator_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.authenticator_obj_initial_shared_version
    }

    fn randomness_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn coin_deny_list_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn bridge_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }
    fn bridge_committee_initiated(&self) -> bool {
        false
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV4 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &self.flags
    }

    fn authenticator_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.authenticator_obj_initial_shared_version
    }

    fn randomness_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.randomness_obj_initial_shared_version
    }

    fn coin_deny_list_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn bridge_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }

    fn bridge_committee_initiated(&self) -> bool {
        false
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV5 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &self.flags
    }

    fn authenticator_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.authenticator_obj_initial_shared_version
    }

    fn randomness_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.randomness_obj_initial_shared_version
    }

    fn coin_deny_list_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.coin_deny_list_obj_initial_shared_version
    }

    fn bridge_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        None
    }
    fn bridge_committee_initiated(&self) -> bool {
        false
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV6 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &self.flags
    }

    fn authenticator_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.authenticator_obj_initial_shared_version
    }

    fn randomness_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.randomness_obj_initial_shared_version
    }

    fn coin_deny_list_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.coin_deny_list_obj_initial_shared_version
    }

    fn bridge_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.bridge_obj_initial_shared_version
    }

    fn bridge_committee_initiated(&self) -> bool {
        self.bridge_committee_initiated
    }
}
