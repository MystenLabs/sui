// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-pipeline availability policies (`rpc.availability`) on the embedded
//! `sui-rpc-store`, end to end: node YAML → rpc-store reader gating → gRPC
//! `Unavailable`.
//!
//! Each test spawns a dedicated fullnode (reusing the harness from
//! [`restore`](crate::restore)) with one pipeline disabled, and asserts that
//! (1) reads needing the gated pipeline return `Code::Unavailable` naming
//! it, (2) neighbouring index surfaces keep serving, and (3) indexing itself
//! is unaffected — `wait_for_indexed` still sees both cohorts advance,
//! because availability is a read-side policy only.
//!
//! Only `enabled: false` is exercised here: `max-checkpoint-lag` gating
//! depends on transient lag, which is nondeterministic in a live cluster,
//! and is covered by the reader's unit tests instead.

use std::collections::BTreeMap;
use std::collections::HashSet;

use prost_types::FieldMask;
use sui_config::RpcConfig;
use sui_config::rpc_config::PipelineAvailabilityConfig;
use sui_config::rpc_config::RpcAvailabilityConfig;
use sui_macros::sim_test;
use sui_rpc::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetBalanceRequest;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::QueryOptions;
use sui_rpc::proto::sui::rpc::v2alpha::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2alpha::list_transactions_response;
use sui_types::base_types::SuiAddress;
use test_cluster::TestClusterBuilder;

use crate::restore::SUI_COIN_TYPE;
use crate::restore::chain_tip;
use crate::restore::sender_filter;
use crate::restore::spawn_fullnode;
use crate::restore::transfer_to_fresh_address;
use crate::restore::wait_for_indexed;

/// An rpc config with embedded indexing on and `pipelines` disabled via
/// `rpc.availability`.
fn config_with_disabled_pipelines(pipelines: &[&str]) -> RpcConfig {
    RpcConfig {
        enable_indexing: Some(true),
        availability: Some(RpcAvailabilityConfig {
            pipelines: pipelines
                .iter()
                .map(|name| {
                    (
                        name.to_string(),
                        PipelineAvailabilityConfig {
                            enabled: Some(false),
                            ..Default::default()
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// `GetBalance` for `owner`'s SUI, surfacing the gRPC status.
async fn sui_balance_response(rpc_url: &str, owner: SuiAddress) -> Result<u64, tonic::Status> {
    let mut client = Client::new(rpc_url.to_owned()).unwrap();
    let mut request = GetBalanceRequest::default();
    request.owner = Some(owner.to_string());
    request.coin_type = Some(SUI_COIN_TYPE.to_string());
    Ok(client
        .state_client()
        .get_balance(request)
        .await?
        .into_inner()
        .balance
        .unwrap()
        .balance
        .unwrap())
}

/// `ListOwnedObjects` object ids for `owner`, surfacing the gRPC status.
async fn owned_object_ids(rpc_url: &str, owner: SuiAddress) -> Result<Vec<String>, tonic::Status> {
    let mut client = Client::new(rpc_url.to_owned()).unwrap();
    let mut request = ListOwnedObjectsRequest::default();
    request.owner = Some(owner.to_string());
    request.read_mask = Some(FieldMask::from_paths(["object_id"]));
    Ok(client
        .state_client()
        .list_owned_objects(request)
        .await?
        .into_inner()
        .objects
        .into_iter()
        .filter_map(|object| object.object_id)
        .collect())
}

/// `ListTransactions` digests for `sender`, surfacing the gRPC status
/// whether it fails at the call or on the stream's first message.
async fn transaction_digests_by_sender(
    rpc_url: &str,
    sender: SuiAddress,
) -> Result<HashSet<String>, tonic::Status> {
    let mut client = LedgerServiceClient::connect(rpc_url.to_owned())
        .await
        .unwrap();
    let mut options = QueryOptions::default();
    options.limit = Some(500);
    let mut request = ListTransactionsRequest::default();
    request.read_mask = Some(FieldMask::from_paths(["digest"]));
    request.filter = Some(sender_filter(sender));
    request.options = Some(options);
    let mut stream = client.list_transactions(request).await?.into_inner();
    let mut digests = HashSet::new();
    while let Some(response) = stream.message().await? {
        if let Some(list_transactions_response::Response::Item(item)) = response.response
            && let Some(digest) = item.transaction.and_then(|tx| tx.digest)
        {
            digests.insert(digest);
        }
    }
    Ok(digests)
}

fn assert_unavailable(status: &tonic::Status, pipeline: &str) {
    assert_eq!(status.code(), tonic::Code::Unavailable, "{status:?}");
    assert!(
        status.message().contains(pipeline),
        "expected the gated pipeline {pipeline:?} in the message: {status:?}",
    );
}

/// Disabling the `balance` pipeline gates `GetBalance` as `Unavailable`
/// while `ListOwnedObjects` (live cohort) and `ListTransactions` (history
/// cohort) keep serving, and indexing keeps advancing.
#[sim_test]
async fn disabled_balance_pipeline_gates_only_balance_reads() {
    let mut cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;
    let (name, rpc_url) =
        spawn_fullnode(&mut cluster, config_with_disabled_pipelines(&["balance"])).await;

    let transfer = transfer_to_fresh_address(&cluster, 11_000_000).await;
    // Both cohorts still index through the tip: gating is read-side only.
    wait_for_indexed(&cluster, &name, chain_tip(&cluster)).await;

    let status = sui_balance_response(&rpc_url, transfer.receiver)
        .await
        .expect_err("balance reads should be gated");
    assert_unavailable(&status, "balance");

    let owned = owned_object_ids(&rpc_url, transfer.receiver)
        .await
        .expect("owned-object reads are not gated");
    assert!(
        !owned.is_empty(),
        "the recipient's transferred coin should be listed",
    );

    let digests = transaction_digests_by_sender(&rpc_url, transfer.sender)
        .await
        .expect("ledger-history reads are not gated");
    assert!(digests.contains(&transfer.digest.to_string()));
}

/// Disabling the `transaction_bitmap` pipeline gates `ListTransactions` as
/// `Unavailable` while `GetBalance` keeps serving.
#[sim_test]
async fn disabled_transaction_bitmap_gates_list_transactions() {
    let mut cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;
    let (name, rpc_url) = spawn_fullnode(
        &mut cluster,
        config_with_disabled_pipelines(&["transaction_bitmap"]),
    )
    .await;

    let transfer = transfer_to_fresh_address(&cluster, 13_000_000).await;
    wait_for_indexed(&cluster, &name, chain_tip(&cluster)).await;

    let status = transaction_digests_by_sender(&rpc_url, transfer.sender)
        .await
        .expect_err("bitmap-backed list reads should be gated");
    assert_unavailable(&status, "transaction_bitmap");

    assert_eq!(
        sui_balance_response(&rpc_url, transfer.receiver)
            .await
            .expect("balance reads are not gated"),
        transfer.amount,
    );
}
