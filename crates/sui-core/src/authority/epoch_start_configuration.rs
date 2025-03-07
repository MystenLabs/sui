// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use sui_config::NodeConfig;

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

    fn use_version_assignment_tables_v3(&self) -> bool {
        self.flags()
            .contains(&EpochFlag::UseVersionAssignmentTablesV3)
    }

    fn is_data_quarantine_active_from_beginning_of_epoch(&self) -> bool {
        self.flags()
            .contains(&EpochFlag::DataQuarantineFromBeginningOfEpoch)
    }
}

// IMPORTANT: Assign explicit values to each variant to ensure that the values are stable.
// When cherry-picking changes from one branch to another, the value of variants must never
// change.
//
// Unlikely: If you cherry pick a change from one branch to another, and there is a collision
// in the value of some variant, the branch which has been released should take precedence.
// In this case, the picked-from branch is inconsistent with the released branch, and must
// be fixed.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
pub enum EpochFlag {
    // The deprecated flags have all been in production for long enough that
    // we have deleted the old code paths they were guarding.
    // We retain them here in order not to break deserialization.
    _InMemoryCheckpointRootsDeprecated = 0,
    _PerEpochFinalizedTransactionsDeprecated = 1,
    _ObjectLockSplitTablesDeprecated = 2,
    _WritebackCacheEnabledDeprecated = 3,
    _StateAccumulatorV2EnabledDeprecated = 4,
    _StateAccumulatorV2EnabledTestnetDeprecated = 5,
    _StateAccumulatorV2EnabledMainnetDeprecated = 6,
    _ExecutedInEpochTableDeprecated = 7,

    UseVersionAssignmentTablesV3 = 8,

    // This flag indicates whether data quarantining has been enabled from the
    // beginning of the epoch.
    DataQuarantineFromBeginningOfEpoch = 9,
}

impl EpochFlag {
    pub fn default_flags_for_new_epoch(_config: &NodeConfig) -> Vec<Self> {
        // NodeConfig arg is not currently used, but we keep it here for future
        // flags that might depend on the config.
        Self::default_flags_impl()
    }

    /// For situations in which there is no config available (e.g. setting up a downloaded snapshot).
    pub fn default_for_no_config() -> Vec<Self> {
        Self::default_flags_impl()
    }

    fn default_flags_impl() -> Vec<Self> {
        vec![
            EpochFlag::UseVersionAssignmentTablesV3,
            EpochFlag::DataQuarantineFromBeginningOfEpoch,
        ]
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
            EpochFlag::_WritebackCacheEnabledDeprecated => {
                write!(f, "WritebackCacheEnabled (DEPRECATED)")
            }
            EpochFlag::_StateAccumulatorV2EnabledDeprecated => {
                write!(f, "StateAccumulatorV2EnabledDeprecated (DEPRECATED)")
            }
            EpochFlag::_ExecutedInEpochTableDeprecated => {
                write!(f, "ExecutedInEpochTable (DEPRECATED)")
            }
            EpochFlag::_StateAccumulatorV2EnabledTestnetDeprecated => {
                write!(f, "StateAccumulatorV2EnabledTestnet (DEPRECATED)")
            }
            EpochFlag::_StateAccumulatorV2EnabledMainnetDeprecated => {
                write!(f, "StateAccumulatorV2EnabledMainnet (DEPRECATED)")
            }
            EpochFlag::UseVersionAssignmentTablesV3 => {
                write!(f, "UseVersionAssignmentTablesV3")
            }
            EpochFlag::DataQuarantineFromBeginningOfEpoch => {
                write!(f, "DataQuarantineFromBeginningOfEpoch")
            }
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
