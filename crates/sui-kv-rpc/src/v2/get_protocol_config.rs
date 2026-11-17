// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `GetProtocolConfig`, served from the `protocol_configs_v2` table.
//!
//! The table has a row for each protocol version the chain ran, written at the checkpoint that
//! decided the version: genesis, or the end-of-epoch checkpoint of the epoch before the one that
//! runs it. A replica that serves this table counts its pipeline's watermark toward the served
//! height (see [`crate::service_info_watermark_pipelines`]), so below that height a missing row
//! means the chain never ran the version. The row for the next epoch's version exists from the
//! last checkpoint of the current epoch, while the current version is still in force, so a
//! version above the one in force at the served height is not found either.

use std::collections::BTreeMap;

use sui_kvstore::BigTableClient;
use sui_kvstore::KeyValueStoreReader;
use sui_kvstore::ProtocolConfigV2Data;
use sui_kvstore::tables::epochs::col;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::ErrorInfo;
use sui_rpc::proto::sui::rpc::v2::{
    GetProtocolConfigRequest, GetProtocolConfigResponse, ProtocolConfig,
};
use sui_rpc_api::grpc::v2::protocol_config_from_parts;
use sui_rpc_api::read_mask_defaults::validate_read_mask;
use sui_rpc_api::{ErrorDetails, ErrorReason, RpcError};

use crate::served_height::ServedHeight;

pub const READ_MASK_DEFAULT: &str = sui_rpc_api::read_mask_defaults::PROTOCOL_CONFIG;

/// The refusal of an instance that does not serve protocol configs from the `protocol_configs_v2`
/// table: `Unimplemented` with a `NOT_SERVED_HERE` [`ErrorInfo`] whose `cause` is
/// `disabled_by_config`.
pub(crate) fn not_served_here() -> RpcError {
    let error_info = ErrorInfo::default()
        .with_reason(ErrorReason::NotServedHere.as_str_name())
        .with_domain(ErrorReason::DOMAIN)
        .with_metadata(BTreeMap::from([(
            "cause".to_owned(),
            "disabled_by_config".to_owned(),
        )]));
    RpcError::new(
        tonic::Code::Unimplemented,
        "GetProtocolConfig is not enabled on this instance",
    )
    .with_details(ErrorDetails::new().with_error_info(error_info))
}

/// Serve the protocol version the request names, or the one in force at `served_height`. A
/// version the chain has not run by `served_height` is not found.
pub async fn get_protocol_config(
    mut client: BigTableClient,
    served_height: ServedHeight,
    request: GetProtocolConfigRequest,
) -> sui_rpc_api::Result<GetProtocolConfigResponse> {
    let read_mask = validate_read_mask::<ProtocolConfig>(request.read_mask, READ_MASK_DEFAULT)?;

    let in_force = protocol_version_in_force(&mut client, served_height).await?;
    let version = request.version.unwrap_or(in_force);
    if version > in_force {
        return Err(protocol_version_not_found(version));
    }
    let Some(message) = read_protocol_config(&mut client, version).await? else {
        return Err(protocol_version_not_found(version));
    };
    Ok(GetProtocolConfigResponse::default()
        .with_protocol_config(ProtocolConfig::merge_from(message, &read_mask)))
}

/// The protocol config of `version` from its `protocol_configs_v2` row, as both
/// `GetProtocolConfig` and `GetEpoch` serve it. `None` if the table has no row for `version`.
pub(crate) async fn read_protocol_config(
    client: &mut BigTableClient,
    version: u64,
) -> Result<Option<ProtocolConfig>, RpcError> {
    Ok(client.get_protocol_configs_v2(version).await?.map(
        |ProtocolConfigV2Data {
             attributes,
             feature_flags,
         }| protocol_config_from_parts(version, attributes, feature_flags),
    ))
}

/// The protocol version of the epoch `served_height` is in, from its `epochs` row.
async fn protocol_version_in_force(
    client: &mut BigTableClient,
    served_height: ServedHeight,
) -> Result<u64, RpcError> {
    let epoch = served_height.epoch;
    client
        .get_epochs_filtered(&[epoch], Some(&[col::EPOCH, col::PROTOCOL_VERSION]))
        .await?
        .pop()
        .and_then(|epoch_info| epoch_info.protocol_version)
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!("served epoch {epoch} has no protocol version in the epochs table"),
            )
        })
}

fn protocol_version_not_found(version: u64) -> RpcError {
    RpcError::new(
        tonic::Code::NotFound,
        format!("Protocol version {version} not found"),
    )
}

#[cfg(test)]
mod tests {
    use prost_types::value::Kind;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_protocol_config::ProtocolVersion;
    use sui_rpc::field::FieldMask;
    use sui_rpc::field::FieldMaskUtil;
    use sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerService;

    use super::*;
    use crate::v2::test_utils::{insert_epoch_protocol_version, insert_protocol_configs_v2_row};
    use crate::{
        AlphaFeatures, KvRpcConfig, KvRpcServer, LedgerHistoryConfig, StagesConfig, WatermarkSource,
    };

    /// A setting whose value is a bool: unset at [`ProtocolVersion::MIN`], set later.
    const BOOL_SETTING: &str = "bridge_should_try_to_finalize_committee";

    const FULL_MASK: &[&str] = &["protocol_version", "feature_flags", "attributes", "configs"];

    fn min_version() -> u64 {
        ProtocolVersion::MIN.as_u64()
    }

    /// The version epoch 1 runs. The chain skips every version between [`min_version`] and it.
    fn epoch_1_version() -> u64 {
        ProtocolVersion::MAX.as_u64() - 1
    }

    /// The version epoch 2 runs, decided at epoch 1's last checkpoint, 19.
    fn epoch_2_version() -> u64 {
        ProtocolVersion::MAX.as_u64()
    }

    /// Epoch 1's last checkpoint, at which the row for epoch 2's version already exists.
    const END_OF_EPOCH_1: ServedHeight = ServedHeight {
        checkpoint: 19,
        epoch: 1,
    };

    /// A mock Bigtable holding epochs 0 and 1 and the `protocol_configs_v2` rows written up to
    /// epoch 1's last checkpoint: those of epoch 0's, epoch 1's and epoch 2's versions.
    async fn client() -> (BigTableClient, MockBigtableServer) {
        let mock = MockBigtableServer::new();
        let (addr, _server) = mock.start().await.expect("start mock BigTable");
        insert_epoch_protocol_version(&mock, 0, min_version()).await;
        insert_epoch_protocol_version(&mock, 1, epoch_1_version()).await;
        for version in [min_version(), epoch_1_version(), epoch_2_version()] {
            insert_protocol_configs_v2_row(&mock, version).await;
        }
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");
        (client, mock)
    }

    async fn get(
        client: &BigTableClient,
        served_height: ServedHeight,
        version: Option<u64>,
        paths: Option<&[&str]>,
    ) -> Result<ProtocolConfig, RpcError> {
        let mut request = GetProtocolConfigRequest::default();
        request.version = version;
        request.read_mask = paths.map(|paths| FieldMask::from_paths(paths.iter().copied()));
        Ok(get_protocol_config(client.clone(), served_height, request)
            .await?
            .protocol_config
            .expect("response carries a protocol config"))
    }

    fn code(error: RpcError) -> tonic::Code {
        tonic::Status::from(error).code()
    }

    /// Without a version, the version in force at the served height is served, with the default
    /// mask's `protocol_version` only.
    #[tokio::test]
    async fn default_version_is_the_one_in_force_at_the_served_height() {
        let (client, _mock) = client().await;
        for (served_height, in_force) in [
            (
                ServedHeight {
                    checkpoint: 5,
                    epoch: 0,
                },
                min_version(),
            ),
            (END_OF_EPOCH_1, epoch_1_version()),
        ] {
            let mut expected = ProtocolConfig::default();
            expected.protocol_version = Some(in_force);
            assert_eq!(
                get(&client, served_height, None, None).await.unwrap(),
                expected,
                "{served_height:?}"
            );
        }
    }

    #[tokio::test]
    async fn serves_a_requested_version_the_chain_ran() {
        let (client, _mock) = client().await;
        let config = get(
            &client,
            END_OF_EPOCH_1,
            Some(min_version()),
            Some(FULL_MASK),
        )
        .await
        .unwrap();
        assert_eq!(config.protocol_version, Some(min_version()));
        assert!(!config.configs.is_empty());
    }

    /// A version the chain skipped has no row, and a version above the one in force has not been
    /// run yet, even at the end-of-epoch checkpoint that wrote its row.
    #[tokio::test]
    async fn versions_the_chain_has_not_run_are_not_found() {
        let (client, _mock) = client().await;
        let skipped = min_version() + 1;
        let error = get(&client, END_OF_EPOCH_1, Some(skipped), None)
            .await
            .unwrap_err();
        assert_eq!(code(error), tonic::Code::NotFound);

        let error = get(&client, END_OF_EPOCH_1, Some(epoch_2_version()), None)
            .await
            .unwrap_err();
        assert_eq!(code(error), tonic::Code::NotFound);
        let served = get(&client, END_OF_EPOCH_1, Some(epoch_1_version()), None)
            .await
            .unwrap();
        assert_eq!(served.protocol_version, Some(epoch_1_version()));

        let inside_epoch_0 = ServedHeight {
            checkpoint: 5,
            epoch: 0,
        };
        let error = get(&client, inside_epoch_0, Some(epoch_1_version()), None)
            .await
            .unwrap_err();
        assert_eq!(code(error), tonic::Code::NotFound);
    }

    /// A setting with no value at a version is absent from `configs` and `attributes`, and a
    /// boolean setting is not a feature flag.
    #[tokio::test]
    async fn leaves_out_unset_settings_and_tells_settings_from_feature_flags() {
        let (client, _mock) = client().await;

        let unset = get(
            &client,
            END_OF_EPOCH_1,
            Some(min_version()),
            Some(FULL_MASK),
        )
        .await
        .unwrap();
        assert!(!unset.configs.is_empty());
        assert!(!unset.configs.contains_key(BOOL_SETTING));
        assert!(!unset.attributes.contains_key(BOOL_SETTING));
        assert!(
            unset
                .configs
                .values()
                .all(|value| !matches!(value.kind, None | Some(Kind::NullValue(_)))),
            "{:?}",
            unset.configs
        );

        let set = get(&client, END_OF_EPOCH_1, None, Some(FULL_MASK))
            .await
            .unwrap();
        assert!(matches!(
            set.configs
                .get(BOOL_SETTING)
                .and_then(|value| value.kind.as_ref()),
            Some(Kind::BoolValue(_))
        ));
        assert!(!set.feature_flags.contains_key(BOOL_SETTING));
        let binary = sui_protocol_config::ProtocolConfig::get_for_version(
            epoch_1_version().into(),
            sui_protocol_config::Chain::Unknown,
        );
        assert_eq!(set.feature_flags, binary.feature_map());
        for (flag, enabled) in &set.feature_flags {
            assert_eq!(
                set.configs.get(flag).and_then(|value| value.kind.clone()),
                Some(Kind::BoolValue(*enabled)),
                "{flag}"
            );
        }
    }

    #[tokio::test]
    async fn rejects_an_invalid_read_mask() {
        let (client, _mock) = client().await;
        let error = get(&client, END_OF_EPOCH_1, None, Some(&["no_such_field"]))
            .await
            .unwrap_err();
        assert_eq!(code(error), tonic::Code::InvalidArgument);
    }

    /// An instance without `enable-protocol-configs-v2` refuses as `Unimplemented` with
    /// `NOT_SERVED_HERE` / `disabled_by_config`, without reading Bigtable.
    #[tokio::test]
    async fn refused_as_not_served_here_when_disabled() {
        use prost::Message as _;
        use sui_rpc::proto::google::rpc::Status;

        let mock = MockBigtableServer::new();
        let (addr, _server) = mock.start().await.expect("start mock BigTable");
        let server = KvRpcServer::new_local_with_config(
            addr.to_string(),
            "test".to_string(),
            None,
            LedgerHistoryConfig::default(),
            KvRpcConfig::default().request_bigtable_concurrency(),
            KvRpcConfig::default().monotonic_read_wait_timeout(),
            StagesConfig::default(),
            WatermarkSource::IndexerStreams(vec![]),
            AlphaFeatures::default(),
        )
        .await
        .expect("create kv-rpc server");

        let status = LedgerService::get_protocol_config(
            &server,
            server.admitted_request(GetProtocolConfigRequest::default()),
        )
        .await
        .unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
        let details = Status::decode(status.details()).expect("status details");
        let error_infos: Vec<ErrorInfo> = details
            .details
            .iter()
            .filter_map(|detail| detail.to_msg::<ErrorInfo>().ok())
            .collect();
        let [error_info] = error_infos.as_slice() else {
            panic!("expected exactly one ErrorInfo in {details:?}");
        };
        assert_eq!(error_info.reason, ErrorReason::NotServedHere.as_str_name());
        assert_eq!(error_info.domain, ErrorReason::DOMAIN);
        assert_eq!(
            error_info.metadata,
            [("cause".to_owned(), "disabled_by_config".to_owned())].into()
        );
        assert!(mock.read_rows_calls().await.is_empty());
    }
}
