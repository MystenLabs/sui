// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use fastcrypto::encoding::{Encoding, Hex};
use serde_json::json;
use strum::IntoEnumIterator;

use sui_types::base_types::ObjectID;

use crate::errors::{Error, ErrorType};
use crate::types::{
    Allow, Case, NetworkIdentifier, NetworkListResponse, NetworkOptionsResponse, NetworkRequest,
    NetworkStatusResponse, OperationStatus, OperationType, Peer, SyncStatus, Version,
};
use crate::{OnlineServerContext, SuiEnv};

// This module implements the [Rosetta Network API](https://www.rosetta-api.org/docs/NetworkApi.html)

/// This endpoint returns a list of NetworkIdentifiers that the Rosetta server supports.
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/NetworkApi.html#networklist)
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
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/NetworkApi.html#networkstatus)
pub async fn status(
    State(context): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<NetworkRequest>, Error>,
) -> Result<NetworkStatusResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    // Get current epoch information including validators
    let epoch = context
        .client
        .get_epoch(None) // None means get latest epoch
        .await?;

    let system_state = epoch
        .system_state
        .ok_or_else(|| Error::DataError("Missing system state in epoch".to_string()))?;
    
    let validators = system_state
        .validators
        .ok_or_else(|| Error::DataError("Missing validators in system state".to_string()))?;

    let peers: Vec<Peer> = validators
        .active_validators
        .iter()
        .filter_map(|validator| {
            let address = validator.address.as_ref()?
                .parse::<sui_types::base_types::SuiAddress>().ok()?;
            let staking_pool = validator.staking_pool.as_ref()?;
            
            Some(Peer {
                peer_id: ObjectID::from(address).into(),
                metadata: Some(json!({
                    "stake_amount": staking_pool.sui_balance.unwrap_or(0),
                    "protocol_public_key": validator.protocol_public_key.as_ref()
                        .map(Hex::encode),
                })),
            })
        })
        .collect();
    let blocks = context.blocks();
    let current_block = blocks.current_block().await?;
    let index = current_block.block.block_identifier.index;
    let target = context
        .client
        .get_latest_checkpoint()
        .await?;

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
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/NetworkApi.html#networkoptions)
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
