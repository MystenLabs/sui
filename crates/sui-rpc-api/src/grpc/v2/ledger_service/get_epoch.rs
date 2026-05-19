// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ErrorReason;
use crate::Result;
use crate::RpcService;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Epoch;
use sui_rpc::proto::sui::rpc::v2::GetEpochRequest;
use sui_rpc::proto::sui::rpc::v2::GetEpochResponse;
use sui_rpc::proto::sui::rpc::v2::ProtocolConfig;
use sui_rpc::proto::timestamp_ms_to_proto;
use sui_sdk_types::EpochId;
use sui_types::sui_system_state::SuiSystemStateTrait;

pub const READ_MASK_DEFAULT: &str = "epoch,first_checkpoint,last_checkpoint,start,end,reference_gas_price,protocol_config.protocol_version";

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
    let current_epoch = current_system_state.epoch();

    let epoch = request.epoch.unwrap_or(current_epoch);

    if read_mask.contains(Epoch::EPOCH_FIELD.name) {
        message.set_epoch(epoch);
    }

    // Fetch epoch info, if indexing is available.
    let mut epoch_info = service
        .reader
        .inner()
        .indexes()
        .and_then(|indexes| indexes.get_epoch_info(epoch).ok().flatten());

    let system_state = if epoch == current_epoch {
        Some(current_system_state)
    } else {
        epoch_info
            .as_mut()
            .and_then(|info| info.system_state.take())
    };

    if let Some(system_state) = system_state {
        if let Some(submask) = read_mask.subtree(Epoch::PROTOCOL_CONFIG_FIELD) {
            let chain = service.reader.inner().get_chain_identifier()?.chain();
            let config = get_protocol_config(system_state.protocol_version(), chain)?;

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

        if read_mask.contains(Epoch::SYSTEM_STATE_FIELD) {
            message.system_state = Some(Box::new(system_state.into()));
        }
    }

    if let Some(epoch_info) = epoch_info {
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
                .map(|version| get_protocol_config(version, chain))
                .transpose()?;

            message.protocol_config =
                protocol_config.map(|config| ProtocolConfig::merge_from(config, &submask));
        }
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

    Ok(GetEpochResponse::new(message))
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
struct ProtocolVersionNotFoundError {
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

fn get_protocol_config(
    version: u64,
    chain: sui_protocol_config::Chain,
) -> Result<ProtocolConfig, ProtocolVersionNotFoundError> {
    let config =
        sui_protocol_config::ProtocolConfig::get_for_version_if_supported(version.into(), chain)
            .ok_or_else(|| ProtocolVersionNotFoundError::new(version))?;
    Ok(protocol_config_to_proto(config))
}

pub fn protocol_config_to_proto(config: sui_protocol_config::ProtocolConfig) -> ProtocolConfig {
    use prost_types::value::Kind;

    let mut message = ProtocolConfig::default();
    message.set_protocol_version(config.version.as_u64());

    // Set deprecated feature flags to the exact feature_map the protocol config gives us
    message.set_feature_flags(config.feature_map());

    // Load configs (today this is just the `attributes`), rendered to a `Value`
    let mut configs = config
        .render::<prost_types::Value>(&mut mysten_common::rpc_format::Unmetered)
        .expect("render to prost Value should succeed")
        .into_iter()
        .filter_map(|(k, maybe_v)| maybe_v.map(move |v| (k, v)))
        .collect::<std::collections::BTreeMap<_, _>>();

    // For backwards compatibility, render attributes to strings, complex types are json
    // stringified
    message.set_attributes(
        configs
            .iter()
            .filter_map(|(k, v)| match &v.kind {
                Some(Kind::NullValue(_)) => None,
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
                None => None,
            })
            .collect(),
    );

    // Convert feature flags to a `Value` then merge with other attributes
    for (k, v) in config
        .feature_map()
        .into_iter()
        .map(|(key, value)| (key, prost_types::Value::from(value)))
    {
        let old = configs.insert(k, v);

        debug_assert!(
            old.is_none(),
            "feature flags and attributes can't have keys which are the same"
        );
    }

    // Set the joined set of attributes and feature flags
    message.set_configs(configs);

    message
}
