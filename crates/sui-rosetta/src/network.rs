// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::{Extension, Json};
use serde_json::json;
use strum::IntoEnumIterator;

use sui_types::base_types::ObjectID;
use sui_types::sui_serde::Hex;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::SUI_SYSTEM_STATE_OBJECT_ID;

use crate::errors::Error;
use crate::types::{
    Allow, Case, NetworkIdentifier, NetworkListResponse, NetworkOptionsResponse, NetworkRequest,
    NetworkStatusResponse, OperationStatus, OperationType, Peer, Version,
};
use crate::ErrorType::InternalError;
use crate::{ErrorType, ServerContext};

pub async fn list(
    Extension(context): Extension<Arc<ServerContext>>,
) -> Result<NetworkListResponse, Error> {
    Ok(NetworkListResponse {
        network_identifiers: vec![NetworkIdentifier {
            blockchain: "sui".to_string(),
            network: context.env,
        }],
    })
}

pub async fn status(
    Json(payload): Json<NetworkRequest>,
    Extension(context): Extension<Arc<ServerContext>>,
) -> Result<NetworkStatusResponse, Error> {
    context.checks_network_identifier(&payload.network_identifier)?;
    let object = context
        .state
        .get_object_read(&SUI_SYSTEM_STATE_OBJECT_ID)
        .await?;

    let system_state: SuiSystemState = bcs::from_bytes(
        object
            .into_object()?
            .data
            .try_as_move()
            .ok_or_else(|| Error::new(InternalError))?
            .contents(),
    )?;

    let peers = system_state
        .validators
        .active_validators
        .iter()
        .map(|v| Peer {
            peer_id: ObjectID::from(v.metadata.sui_address).into(),
            metadata: Some(json!({
                "public_key": Hex::from_bytes(&v.metadata.pubkey_bytes),
                "stake_amount": v.stake_amount
            })),
        })
        .collect();
    let blocks = context.blocks();
    let current_block = blocks.current_block().await?;

    Ok(NetworkStatusResponse {
        current_block_identifier: current_block.block.block_identifier,
        current_block_timestamp: current_block.block.timestamp,
        genesis_block_identifier: blocks.genesis_block_identifier(),
        oldest_block_identifier: Some(blocks.oldest_block_identifier().await?),
        sync_status: None,
        peers,
    })
}

pub async fn options(
    Json(payload): Json<NetworkRequest>,
    Extension(state): Extension<Arc<ServerContext>>,
) -> Result<NetworkOptionsResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;

    let errors = ErrorType::iter().map(Error::new).collect();

    Ok(NetworkOptionsResponse {
        version: Version {
            rosetta_version: "1.4.12".to_string(),
            node_version: env!("CARGO_PKG_VERSION").to_owned(),
            middleware_version: None,
            metadata: None,
        },
        allow: Allow {
            operation_statuses: OperationStatus::iter().collect(),
            operation_types: OperationType::iter().collect(),
            errors,
            historical_balance_lookup: false,
            timestamp_start_index: None,
            call_methods: vec![],
            balance_exemptions: vec![],
            mempool_coins: false,
            block_hash_case: Some(Case::Null),
            transaction_hash_case: Some(Case::Null),
        },
    })
}
