// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use sui_config::NodeConfig;

use std::fmt;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::bridge::is_bridge_committee_initiated;
use sui_types::epoch_data::EpochData;
use sui_types::error::SuiResult;
use sui_types::messages_checkpoint::{CheckpointDigest, CheckpointTimestamp};
use sui_types::object::Owner;
use sui_types::storage::ObjectStore;
use sui_types::sui_system_state::epoch_start_sui_system_state::{
    EpochStartSystemState, EpochStartSystemStateTrait,
};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_ADDRESS_ALIAS_STATE_OBJECT_ID,
    SUI_AUTHENTICATOR_STATE_OBJECT_ID, SUI_BRIDGE_OBJECT_ID, SUI_COIN_REGISTRY_OBJECT_ID,
    SUI_DENY_LIST_OBJECT_ID, SUI_DISPLAY_REGISTRY_OBJECT_ID,
    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID, SUI_RANDOMNESS_STATE_OBJECT_ID,
};

/// Well-known shared system objects whose initial shared version is recorded in
/// the epoch start configuration. To make a new system object's initial shared
/// version available at epoch start, add its object id here -- no new
/// `EpochStartConfiguration` version is required.
const SYSTEM_SHARED_OBJECT_IDS: &[ObjectID] = &[
    SUI_AUTHENTICATOR_STATE_OBJECT_ID,
    SUI_RANDOMNESS_STATE_OBJECT_ID,
    SUI_DENY_LIST_OBJECT_ID,
    SUI_BRIDGE_OBJECT_ID,
    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    SUI_COIN_REGISTRY_OBJECT_ID,
    SUI_DISPLAY_REGISTRY_OBJECT_ID,
    SUI_ADDRESS_ALIAS_STATE_OBJECT_ID,
    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
];

/// Reads the initial shared version of a system shared object from the store.
/// Returns `None` if the object does not yet exist at the start of the epoch.
fn get_system_object_initial_shared_version(
    object_store: &dyn ObjectStore,
    object_id: ObjectID,
) -> Option<SequenceNumber> {
    object_store
        .get_object(&object_id)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("System object {object_id} must be shared"),
        })
}

/// Helper for the frozen legacy configurations (V1..V10), whose individual
/// `Option<SequenceNumber>` fields predate the generic version map. Looks up
/// `object_id` against the (id, version) pairs the configuration stored.
fn legacy_lookup(
    object_id: ObjectID,
    pairs: &[(ObjectID, Option<SequenceNumber>)],
) -> Option<SequenceNumber> {
    pairs
        .iter()
        .find(|(id, _)| *id == object_id)
        .and_then(|(_, version)| *version)
}

#[enum_dispatch]
pub trait EpochStartConfigTrait {
    fn epoch_digest(&self) -> CheckpointDigest;
    fn epoch_start_state(&self) -> &EpochStartSystemState;
    fn flags(&self) -> &[EpochFlag];
    fn bridge_committee_initiated(&self) -> bool;
    /// Returns the initial shared version of the given system object as of the
    /// start of the epoch, or `None` if the object did not exist yet.
    fn system_object_initial_shared_version(&self, object_id: ObjectID) -> Option<SequenceNumber>;
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
    _GlobalStateHashV2EnabledDeprecated = 4,
    _GlobalStateHashV2EnabledTestnetDeprecated = 5,
    _GlobalStateHashV2EnabledMainnetDeprecated = 6,
    _ExecutedInEpochTableDeprecated = 7,
    _UseVersionAssignmentTablesV3 = 8,
    _DataQuarantineFromBeginningOfEpochDeprecated = 9,
    _UseCommitHandlerV2Deprecated = 10,

    // Used for `test_epoch_flag_upgrade`.
    #[cfg(msim)]
    DummyFlag = 11,
}

impl EpochFlag {
    pub fn default_flags_for_new_epoch(_config: &NodeConfig) -> Vec<Self> {
        // NodeConfig arg is not currently used, but we keep it here for future
        // flags that might depend on the config.
        Self::default_flags_impl()
    }

    // Return flags that are mandatory for the current version of the code. This is used
    // so that `test_epoch_flag_upgrade` can still work correctly even when there are no
    // optional flags.
    pub fn mandatory_flags() -> Vec<Self> {
        vec![]
    }

    /// For situations in which there is no config available (e.g. setting up a downloaded snapshot).
    pub fn default_for_no_config() -> Vec<Self> {
        Self::default_flags_impl()
    }

    fn default_flags_impl() -> Vec<Self> {
        #[cfg(msim)]
        {
            vec![EpochFlag::DummyFlag]
        }
        #[cfg(not(msim))]
        {
            vec![]
        }
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
            EpochFlag::_GlobalStateHashV2EnabledDeprecated => {
                write!(f, "GlobalStateHashV2EnabledDeprecated (DEPRECATED)")
            }
            EpochFlag::_ExecutedInEpochTableDeprecated => {
                write!(f, "ExecutedInEpochTable (DEPRECATED)")
            }
            EpochFlag::_GlobalStateHashV2EnabledTestnetDeprecated => {
                write!(f, "GlobalStateHashV2EnabledTestnet (DEPRECATED)")
            }
            EpochFlag::_GlobalStateHashV2EnabledMainnetDeprecated => {
                write!(f, "GlobalStateHashV2EnabledMainnet (DEPRECATED)")
            }
            EpochFlag::_UseVersionAssignmentTablesV3 => {
                write!(f, "UseVersionAssignmentTablesV3 (DEPRECATED)")
            }
            EpochFlag::_DataQuarantineFromBeginningOfEpochDeprecated => {
                write!(f, "DataQuarantineFromBeginningOfEpoch (DEPRECATED)")
            }
            EpochFlag::_UseCommitHandlerV2Deprecated => {
                write!(f, "UseCommitHandlerV2 (DEPRECATED)")
            }
            #[cfg(msim)]
            EpochFlag::DummyFlag => {
                write!(f, "DummyFlag")
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
    V7(EpochStartConfigurationV7),
    V8(EpochStartConfigurationV8),
    V9(EpochStartConfigurationV9),
    V10(EpochStartConfigurationV10),
    V11(EpochStartConfigurationV11),
}

impl EpochStartConfiguration {
    pub fn new(
        system_state: EpochStartSystemState,
        epoch_digest: CheckpointDigest,
        object_store: &dyn ObjectStore,
        initial_epoch_flags: Vec<EpochFlag>,
    ) -> SuiResult<Self> {
        let mut system_object_versions = BTreeMap::new();
        for &object_id in SYSTEM_SHARED_OBJECT_IDS {
            if let Some(version) = get_system_object_initial_shared_version(object_store, object_id)
            {
                system_object_versions.insert(object_id, version);
            }
        }
        let bridge_committee_initiated = is_bridge_committee_initiated(object_store)?;
        Ok(Self::V11(EpochStartConfigurationV11 {
            system_state,
            epoch_digest,
            flags: initial_epoch_flags,
            system_object_versions,
            bridge_committee_initiated,
        }))
    }

    pub fn new_at_next_epoch_for_testing(&self) -> Self {
        // We only need to implement this function for the latest version.
        // When a new version is introduced, this function should be updated.
        match self {
            Self::V11(config) => Self::V11(EpochStartConfigurationV11 {
                system_state: config.system_state.new_at_next_epoch_for_testing(),
                epoch_digest: config.epoch_digest,
                flags: config.flags.clone(),
                system_object_versions: config.system_object_versions.clone(),
                bridge_committee_initiated: config.bridge_committee_initiated,
            }),
            _ => panic!(
                "This function is only implemented for the latest version of EpochStartConfiguration"
            ),
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

    // Convenience accessors for the well-known system objects. These delegate to
    // the generic `system_object_initial_shared_version` lookup, so a new system
    // object does not strictly require its own accessor.
    pub fn authenticator_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.system_object_initial_shared_version(SUI_AUTHENTICATOR_STATE_OBJECT_ID)
    }

    pub fn randomness_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.system_object_initial_shared_version(SUI_RANDOMNESS_STATE_OBJECT_ID)
    }

    pub fn coin_deny_list_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.system_object_initial_shared_version(SUI_DENY_LIST_OBJECT_ID)
    }

    pub fn bridge_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.system_object_initial_shared_version(SUI_BRIDGE_OBJECT_ID)
    }

    pub fn accumulator_root_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.system_object_initial_shared_version(SUI_ACCUMULATOR_ROOT_OBJECT_ID)
    }

    pub fn coin_registry_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.system_object_initial_shared_version(SUI_COIN_REGISTRY_OBJECT_ID)
    }

    pub fn display_registry_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.system_object_initial_shared_version(SUI_DISPLAY_REGISTRY_OBJECT_ID)
    }

    pub fn address_alias_state_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.system_object_initial_shared_version(SUI_ADDRESS_ALIAS_STATE_OBJECT_ID)
    }

    pub fn forwarding_address_registry_obj_initial_shared_version(&self) -> Option<SequenceNumber> {
        self.system_object_initial_shared_version(SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
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

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV7 {
    system_state: EpochStartSystemState,
    epoch_digest: CheckpointDigest,
    flags: Vec<EpochFlag>,
    /// Do the state objects exist at the beginning of the epoch?
    authenticator_obj_initial_shared_version: Option<SequenceNumber>,
    randomness_obj_initial_shared_version: Option<SequenceNumber>,
    coin_deny_list_obj_initial_shared_version: Option<SequenceNumber>,
    bridge_obj_initial_shared_version: Option<SequenceNumber>,
    bridge_committee_initiated: bool,
    accumulator_root_obj_initial_shared_version: Option<SequenceNumber>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV8 {
    system_state: EpochStartSystemState,
    epoch_digest: CheckpointDigest,
    flags: Vec<EpochFlag>,
    /// Do the state objects exist at the beginning of the epoch?
    authenticator_obj_initial_shared_version: Option<SequenceNumber>,
    randomness_obj_initial_shared_version: Option<SequenceNumber>,
    coin_deny_list_obj_initial_shared_version: Option<SequenceNumber>,
    bridge_obj_initial_shared_version: Option<SequenceNumber>,
    bridge_committee_initiated: bool,
    accumulator_root_obj_initial_shared_version: Option<SequenceNumber>,
    coin_registry_obj_initial_shared_version: Option<SequenceNumber>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV9 {
    system_state: EpochStartSystemState,
    epoch_digest: CheckpointDigest,
    flags: Vec<EpochFlag>,
    /// Do the state objects exist at the beginning of the epoch?
    authenticator_obj_initial_shared_version: Option<SequenceNumber>,
    randomness_obj_initial_shared_version: Option<SequenceNumber>,
    coin_deny_list_obj_initial_shared_version: Option<SequenceNumber>,
    bridge_obj_initial_shared_version: Option<SequenceNumber>,
    bridge_committee_initiated: bool,
    accumulator_root_obj_initial_shared_version: Option<SequenceNumber>,
    coin_registry_obj_initial_shared_version: Option<SequenceNumber>,
    display_registry_obj_initial_shared_version: Option<SequenceNumber>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV10 {
    system_state: EpochStartSystemState,
    epoch_digest: CheckpointDigest,
    flags: Vec<EpochFlag>,
    /// Do the state objects exist at the beginning of the epoch?
    authenticator_obj_initial_shared_version: Option<SequenceNumber>,
    randomness_obj_initial_shared_version: Option<SequenceNumber>,
    coin_deny_list_obj_initial_shared_version: Option<SequenceNumber>,
    bridge_obj_initial_shared_version: Option<SequenceNumber>,
    bridge_committee_initiated: bool,
    accumulator_root_obj_initial_shared_version: Option<SequenceNumber>,
    coin_registry_obj_initial_shared_version: Option<SequenceNumber>,
    display_registry_obj_initial_shared_version: Option<SequenceNumber>,
    address_alias_state_obj_initial_shared_version: Option<SequenceNumber>,
}

/// Current configuration shape. The per-object initial shared versions are kept
/// in a map keyed by object id, so introducing a new system shared object only
/// requires extending `SYSTEM_SHARED_OBJECT_IDS` -- no new configuration version.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartConfigurationV11 {
    system_state: EpochStartSystemState,
    epoch_digest: CheckpointDigest,
    flags: Vec<EpochFlag>,
    /// Initial shared versions of the system shared objects that existed at the
    /// start of the epoch, keyed by object id. Objects absent from the map did
    /// not exist yet.
    system_object_versions: BTreeMap<ObjectID, SequenceNumber>,
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

    fn bridge_committee_initiated(&self) -> bool {
        false
    }

    fn system_object_initial_shared_version(&self, _object_id: ObjectID) -> Option<SequenceNumber> {
        None
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

    fn bridge_committee_initiated(&self) -> bool {
        false
    }

    fn system_object_initial_shared_version(&self, _object_id: ObjectID) -> Option<SequenceNumber> {
        None
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

    fn bridge_committee_initiated(&self) -> bool {
        false
    }

    fn system_object_initial_shared_version(&self, object_id: ObjectID) -> Option<SequenceNumber> {
        legacy_lookup(
            object_id,
            &[(
                SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                self.authenticator_obj_initial_shared_version,
            )],
        )
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

    fn bridge_committee_initiated(&self) -> bool {
        false
    }

    fn system_object_initial_shared_version(&self, object_id: ObjectID) -> Option<SequenceNumber> {
        legacy_lookup(
            object_id,
            &[
                (
                    SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                    self.authenticator_obj_initial_shared_version,
                ),
                (
                    SUI_RANDOMNESS_STATE_OBJECT_ID,
                    self.randomness_obj_initial_shared_version,
                ),
            ],
        )
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

    fn bridge_committee_initiated(&self) -> bool {
        false
    }

    fn system_object_initial_shared_version(&self, object_id: ObjectID) -> Option<SequenceNumber> {
        legacy_lookup(
            object_id,
            &[
                (
                    SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                    self.authenticator_obj_initial_shared_version,
                ),
                (
                    SUI_RANDOMNESS_STATE_OBJECT_ID,
                    self.randomness_obj_initial_shared_version,
                ),
                (
                    SUI_DENY_LIST_OBJECT_ID,
                    self.coin_deny_list_obj_initial_shared_version,
                ),
            ],
        )
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

    fn bridge_committee_initiated(&self) -> bool {
        self.bridge_committee_initiated
    }

    fn system_object_initial_shared_version(&self, object_id: ObjectID) -> Option<SequenceNumber> {
        legacy_lookup(
            object_id,
            &[
                (
                    SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                    self.authenticator_obj_initial_shared_version,
                ),
                (
                    SUI_RANDOMNESS_STATE_OBJECT_ID,
                    self.randomness_obj_initial_shared_version,
                ),
                (
                    SUI_DENY_LIST_OBJECT_ID,
                    self.coin_deny_list_obj_initial_shared_version,
                ),
                (SUI_BRIDGE_OBJECT_ID, self.bridge_obj_initial_shared_version),
            ],
        )
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV7 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &self.flags
    }

    fn bridge_committee_initiated(&self) -> bool {
        self.bridge_committee_initiated
    }

    fn system_object_initial_shared_version(&self, object_id: ObjectID) -> Option<SequenceNumber> {
        legacy_lookup(
            object_id,
            &[
                (
                    SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                    self.authenticator_obj_initial_shared_version,
                ),
                (
                    SUI_RANDOMNESS_STATE_OBJECT_ID,
                    self.randomness_obj_initial_shared_version,
                ),
                (
                    SUI_DENY_LIST_OBJECT_ID,
                    self.coin_deny_list_obj_initial_shared_version,
                ),
                (SUI_BRIDGE_OBJECT_ID, self.bridge_obj_initial_shared_version),
                (
                    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                    self.accumulator_root_obj_initial_shared_version,
                ),
            ],
        )
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV8 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &self.flags
    }

    fn bridge_committee_initiated(&self) -> bool {
        self.bridge_committee_initiated
    }

    fn system_object_initial_shared_version(&self, object_id: ObjectID) -> Option<SequenceNumber> {
        legacy_lookup(
            object_id,
            &[
                (
                    SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                    self.authenticator_obj_initial_shared_version,
                ),
                (
                    SUI_RANDOMNESS_STATE_OBJECT_ID,
                    self.randomness_obj_initial_shared_version,
                ),
                (
                    SUI_DENY_LIST_OBJECT_ID,
                    self.coin_deny_list_obj_initial_shared_version,
                ),
                (SUI_BRIDGE_OBJECT_ID, self.bridge_obj_initial_shared_version),
                (
                    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                    self.accumulator_root_obj_initial_shared_version,
                ),
                (
                    SUI_COIN_REGISTRY_OBJECT_ID,
                    self.coin_registry_obj_initial_shared_version,
                ),
            ],
        )
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV9 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &self.flags
    }

    fn bridge_committee_initiated(&self) -> bool {
        self.bridge_committee_initiated
    }

    fn system_object_initial_shared_version(&self, object_id: ObjectID) -> Option<SequenceNumber> {
        legacy_lookup(
            object_id,
            &[
                (
                    SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                    self.authenticator_obj_initial_shared_version,
                ),
                (
                    SUI_RANDOMNESS_STATE_OBJECT_ID,
                    self.randomness_obj_initial_shared_version,
                ),
                (
                    SUI_DENY_LIST_OBJECT_ID,
                    self.coin_deny_list_obj_initial_shared_version,
                ),
                (SUI_BRIDGE_OBJECT_ID, self.bridge_obj_initial_shared_version),
                (
                    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                    self.accumulator_root_obj_initial_shared_version,
                ),
                (
                    SUI_COIN_REGISTRY_OBJECT_ID,
                    self.coin_registry_obj_initial_shared_version,
                ),
                (
                    SUI_DISPLAY_REGISTRY_OBJECT_ID,
                    self.display_registry_obj_initial_shared_version,
                ),
            ],
        )
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV10 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &self.flags
    }

    fn bridge_committee_initiated(&self) -> bool {
        self.bridge_committee_initiated
    }

    fn system_object_initial_shared_version(&self, object_id: ObjectID) -> Option<SequenceNumber> {
        legacy_lookup(
            object_id,
            &[
                (
                    SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                    self.authenticator_obj_initial_shared_version,
                ),
                (
                    SUI_RANDOMNESS_STATE_OBJECT_ID,
                    self.randomness_obj_initial_shared_version,
                ),
                (
                    SUI_DENY_LIST_OBJECT_ID,
                    self.coin_deny_list_obj_initial_shared_version,
                ),
                (SUI_BRIDGE_OBJECT_ID, self.bridge_obj_initial_shared_version),
                (
                    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                    self.accumulator_root_obj_initial_shared_version,
                ),
                (
                    SUI_COIN_REGISTRY_OBJECT_ID,
                    self.coin_registry_obj_initial_shared_version,
                ),
                (
                    SUI_DISPLAY_REGISTRY_OBJECT_ID,
                    self.display_registry_obj_initial_shared_version,
                ),
                (
                    SUI_ADDRESS_ALIAS_STATE_OBJECT_ID,
                    self.address_alias_state_obj_initial_shared_version,
                ),
            ],
        )
    }
}

impl EpochStartConfigTrait for EpochStartConfigurationV11 {
    fn epoch_digest(&self) -> CheckpointDigest {
        self.epoch_digest
    }

    fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.system_state
    }

    fn flags(&self) -> &[EpochFlag] {
        &self.flags
    }

    fn bridge_committee_initiated(&self) -> bool {
        self.bridge_committee_initiated
    }

    fn system_object_initial_shared_version(&self, object_id: ObjectID) -> Option<SequenceNumber> {
        self.system_object_versions.get(&object_id).copied()
    }
}
