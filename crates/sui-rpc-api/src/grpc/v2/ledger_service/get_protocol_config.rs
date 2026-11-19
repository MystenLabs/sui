// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::get_epoch::ProtocolVersionNotFoundError;
use super::get_epoch::protocol_config_for_version;
use crate::Result;
use crate::RpcService;
use crate::read_mask_defaults::validate_read_mask;
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::GetProtocolConfigRequest;
use sui_rpc::proto::sui::rpc::v2::GetProtocolConfigResponse;
use sui_rpc::proto::sui::rpc::v2::ProtocolConfig;
use sui_types::sui_system_state::SuiSystemStateTrait;

pub const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::PROTOCOL_CONFIG;

#[tracing::instrument(skip(service))]
pub fn get_protocol_config(
    service: &RpcService,
    request: GetProtocolConfigRequest,
) -> Result<GetProtocolConfigResponse> {
    let read_mask = validate_read_mask::<ProtocolConfig>(request.read_mask, READ_MASK_DEFAULT)?;

    // The current protocol version comes from the system state, the same "latest" `GetEpoch`
    // uses. Versions above it have not run on the chain yet.
    let current_version = service.reader.get_system_state()?.protocol_version();
    let version = request.version.unwrap_or(current_version);
    if version > current_version {
        return Err(ProtocolVersionNotFoundError::new(version).into());
    }

    // The binary computes the config of any version it supports, including versions the chain
    // skipped.
    let chain = service.reader.inner().get_chain_identifier()?.chain();
    let config = protocol_config_for_version(version, chain)?;

    Ok(GetProtocolConfigResponse::default()
        .with_protocol_config(ProtocolConfig::merge_from(config, &read_mask)))
}
