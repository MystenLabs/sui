// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use prost_types::FieldMask;
use serde_json::json;
use strum::IntoEnumIterator;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{GetCheckpointRequest, GetEpochRequest};

use fastcrypto::encoding::Hex;
use sui_types::base_types::{ObjectID, SuiAddress};

use crate::errors::{Error, ErrorType};
use crate::types::{
    Allow, Case, NetworkIdentifier, NetworkListResponse, NetworkOptionsResponse, NetworkRequest,
    NetworkStatusResponse, OperationStatus, OperationType, Peer, SyncStatus, Version,
};
use crate::{OnlineServerContext, SuiEnv};

// This module implements the [Mesh Network API](https://docs.cdp.coinbase.com/mesh/mesh-api-spec/api-reference#network)

/// This endpoint returns a list of NetworkIdentifiers that the Rosetta server supports.
///
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/network/get-list-of-available-network)
pub async fn list(Extension(env): Extension<SuiEnv>) -> Result<NetworkListResponse, Error> {
    Ok(NetworkListResponse {
        network_identifiers: vec![NetworkIdentifier {
            blockchain: "sui".to_string(),
            network: env,
        }],
    })
}

/// This endpoint returns the current status of the network requested.
///
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/network/get-network-status)
pub async fn status(
    State(context): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<NetworkRequest>, Error>,
) -> Result<NetworkStatusResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    let mut client = context.client.clone();
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));

    let response = client
        .ledger_client()
        .get_epoch(request)
        .await?
        .into_inner();

    let system_state = response.epoch().system_state();

    let peers = system_state
        .validators()
        .active_validators()
        .iter()
        .map(|validator| {
            let address = validator
                .address()
                .parse::<SuiAddress>()
                .map_err(|e| Error::DataError(format!("Invalid validator address: {}", e)))?;
            let public_key = validator.protocol_public_key();
            let stake_amount = validator.staking_pool().sui_balance();
            Ok(Peer {
                peer_id: ObjectID::from(address).into(),
                metadata: Some(json!({
                    "public_key": Hex::from_bytes(public_key),
                    "stake_amount": stake_amount,
                })),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let blocks = context.blocks();
    let current_block = blocks.current_block().await?;
    let index = current_block.block.block_identifier.index;

    let mut client = context.client.clone();
    let checkpoint_request =
        GetCheckpointRequest::latest().with_read_mask(FieldMask::from_paths(["sequence_number"]));

    let checkpoint_response = client
        .ledger_client()
        .get_checkpoint(checkpoint_request)
        .await?
        .into_inner();

    let target = checkpoint_response.checkpoint().sequence_number();
    Ok(NetworkStatusResponse {
        current_block_identifier: current_block.block.block_identifier,
        current_block_timestamp: current_block.block.timestamp,
        genesis_block_identifier: blocks.genesis_block_identifier().await?,
        oldest_block_identifier: Some(blocks.oldest_block_identifier().await?),
        sync_status: Some(SyncStatus {
            current_index: Some(index),
            target_index: Some(target),
            stage: None,
            synced: Some(index == target),
        }),
        peers,
    })
}

/// This endpoint returns the version information and allowed network-specific types for a NetworkIdentifier.
///
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/network/get-network-options)
pub async fn options(
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<NetworkRequest>, Error>,
) -> Result<NetworkOptionsResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    let errors = ErrorType::iter().collect();
    let operation_statuses = vec![
        json!({"status": OperationStatus::Success, "successful" : true}),
        json!({"status": OperationStatus::Failure, "successful" : false}),
    ];

    Ok(NetworkOptionsResponse {
        version: Version {
            rosetta_version: "1.4.14".to_string(),
            node_version: env!("CARGO_PKG_VERSION").to_owned(),
            middleware_version: None,
            metadata: None,
        },
        allow: Allow {
            operation_statuses,
            operation_types: OperationType::iter().collect(),
            errors,
            historical_balance_lookup: true,
            timestamp_start_index: None,
            call_methods: vec![],
            balance_exemptions: vec![],
            mempool_coins: false,
            block_hash_case: Some(Case::Null),
            transaction_hash_case: Some(Case::Null),
        },
    })
}
