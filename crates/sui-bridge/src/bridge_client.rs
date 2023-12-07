// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `BridgeClient` talks to BridgeNode.

use arc_swap::ArcSwap;
use std::sync::Arc;

use crate::error::BridgeResult;
use crate::events::SuiBridgeEvent;
use crate::server::APPLICATION_JSON;
use crate::types::{
    BridgeCommittee, BridgeEvent, SignedBridgeEvent, VerifiedSignedBridgeEvent,
};
use crate::crypto::verify_signed_bridge_event;

#[derive(Clone)]
pub struct BridgeClient {
    inner: reqwest::Client,
    committee: Arc<ArcSwap<BridgeCommittee>>,
    base_url: String,
}

impl BridgeClient {
    pub fn new<S: Into<String>>(base_url: S, committee: Arc<ArcSwap<BridgeCommittee>>) -> Self {
        Self {
            inner: reqwest::Client::new(),
            base_url: base_url.into(),
            committee,
        }
    }

    // Important: the paths need to match the ones in server.rs
    fn bridge_event_to_path(event: &BridgeEvent) -> String {
        match event {
            BridgeEvent::Sui(SuiBridgeEvent::SuiToEthTokenBridgeV1(e)) => format!(
                "sign/bridge_tx/sui/eth/{}/{}",
                e.sui_tx_digest, e.sui_tx_event_index
            ),
            // TODO add other events
            _ => unimplemented!(),
        }
    }

    pub async fn request_sign_bridge_event(
        &self,
        e: BridgeEvent,
    ) -> BridgeResult<VerifiedSignedBridgeEvent> {
        let url = format!("{}/{}", self.base_url, Self::bridge_event_to_path(&e));
        let signed_bridge_event: SignedBridgeEvent = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, APPLICATION_JSON)
            .send()
            .await?
            .json()
            .await?;
        verify_signed_bridge_event(signed_bridge_event, &self.committee.load())
    }
}
