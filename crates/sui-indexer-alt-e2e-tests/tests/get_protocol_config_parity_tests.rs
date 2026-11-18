// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `LedgerService/GetProtocolConfig` and `GetEpoch`'s `protocol_config` on a fullnode, which
//! computes protocol configs from its binary, and on kv-rpc indexing the same chain, which serves
//! them from the `protocol_configs_v2` table.

use std::time::Duration;

use prometheus::Registry;
use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_kv_rpc::KvRpcConfig;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetEpochRequest;
use sui_rpc::proto::sui::rpc::v2::GetProtocolConfigRequest;
use sui_rpc::proto::sui::rpc::v2::ProtocolConfig;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use tempfile::TempDir;
use test_cluster::TestClusterBuilder;
use tonic::transport::Channel;

const FULL_MASK: &[&str] = &["protocol_version", "feature_flags", "attributes", "configs"];

/// The whole protocol config of `version`, or of the version in force when it is `None`.
async fn get_protocol_config(
    client: &mut LedgerServiceClient<Channel>,
    version: Option<u64>,
) -> Result<ProtocolConfig, tonic::Status> {
    let mut request = GetProtocolConfigRequest::default();
    request.version = version;
    request.read_mask = Some(FieldMask::from_paths(FULL_MASK.iter().copied()));
    Ok(client
        .get_protocol_config(request)
        .await?
        .into_inner()
        .protocol_config
        .unwrap_or_else(|| panic!("GetProtocolConfig {version:?} carries no protocol config")))
}

async fn epoch_protocol_config(
    client: &mut LedgerServiceClient<Channel>,
    epoch: u64,
) -> Option<ProtocolConfig> {
    let request =
        GetEpochRequest::new(epoch).with_read_mask(FieldMask::from_paths(["protocol_config"]));
    client
        .get_epoch(request)
        .await
        .unwrap_or_else(|status| panic!("GetEpoch {epoch}: {status:?}"))
        .into_inner()
        .epoch
        .unwrap_or_else(|| panic!("GetEpoch {epoch} found no epoch"))
        .protocol_config
}

async fn latest_checkpoint(client: &mut LedgerServiceClient<Channel>) -> u64 {
    client
        .get_checkpoint(
            GetCheckpointRequest::latest()
                .with_read_mask(FieldMask::from_paths(["sequence_number"])),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap()
        .sequence_number
        .unwrap()
}

/// With `enable-protocol-configs-v2`, kv-rpc serves the protocol config of the version the chain
/// runs, named or defaulted, and the running epoch's `protocol_config`, as the fullnode computes
/// them, and both answer `NOT_FOUND` for the version above it.
#[tokio::test]
async fn test_protocol_config_matches_between_fullnode_and_kv_rpc() {
    let ingestion_dir = TempDir::new().unwrap();
    let onchain = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_data_ingestion_dir(ingestion_dir.path().to_path_buf())
        .build()
        .await;
    onchain.wait_for_rpc_index_ready().await;
    let mut fullnode = LedgerServiceClient::connect(onchain.rpc_url().to_owned())
        .await
        .unwrap();

    let offchain = OffchainCluster::new(
        ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(ingestion_dir.path().to_path_buf()),
                ..Default::default()
            },
            ..Default::default()
        },
        OffchainClusterConfig {
            kv_rpc_config: KvRpcConfig {
                enable_protocol_configs_v2: Some(true),
                ..Default::default()
            },
            ..Default::default()
        },
        &Registry::new(),
    )
    .await
    .unwrap();
    let tip = latest_checkpoint(&mut fullnode).await;
    offchain
        .wait_for_kv_rpc(tip, Duration::from_secs(60))
        .await
        .unwrap_or_else(|_| panic!("kv-rpc serves checkpoint {tip}"));
    let mut kv_rpc = LedgerServiceClient::connect(offchain.kv_rpc_url().to_string())
        .await
        .unwrap();

    let served = get_protocol_config(&mut fullnode, None).await.unwrap();
    let version = served.protocol_version.expect("protocol_version");
    // The comparison only means something if the config carries settings.
    assert!(!served.feature_flags.is_empty());
    assert!(!served.attributes.is_empty());
    assert!(!served.configs.is_empty());
    for requested in [None, Some(version)] {
        assert_eq!(
            get_protocol_config(&mut kv_rpc, requested).await.unwrap(),
            served,
            "{requested:?}"
        );
    }

    let epoch_config = epoch_protocol_config(&mut fullnode, 0).await;
    assert_eq!(epoch_config.as_ref(), Some(&served));
    assert_eq!(epoch_protocol_config(&mut kv_rpc, 0).await, epoch_config);

    let fullnode_status = get_protocol_config(&mut fullnode, Some(version + 1))
        .await
        .unwrap_err();
    assert_eq!(fullnode_status.code(), tonic::Code::NotFound);
    let kv_rpc_status = get_protocol_config(&mut kv_rpc, Some(version + 1))
        .await
        .unwrap_err();
    assert_eq!(kv_rpc_status.code(), fullnode_status.code());
    assert_eq!(kv_rpc_status.message(), fullnode_status.message());
}
