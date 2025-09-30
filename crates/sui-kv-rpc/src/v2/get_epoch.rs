// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::{BigTableClient, KeyValueStoreReader};
use sui_protocol_config::{Chain, ProtocolConfig};
use sui_rpc::field::{FieldMask, FieldMaskTree, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::Epoch;
use sui_rpc::proto::sui::rpc::v2::ProtocolConfig as RpcProtocolConfig;
use sui_rpc::proto::sui::rpc::v2::{GetEpochRequest, GetEpochResponse};
use sui_rpc_api::{
    grpc::v2::protocol_config_to_proto,
    proto::{google::rpc::bad_request::FieldViolation, timestamp_ms_to_proto},
    ErrorReason,
};
use sui_sdk_types::ValidatorCommittee;
use sui_types::sui_system_state::SuiSystemStateTrait;

pub const READ_MASK_DEFAULT: &str =
    "epoch,first_checkpoint,last_checkpoint,start,end,reference_gas_price,protocol_config.protocol_version";

pub async fn get_epoch(
    mut client: BigTableClient,
    request: GetEpochRequest,
    chain: Chain,
) -> sui_rpc_api::Result<GetEpochResponse> {
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

    let maybe_epoch_info = if let Some(epoch) = request.epoch {
        client.get_epoch(epoch).await?
    } else {
        client.get_latest_epoch().await?
    };

    let Some(epoch_info) = maybe_epoch_info else {
        return Ok(GetEpochResponse::default());
    };

    if read_mask.contains(Epoch::EPOCH_FIELD.name) {
        message.epoch = Some(epoch_info.epoch);
    }
    if read_mask.contains(Epoch::FIRST_CHECKPOINT_FIELD.name) {
        message.first_checkpoint = epoch_info.start_checkpoint;
    }
    if read_mask.contains(Epoch::LAST_CHECKPOINT_FIELD.name) {
        message.last_checkpoint = epoch_info.end_checkpoint;
    }
    if read_mask.contains(Epoch::START_FIELD.name) {
        message.start = epoch_info.start_timestamp_ms.map(timestamp_ms_to_proto);
    }
    if read_mask.contains(Epoch::END_FIELD.name) {
        message.end = epoch_info.end_timestamp_ms.map(timestamp_ms_to_proto);
    }
    if read_mask.contains(Epoch::REFERENCE_GAS_PRICE_FIELD.name) {
        message.reference_gas_price = epoch_info.reference_gas_price;
    }
    if let (Some(submask), Some(version)) = (
        read_mask.subtree(Epoch::PROTOCOL_CONFIG_FIELD.name),
        epoch_info.protocol_version,
    ) {
        let protocol_config = ProtocolConfig::get_for_version_if_supported(version.into(), chain);
        message.protocol_config = protocol_config.map(|config| {
            RpcProtocolConfig::merge_from(protocol_config_to_proto(config), &submask)
        });
    }
    if read_mask.contains(Epoch::COMMITTEE_FIELD.name) {
        message.committee = epoch_info.system_state.map(|system_state| {
            let committee: ValidatorCommittee = system_state
                .get_current_epoch_committee()
                .committee()
                .clone()
                .into();
            committee.into()
        });
    }
    Ok(GetEpochResponse::new(message))
}
