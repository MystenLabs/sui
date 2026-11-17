// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::BigTableClient;
use sui_kvstore::tables::epochs::col;
use sui_protocol_config::{Chain, ProtocolConfig};
use sui_rpc::field::{FieldMask, FieldMaskTree, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::Epoch;
use sui_rpc::proto::sui::rpc::v2::ProtocolConfig as RpcProtocolConfig;
use sui_rpc::proto::sui::rpc::v2::{GetEpochRequest, GetEpochResponse};
use sui_rpc_api::{
    ErrorReason, RpcError,
    grpc::v2::protocol_config_to_proto,
    proto::{google::rpc::bad_request::FieldViolation, timestamp_ms_to_proto},
};
use sui_types::sui_system_state::SuiSystemStateTrait;

use crate::served_height::ServedHeight;
use crate::v2::get_protocol_config::read_protocol_config;

pub const READ_MASK_DEFAULT: &str = sui_rpc_api::read_mask_defaults::EPOCH;

/// Where an epoch's `protocol_config` comes from.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ProtocolConfigSource {
    /// The `protocol_configs_v2` table, which has a row for every protocol version the chain ran.
    ProtocolConfigsV2,
    /// The protocol configs compiled into this binary for the chain.
    Binary(Chain),
}

/// Serve the epoch the request names, or the latest one, as of `served_height`: an epoch that
/// starts above it is not found, and an epoch that ends above it has not ended yet, so its end
/// fields are left out even when the indexer has already written them.
pub async fn get_epoch(
    mut client: BigTableClient,
    served_height: ServedHeight,
    request: GetEpochRequest,
    protocol_configs: ProtocolConfigSource,
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

    let columns = epoch_columns(&read_mask);
    let epoch = request.epoch.unwrap_or(served_height.epoch);
    if epoch > served_height.epoch {
        return Ok(GetEpochResponse::default());
    }
    let Some(mut epoch_info) = client
        .get_epochs_filtered(&[epoch], Some(&columns))
        .await?
        .pop()
    else {
        return Ok(GetEpochResponse::default());
    };
    if epoch_info
        .end_checkpoint
        .is_some_and(|end_checkpoint| end_checkpoint > served_height.checkpoint)
    {
        epoch_info.end_checkpoint = None;
        epoch_info.end_timestamp_ms = None;
    }

    if read_mask.contains(Epoch::EPOCH_FIELD.name) {
        message.epoch = epoch_info.epoch;
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
        let protocol_config = match protocol_configs {
            ProtocolConfigSource::Binary(chain) => {
                ProtocolConfig::get_for_version_if_supported(version.into(), chain)
                    .map(protocol_config_to_proto)
            }
            ProtocolConfigSource::ProtocolConfigsV2 => {
                // The epoch is served, so its version was decided at or below the served height,
                // which the `protocol_configs_v2` pipeline's watermark bounds.
                let protocol_config = read_protocol_config(&mut client, version)
                    .await?
                    .ok_or_else(|| {
                        RpcError::new(
                            tonic::Code::Internal,
                            format!(
                                "epoch {epoch} ran protocol version {version}, which has no \
                                 protocol_configs_v2 row"
                            ),
                        )
                    })?;
                Some(protocol_config)
            }
        };
        message.protocol_config = protocol_config
            .map(|protocol_config| RpcProtocolConfig::merge_from(protocol_config, &submask));
    }
    if read_mask.contains(Epoch::COMMITTEE_FIELD.name) {
        message.committee = epoch_info.system_state.as_ref().map(|system_state| {
            system_state
                .get_current_epoch_committee()
                .committee()
                .clone()
                .into()
        });
    }
    if read_mask.contains(Epoch::SYSTEM_STATE_FIELD.name) {
        message.system_state = epoch_info
            .system_state
            .map(|system_state| Box::new(system_state.into()));
    }
    Ok(GetEpochResponse::new(message))
}

/// Compute the set of BigTable columns needed for the given read mask.
/// Always includes `ep` (epoch number, small).
fn epoch_columns(mask: &FieldMaskTree) -> Vec<&'static str> {
    let mut columns = vec![col::EPOCH];

    if mask.contains(Epoch::FIRST_CHECKPOINT_FIELD.name) {
        columns.push(col::START_CHECKPOINT);
    }
    // `end_checkpoint` also decides whether the end timestamp is served yet.
    if mask.contains(Epoch::LAST_CHECKPOINT_FIELD.name) || mask.contains(Epoch::END_FIELD.name) {
        columns.push(col::END_CHECKPOINT);
    }
    if mask.contains(Epoch::START_FIELD.name) {
        columns.push(col::START_TIMESTAMP);
    }
    if mask.contains(Epoch::END_FIELD.name) {
        columns.push(col::END_TIMESTAMP);
    }
    if mask.contains(Epoch::REFERENCE_GAS_PRICE_FIELD.name) {
        columns.push(col::REFERENCE_GAS_PRICE);
    }
    if mask.subtree(Epoch::PROTOCOL_CONFIG_FIELD.name).is_some() {
        columns.push(col::PROTOCOL_VERSION);
    }
    if mask.contains(Epoch::COMMITTEE_FIELD.name) || mask.contains(Epoch::SYSTEM_STATE_FIELD.name) {
        columns.push(col::SYSTEM_STATE);
    }

    columns
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use sui_kvstore::tables::epochs;
    use sui_kvstore::testing::MockBigtableServer;

    use super::*;

    fn u64_cell(column: &'static str, value: u64) -> (&'static str, Bytes) {
        (column, Bytes::copy_from_slice(&value.to_be_bytes()))
    }

    const BOUNDS_MASK: &[&str] = &["epoch", "first_checkpoint", "last_checkpoint", "end"];

    async fn get(
        client: &BigTableClient,
        served_height: ServedHeight,
        epoch: Option<u64>,
        paths: &[&str],
    ) -> Epoch {
        let mut request = GetEpochRequest::default();
        request.epoch = epoch;
        request.read_mask = Some(FieldMask::from_paths(paths.iter().copied()));
        get_epoch(
            client.clone(),
            served_height,
            request,
            ProtocolConfigSource::Binary(Chain::Unknown),
        )
        .await
        .expect("get_epoch succeeds")
        .epoch
        .unwrap_or_default()
    }

    #[tokio::test]
    async fn epochs_are_served_as_of_the_served_height() {
        let mock = MockBigtableServer::new();
        let (addr, _server) = mock.start().await.expect("start mock BigTable");
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");
        // Epoch 0 covers checkpoints 0..=9 and its end row is written; epoch 1 starts at 10.
        mock.insert_row(
            epochs::NAME,
            epochs::encode_key(0),
            [
                u64_cell(col::EPOCH, 0),
                u64_cell(col::START_CHECKPOINT, 0),
                u64_cell(col::END_CHECKPOINT, 9),
                u64_cell(col::END_TIMESTAMP, 9_000),
            ],
        )
        .await;
        mock.insert_row(
            epochs::NAME,
            epochs::encode_key(1),
            [u64_cell(col::EPOCH, 1), u64_cell(col::START_CHECKPOINT, 10)],
        )
        .await;

        // Inside epoch 0: latest is epoch 0, which has not ended yet, and epoch 1 does not exist.
        let inside_epoch_0 = ServedHeight {
            checkpoint: 8,
            epoch: 0,
        };
        for epoch in [None, Some(0)] {
            let served = get(&client, inside_epoch_0, epoch, BOUNDS_MASK).await;
            assert_eq!(served.epoch, Some(0), "{epoch:?}");
            assert_eq!(served.first_checkpoint, Some(0), "{epoch:?}");
            assert_eq!(served.last_checkpoint, None, "{epoch:?}");
            assert_eq!(served.end, None, "{epoch:?}");
        }
        // A mask asking only for `end` still hides it: `end_checkpoint` is read to decide.
        let end_only = get(&client, inside_epoch_0, Some(0), &["end"]).await;
        assert_eq!(end_only.end, None);
        assert_eq!(
            get(&client, inside_epoch_0, Some(1), BOUNDS_MASK).await,
            Epoch::default()
        );

        // At epoch 0's last checkpoint, epoch 0 has ended.
        let end_of_epoch_0 = ServedHeight {
            checkpoint: 9,
            epoch: 0,
        };
        let ended = get(&client, end_of_epoch_0, None, BOUNDS_MASK).await;
        assert_eq!(ended.epoch, Some(0));
        assert_eq!(ended.last_checkpoint, Some(9));
        assert_eq!(ended.end, Some(timestamp_ms_to_proto(9_000)));
        let end_only = get(&client, end_of_epoch_0, Some(0), &["end"]).await;
        assert_eq!(end_only.end, Some(timestamp_ms_to_proto(9_000)));

        // Inside epoch 1: latest is epoch 1.
        let inside_epoch_1 = ServedHeight {
            checkpoint: 10,
            epoch: 1,
        };
        let latest = get(&client, inside_epoch_1, None, BOUNDS_MASK).await;
        assert_eq!(latest.epoch, Some(1));
        assert_eq!(latest.first_checkpoint, Some(10));
    }

    async fn get_protocol_config(
        client: &BigTableClient,
        epoch: u64,
        protocol_configs: ProtocolConfigSource,
    ) -> sui_rpc_api::Result<Option<RpcProtocolConfig>> {
        let mut request = GetEpochRequest::default();
        request.epoch = Some(epoch);
        request.read_mask = Some(FieldMask::from_paths(["protocol_config"]));
        let response = get_epoch(
            client.clone(),
            crate::v2::test_utils::UNBOUNDED_SERVED_HEIGHT,
            request,
            protocol_configs,
        )
        .await?;
        Ok(response.epoch.unwrap_or_default().protocol_config)
    }

    /// Read from the `protocol_configs_v2` table, an epoch's protocol config is the one computed
    /// from the binary: settings unset at its version are left out.
    #[tokio::test]
    async fn protocol_config_from_the_table_matches_the_binary() {
        use crate::v2::test_utils::{
            insert_epoch_protocol_version, insert_protocol_configs_v2_row,
        };
        use sui_protocol_config::ProtocolVersion;

        let mock = MockBigtableServer::new();
        let (addr, _server) = mock.start().await.expect("start mock BigTable");
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");
        // The earliest version leaves settings unset; the latest sets more of them.
        let versions = [ProtocolVersion::MIN.as_u64(), ProtocolVersion::MAX.as_u64()];
        for (epoch, version) in versions.into_iter().enumerate() {
            insert_epoch_protocol_version(&mock, epoch as u64, version).await;
            insert_protocol_configs_v2_row(&mock, version).await;
        }

        for (epoch, version) in versions.into_iter().enumerate() {
            let from_binary = get_protocol_config(
                &client,
                epoch as u64,
                ProtocolConfigSource::Binary(Chain::Unknown),
            )
            .await
            .unwrap()
            .expect("binary supports the version");
            assert_eq!(from_binary.protocol_version, Some(version));
            assert!(!from_binary.configs.is_empty() && !from_binary.feature_flags.is_empty());
            let from_table = get_protocol_config(
                &client,
                epoch as u64,
                ProtocolConfigSource::ProtocolConfigsV2,
            )
            .await
            .unwrap();
            assert_eq!(from_table, Some(from_binary), "epoch {epoch}");
        }
    }

    /// Reading from the table, an epoch whose version has no row is an inconsistency between the
    /// `epochs` and `protocol_configs_v2` tables.
    #[tokio::test]
    async fn protocol_config_missing_from_the_table_is_internal() {
        let mock = MockBigtableServer::new();
        let (addr, _server) = mock.start().await.expect("start mock BigTable");
        let client = BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");
        crate::v2::test_utils::insert_epoch_protocol_version(&mock, 0, 1).await;

        let error = get_protocol_config(&client, 0, ProtocolConfigSource::ProtocolConfigsV2)
            .await
            .unwrap_err();
        assert_eq!(tonic::Status::from(error).code(), tonic::Code::Internal);
    }
}
