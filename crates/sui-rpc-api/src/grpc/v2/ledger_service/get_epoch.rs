// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ErrorReason;
use crate::Result;
use crate::RpcService;
use crate::error::EpochNotFoundError;
use crate::reader::indexes_disabled;
use prost_types::FieldMask;
use prost_types::value::Kind;
use std::collections::BTreeMap;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Epoch;
use sui_rpc::proto::sui::rpc::v2::GetEpochRequest;
use sui_rpc::proto::sui::rpc::v2::GetEpochResponse;
use sui_rpc::proto::sui::rpc::v2::ProtocolConfig;
use sui_rpc::proto::sui::rpc::v2::SystemEpochInfo;
use sui_rpc::proto::timestamp_ms_to_proto;
use sui_sdk_types::EpochId;
use sui_types::storage::EpochInfo;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::SuiSystemStateTrait;

pub const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::EPOCH;

/// The latest epoch is the live system state's. The live epoch's `protocol_config`, `start`,
/// `reference_gas_price` and `system_state` come from the live state; any other epoch's come from
/// its rpc-store row, with `system_state` the epoch-start snapshot. The end-of-epoch fields are
/// set exactly when `last_checkpoint` is, from the same rpc-store row, as soon as the row has
/// them.
#[tracing::instrument(skip(service))]
pub fn get_epoch(service: &RpcService, request: GetEpochRequest) -> Result<GetEpochResponse> {
    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask.validate::<Epoch>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };

    let mut message = Epoch::default();

    let current_system_state = service.reader.get_system_state()?;
    let live_epoch = current_system_state.epoch();

    let indexes = service.reader.inner().indexes();
    let epoch = request.epoch.unwrap_or(live_epoch);

    // A past epoch's data lives only in the rpc indexes; the live system state is the live
    // epoch's. An epoch after the live one does not exist yet, which needs no index to tell.
    if epoch < live_epoch && indexes.is_none() {
        return Err(indexes_disabled());
    }

    // Fetch epoch info, if indexing is available.
    let epoch_info = indexes
        .map(|indexes| indexes.get_epoch_info(epoch))
        .transpose()?
        .flatten();

    if epoch != live_epoch && epoch_info.is_none() {
        return Err(EpochNotFoundError::new(epoch).into());
    }

    if read_mask.contains(Epoch::EPOCH_FIELD.name) {
        message.set_epoch(epoch);
    }

    // The live epoch's protocol config, start and reference gas price come from the live state;
    // any other epoch's come from its rpc-store row below.
    if epoch == live_epoch {
        let system_state = &current_system_state;
        if let Some(submask) = read_mask.subtree(Epoch::PROTOCOL_CONFIG_FIELD) {
            let chain = service.reader.inner().get_chain_identifier()?.chain();
            let config = protocol_config_for_version(system_state.protocol_version(), chain)?;

            message.set_protocol_config(ProtocolConfig::merge_from(config, &submask));
        }

        if read_mask.contains(Epoch::START_FIELD) {
            message.set_start(timestamp_ms_to_proto(
                system_state.epoch_start_timestamp_ms(),
            ));
        }

        if read_mask.contains(Epoch::REFERENCE_GAS_PRICE_FIELD) {
            message.set_reference_gas_price(system_state.reference_gas_price());
        }
    }

    if let Some(epoch_info) = &epoch_info {
        if read_mask.contains(Epoch::FIRST_CHECKPOINT_FIELD) {
            message.first_checkpoint = epoch_info.start_checkpoint;
        }

        if read_mask.contains(Epoch::LAST_CHECKPOINT_FIELD) {
            message.last_checkpoint = epoch_info.end_checkpoint;
        }

        if read_mask.contains(Epoch::START_FIELD) && message.start.is_none() {
            message.start = epoch_info.start_timestamp_ms.map(timestamp_ms_to_proto);
        }

        if read_mask.contains(Epoch::END_FIELD) {
            message.end = epoch_info.end_timestamp_ms.map(timestamp_ms_to_proto);
        }

        if read_mask.contains(Epoch::REFERENCE_GAS_PRICE_FIELD.name)
            && message.reference_gas_price.is_none()
        {
            message.reference_gas_price = epoch_info.reference_gas_price;
        }

        if let Some(submask) = read_mask.subtree(Epoch::PROTOCOL_CONFIG_FIELD.name)
            && message.protocol_config.is_none()
        {
            let chain = service.reader.inner().get_chain_identifier()?.chain();
            let protocol_config = epoch_info
                .protocol_version
                .map(|version| protocol_config_for_version(version, chain))
                .transpose()?;

            message.protocol_config =
                protocol_config.map(|config| ProtocolConfig::merge_from(config, &submask));
        }

        if epoch_info.end_checkpoint.is_some() {
            set_epoch_end_fields(&mut message, &read_mask, epoch_info);
        }
    }

    if read_mask.contains(Epoch::TRANSACTION_START_INCLUSIVE_FIELD) {
        message.transaction_start_inclusive = match epoch.checked_sub(1) {
            None => Some(0),
            Some(previous) => indexes
                .map(|indexes| indexes.get_epoch_info(previous))
                .transpose()?
                .flatten()
                .and_then(|info| info.tx_hi),
        };
    }

    if read_mask.contains(Epoch::COMMITTEE_FIELD.name) {
        message.committee = Some(
            service
                .reader
                .get_committee(epoch)
                .ok_or_else(|| CommitteeNotFoundError::new(epoch))?
                .into(),
        );
    }

    if read_mask.contains(Epoch::SYSTEM_STATE_FIELD) {
        let system_state = if epoch == live_epoch {
            Some(current_system_state)
        } else {
            epoch_info.and_then(epoch_start_system_state)
        };
        message.system_state = system_state.map(|system_state| Box::new(system_state.into()));
    }

    Ok(GetEpochResponse::new(message))
}

/// The rpc-store row's epoch-start snapshot. A restore that lands mid-epoch seeds the row from
/// the state at the restore point and leaves `start_checkpoint` unset until the epoch's first
/// checkpoint is indexed, so that state is not served as the start state.
fn epoch_start_system_state(epoch_info: EpochInfo) -> Option<SuiSystemState> {
    epoch_info.start_checkpoint?;
    epoch_info.system_state
}

/// Set the end-of-epoch fields `read_mask` selects from `epoch_info`. Call it exactly when the
/// response serves the epoch's `last_checkpoint`, so the end fields appear together with it.
pub fn set_epoch_end_fields(
    message: &mut Epoch,
    read_mask: &FieldMaskTree,
    epoch_info: &EpochInfo,
) {
    if read_mask.contains(Epoch::TRANSACTION_END_EXCLUSIVE_FIELD) {
        message.transaction_end_exclusive = epoch_info.tx_hi;
    }
    if read_mask.contains(Epoch::ENDED_IN_SAFE_MODE_FIELD) {
        message.ended_in_safe_mode = epoch_info.safe_mode;
    }
    if read_mask.contains(Epoch::SYSTEM_EPOCH_INFO_FIELD) {
        let mut info = SystemEpochInfo::default();
        info.total_stake = epoch_info.total_stake;
        info.storage_fund_reinvestment = epoch_info.storage_fund_reinvestment;
        info.storage_charge = epoch_info.storage_charge;
        info.storage_rebate = epoch_info.storage_rebate;
        info.storage_fund_balance = epoch_info.storage_fund_balance;
        info.stake_subsidy_amount = epoch_info.stake_subsidy_amount;
        info.total_gas_fees = epoch_info.total_gas_fees;
        info.total_stake_rewards_distributed = epoch_info.total_stake_rewards_distributed;
        info.leftover_storage_fund_inflow = epoch_info.leftover_storage_fund_inflow;
        // An epoch that ended in safe mode emitted no `SystemEpochInfoEvent`, so every counter
        // is unset and so is the message.
        message.system_epoch_info = (info != SystemEpochInfo::default()).then_some(info);
    }
    if read_mask.contains(Epoch::EPOCH_COMMITMENTS_FIELD) {
        message.epoch_commitments = epoch_info
            .epoch_commitments
            .iter()
            .flatten()
            .cloned()
            .map(Into::into)
            .collect();
    }
}

#[derive(Debug)]
pub struct CommitteeNotFoundError {
    epoch: EpochId,
}

impl CommitteeNotFoundError {
    pub fn new(epoch: EpochId) -> Self {
        Self { epoch }
    }
}

impl std::fmt::Display for CommitteeNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Committee for epoch {} not found", self.epoch)
    }
}

impl std::error::Error for CommitteeNotFoundError {}

impl From<CommitteeNotFoundError> for crate::RpcError {
    fn from(value: CommitteeNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}

#[derive(Debug)]
pub(super) struct ProtocolVersionNotFoundError {
    version: u64,
}

impl ProtocolVersionNotFoundError {
    pub fn new(version: u64) -> Self {
        Self { version }
    }
}

impl std::fmt::Display for ProtocolVersionNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Protocol version {} not found", self.version)
    }
}

impl std::error::Error for ProtocolVersionNotFoundError {}

impl From<ProtocolVersionNotFoundError> for crate::RpcError {
    fn from(value: ProtocolVersionNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}

pub(super) fn protocol_config_for_version(
    version: u64,
    chain: sui_protocol_config::Chain,
) -> Result<ProtocolConfig, ProtocolVersionNotFoundError> {
    let config =
        sui_protocol_config::ProtocolConfig::get_for_version_if_supported(version.into(), chain)
            .ok_or_else(|| ProtocolVersionNotFoundError::new(version))?;
    Ok(protocol_config_to_proto(config))
}

pub fn protocol_config_to_proto(config: sui_protocol_config::ProtocolConfig) -> ProtocolConfig {
    let attributes = config
        .render::<prost_types::Value>(&mut mysten_common::rpc_format::Unmetered)
        .expect("render to prost Value should succeed");
    protocol_config_from_parts(config.version.as_u64(), attributes, config.feature_map())
}

/// Builds the proto message from a version's rendered settings and its feature flags.
///
/// `attributes` is the output of `ProtocolConfig::render`, with a null value for each setting
/// that has no value at `protocol_version`. `feature_flags` is `ProtocolConfig::feature_map`.
/// Settings with no value are left out of both `configs` and `attributes`.
pub fn protocol_config_from_parts(
    protocol_version: u64,
    attributes: BTreeMap<String, prost_types::Value>,
    feature_flags: BTreeMap<String, bool>,
) -> ProtocolConfig {
    let mut message = ProtocolConfig::default();
    message.set_protocol_version(protocol_version);

    // The deprecated string view of the settings: complex types are JSON-stringified.
    message.set_attributes(
        attributes
            .iter()
            .filter_map(|(k, v)| match &v.kind {
                Some(Kind::NullValue(_)) | None => None,
                Some(Kind::NumberValue(n)) => Some((k.to_owned(), n.to_string())),
                Some(Kind::StringValue(s)) => Some((k.to_owned(), s.to_owned())),
                Some(Kind::BoolValue(b)) => Some((k.to_owned(), b.to_string())),
                Some(Kind::StructValue(s)) => Some((
                    k.to_owned(),
                    serde_json::to_string(&sui_rpc::_serde::StructSerializer(s)).unwrap(),
                )),
                Some(Kind::ListValue(list)) => Some((
                    k.to_owned(),
                    serde_json::to_string(&sui_rpc::_serde::ListValueSerializer(list)).unwrap(),
                )),
            })
            .collect(),
    );

    let mut configs: BTreeMap<_, _> = attributes
        .into_iter()
        .filter(|(_, v)| !matches!(v.kind, None | Some(Kind::NullValue(_))))
        .collect();

    // `configs` holds the feature flags alongside the settings, as bool values.
    for (name, enabled) in &feature_flags {
        let old = configs.insert(name.clone(), prost_types::Value::from(*enabled));
        debug_assert!(
            old.is_none(),
            "feature flags and attributes can't have keys which are the same"
        );
    }

    message.set_feature_flags(feature_flags);
    message.set_configs(configs);

    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_protocol_config::Chain;
    use sui_protocol_config::ProtocolVersion;

    const BOOL_SETTING: &str = "bridge_should_try_to_finalize_committee";

    fn config_at(version: ProtocolVersion) -> sui_protocol_config::ProtocolConfig {
        sui_protocol_config::ProtocolConfig::get_for_version(version, Chain::Unknown)
    }

    fn is_null(value: &prost_types::Value) -> bool {
        matches!(value.kind, Some(Kind::NullValue(_)))
    }

    #[test]
    fn boolean_setting_is_not_a_feature_flag() {
        let config = config_at(ProtocolVersion::MAX);
        let enabled = config
            .bridge_should_try_to_finalize_committee_as_option()
            .expect("set at the highest version");

        let message = protocol_config_to_proto(config);

        assert!(!message.feature_flags.contains_key(BOOL_SETTING));
        assert_eq!(
            message.configs[BOOL_SETTING],
            prost_types::Value::from(enabled)
        );
        assert_eq!(message.attributes[BOOL_SETTING], enabled.to_string());
    }

    #[test]
    fn protocol_config_omits_unset_settings() {
        let min_config = config_at(ProtocolVersion::MIN);
        assert!(
            min_config
                .bridge_should_try_to_finalize_committee_as_option()
                .is_none()
        );
        let message = protocol_config_to_proto(min_config);
        assert!(!message.configs.contains_key(BOOL_SETTING));
        assert!(!message.attributes.contains_key(BOOL_SETTING));

        for version in [ProtocolVersion::MIN, ProtocolVersion::MAX] {
            let config = config_at(version);
            let feature_flags = config.feature_map();
            let settings: BTreeMap<_, _> = config
                .render::<prost_types::Value>(&mut mysten_common::rpc_format::Unmetered)
                .unwrap()
                .into_iter()
                .filter(|(_, v)| !is_null(v))
                .collect();

            let message = protocol_config_to_proto(config);

            assert_eq!(message.protocol_version, Some(version.as_u64()));
            assert_eq!(message.feature_flags, feature_flags);

            let mut expected_configs = settings.clone();
            expected_configs.extend(
                feature_flags
                    .iter()
                    .map(|(k, v)| (k.clone(), prost_types::Value::from(*v))),
            );
            assert_eq!(message.configs, expected_configs);

            // `attributes` holds exactly the settings with a value.
            assert!(message.attributes.keys().eq(settings.keys()));
        }
    }
}
