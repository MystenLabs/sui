// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A mock implementation for `BridgeRequestHandlerTrait`
//! that handles requests according to preset behaviors.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use crate::crypto::BridgeAuthorityKeyPair;
use crate::crypto::BridgeAuthoritySignInfo;
use crate::error::BridgeError;
use crate::error::BridgeResult;
use crate::types::SignedBridgeAction;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use axum::Json;
use sui_types::digests::TransactionDigest;

use super::handler::BridgeRequestHandlerTrait;
use super::make_router;

#[allow(clippy::type_complexity)]
#[derive(Clone)]
pub struct BridgeRequestMockHandler {
    signer: Arc<ArcSwap<Option<BridgeAuthorityKeyPair>>>,
    sui_token_events:
        Arc<Mutex<HashMap<(TransactionDigest, u16), BridgeResult<SignedBridgeAction>>>>,
    sui_token_events_requested: Arc<Mutex<HashMap<(TransactionDigest, u16), u64>>>,
}

impl BridgeRequestMockHandler {
    pub fn new() -> Self {
        Self {
            signer: Arc::new(ArcSwap::new(Arc::new(None))),
            sui_token_events: Arc::new(Mutex::new(HashMap::new())),
            sui_token_events_requested: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn add_sui_event_response(
        &self,
        tx_digest: TransactionDigest,
        idx: u16,
        response: BridgeResult<SignedBridgeAction>,
    ) {
        self.sui_token_events
            .lock()
            .unwrap()
            .insert((tx_digest, idx), response);
    }

    pub fn get_sui_token_events_requested(
        &self,
        tx_digest: TransactionDigest,
        event_index: u16,
    ) -> u64 {
        *self
            .sui_token_events_requested
            .lock()
            .unwrap()
            .get(&(tx_digest, event_index))
            .unwrap_or(&0)
    }

    pub fn set_signer(&self, signer: BridgeAuthorityKeyPair) {
        self.signer.store(Arc::new(Some(signer)));
    }
}

#[async_trait]
impl BridgeRequestHandlerTrait for BridgeRequestMockHandler {
    async fn handle_eth_tx_hash(
        &self,
        _tx_hash_hex: String,
        _event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError> {
        unimplemented!()
    }

    async fn handle_sui_tx_digest(
        &self,
        tx_digest_base58: String,
        event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError> {
        let tx_digest = TransactionDigest::from_str(&tx_digest_base58)
            .map_err(|_e| BridgeError::InvalidTxHash)?;
        let preset = self.sui_token_events.lock().unwrap();
        if !preset.contains_key(&(tx_digest, event_idx)) {
            // Ok to panic in test
            panic!(
                "No preset handle_sui_tx_digest result for tx_digest: {}, event_idx: {}",
                tx_digest, event_idx
            );
        }
        let mut requested = self.sui_token_events_requested.lock().unwrap();
        let entry = requested.entry((tx_digest, event_idx)).or_default();
        *entry += 1;
        let result = preset.get(&(tx_digest, event_idx)).unwrap();
        if let Err(e) = result {
            return Err(e.clone());
        }
        let signed_action: &sui_types::message_envelope::Envelope<
            crate::types::BridgeAction,
            crate::crypto::BridgeAuthoritySignInfo,
        > = result.as_ref().unwrap();
        Ok(Json(signed_action.clone()))
    }

    async fn handle_governance_action(
        &self,
        action: crate::types::BridgeAction,
    ) -> Result<Json<SignedBridgeAction>, BridgeError> {
        let sig =
            BridgeAuthoritySignInfo::new(&action, self.signer.load().as_ref().as_ref().unwrap());
        let signed_action = SignedBridgeAction::new_from_data_and_sig(action, sig);
        Ok(Json(signed_action))
    }
}

pub fn run_mock_server(
    socket_address: SocketAddr,
    mock_handler: BridgeRequestMockHandler,
) -> tokio::task::JoinHandle<()> {
    tracing::info!("Starting mock server at {}", socket_address);
    let server = axum::Server::bind(&socket_address)
        .serve(make_router(Arc::new(mock_handler)).into_make_service());
    tokio::spawn(async move { server.await.unwrap() })
}
