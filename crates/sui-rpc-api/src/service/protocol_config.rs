// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::node::v2alpha::GetProtocolConfigRequest;
use crate::proto::node::v2alpha::GetProtocolConfigResponse;
use crate::Result;
use crate::RpcService;
use sui_protocol_config::ProtocolConfig;
use sui_protocol_config::ProtocolConfigValue;
use sui_protocol_config::ProtocolVersion;

impl RpcService {
    pub fn get_protocol_config(
        &self,
        request: GetProtocolConfigRequest,
    ) -> Result<GetProtocolConfigResponse> {
        let version = if let Some(version) = request.version {
            version
        } else {
            self.reader.get_system_state_summary()?.protocol_version
        };

        let config = ProtocolConfig::get_for_version_if_supported(
            version.into(),
            self.reader.inner().get_chain_identifier()?.chain(),
        )
        .ok_or_else(|| ProtocolNotFoundError::new(version))?;

        Ok(config_to_proto(config))
    }
}

#[derive(Debug)]
pub struct ProtocolNotFoundError {
    version: u64,
}

impl ProtocolNotFoundError {
    pub fn new(version: u64) -> Self {
        Self { version }
    }
}

impl std::fmt::Display for ProtocolNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Protocol version {} not found", self.version)
    }
}

impl std::error::Error for ProtocolNotFoundError {}

impl From<ProtocolNotFoundError> for crate::RpcError {
    fn from(value: ProtocolNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}

fn config_to_proto(config: ProtocolConfig) -> GetProtocolConfigResponse {
    let protocol_version = config.version.as_u64();
    let attributes = config
        .attr_map()
        .into_iter()
        .filter_map(|(k, maybe_v)| {
            maybe_v.map(move |v| {
                let v = match v {
                    ProtocolConfigValue::u16(x) => x.to_string(),
                    ProtocolConfigValue::u32(y) => y.to_string(),
                    ProtocolConfigValue::u64(z) => z.to_string(),
                    ProtocolConfigValue::bool(b) => b.to_string(),
                };
                (k, v)
            })
        })
        .collect();
    let feature_flags = config.feature_map();

    GetProtocolConfigResponse {
        protocol_version: Some(protocol_version),
        feature_flags,
        attributes,
        max_suppported_protocol_version: Some(ProtocolVersion::MAX.as_u64()),
        min_suppported_protocol_version: Some(ProtocolVersion::MIN.as_u64()),
    }
}
