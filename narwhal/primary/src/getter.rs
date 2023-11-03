// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::PrimaryMetrics;
use crate::verifier::Verifier;
use anemo::Request;
use config::{AuthorityIdentifier, Committee, Stake};
use crypto::NetworkPublicKey;
use mysten_metrics::metered_channel::Sender;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use network::anemo_ext::NetworkExt as _;
use parking_lot::Mutex;
use rand::{rngs::ThreadRng, seq::SliceRandom};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::Instant;
use tracing::{debug, instrument, trace};
use types::error::{DagError, DagResult};
use types::{
    GetHeadersRequest, GetHeadersResponse, HeaderKey, PrimaryToPrimaryClient, SignedHeader,
};

const GET_DELAY: Duration = Duration::from_millis(100);
const GET_RETRY_INTERVAL: Duration = Duration::from_millis(200);

/// The HeaderGetter is responsible for getting specific headers that this primary is missing
/// from peers.
pub(crate) struct HeaderGetter {
    state: Arc<HeaderGetterState>,
}

/// Thread-safe internal state of HeaderFetcher shared with its fetch task.
struct HeaderGetterState {
    /// Identity of the current authority.
    authority_id: AuthorityIdentifier,
    /// Committee of the current epoch.
    committee: Committee,
    /// Stake weight per peer.
    weights: Vec<(NetworkPublicKey, Stake)>,
    /// Network client to fetch headers from other primaries.
    network: anemo::Network,
    /// Verifies then sends headers to Core for processing.
    verifier: Arc<Verifier>,
    /// Sends verified headers to Core for processing.
    tx_verified_headers: Sender<SignedHeader>,
    /// Inflight headers being retrieved.
    inflight: Mutex<InflightState>,
    /// The metrics handler
    metrics: Arc<PrimaryMetrics>,
}

struct InflightState {
    last_get: BTreeMap<HeaderKey, Instant>,
    last_gc: Instant,
}

impl Default for InflightState {
    fn default() -> Self {
        Self {
            last_get: Default::default(),
            last_gc: Instant::now(),
        }
    }
}

impl HeaderGetter {
    pub fn new(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        network: anemo::Network,
        verifier: Arc<Verifier>,
        tx_verified_headers: Sender<SignedHeader>,
        metrics: Arc<PrimaryMetrics>,
    ) -> Self {
        let weights = committee
            .others_primaries_by_id(authority_id)
            .into_iter()
            .map(|(id, _, network_key)| (network_key, committee.authority(&id).unwrap().stake()))
            .collect();
        let state = HeaderGetterState {
            authority_id,
            committee,
            weights,
            network,
            verifier,
            tx_verified_headers,
            inflight: Default::default(),
            metrics,
        };
        Self {
            state: Arc::new(state),
        }
    }

    pub(crate) fn get_missing(&self, missing: Vec<(HeaderKey, Instant)>) {
        let mut inflight = self.state.inflight.lock();
        let now = Instant::now();
        let missing = missing
            .into_iter()
            .filter(|(k, t)| {
                if now - *t < GET_DELAY {
                    return false;
                }
                if let Some(start) = inflight.last_get.get(k) {
                    if now - *start < GET_RETRY_INTERVAL {
                        return false;
                    }
                }
                inflight.last_get.insert(*k, now);
                true
            })
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return;
        }
        let state = self.state.clone();
        spawn_monitored_task!(async move {
            let _scope = monitored_scope("Getter::task");
            // Send request to get headers.
            let request = GetHeadersRequest {
                missing: missing.into_iter().map(|(k, _)| k).collect(),
            };
            let Ok(response) = get_headers_helper(&state, request).await else {
                return Ok::<(), DagError>(());
            };

            let response = response.into_body();
            let num_headers = response.headers.len() as u64;
            process_headers_helper(&state, response).await?;
            debug!("Successfully got and processed {num_headers} headers");

            let mut inflight = state.inflight.lock();
            let now = Instant::now();
            if now - inflight.last_gc > Duration::from_secs(2) {
                inflight
                    .last_get
                    .retain(|_, v| now - *v < GET_RETRY_INTERVAL);
                inflight.last_gc = now;
            }

            Ok::<(), DagError>(())
        });
    }
}

/// Fetches headers from other primaries concurrently, with ~5 sec interval between each request.
/// Terminates after the 1st successful response is received.
#[instrument(level = "debug", skip_all)]
async fn get_headers_helper(
    state: &HeaderGetterState,
    request: GetHeadersRequest,
) -> Result<anemo::Response<GetHeadersResponse>, anemo::rpc::Status> {
    let _scope = monitored_scope("Getter::request");
    trace!("Start sending get headers requests");
    let peer = state
        .weights
        .choose_weighted(&mut ThreadRng::default(), |(_, stake)| *stake)
        .unwrap()
        .0
        .clone();

    debug!("Starting to get headers");
    let request = Request::new(request).with_timeout(Duration::from_secs(2));
    let wait_peer = state.network.waiting_peer(anemo::PeerId(peer.0.to_bytes()));
    let mut client = PrimaryToPrimaryClient::new(wait_peer);
    debug!("Sending out fetch request in parallel to {peer}");
    client.get_headers(request).await
}

#[instrument(level = "debug", skip_all)]
async fn process_headers_helper(
    state: &HeaderGetterState,
    response: GetHeadersResponse,
) -> DagResult<()> {
    trace!("Start sending headers to processing");

    let _scope = monitored_scope("Fetcher::verify");
    for header in &response.headers {
        state.verifier.verify(header).await?;
    }
    for header in response.headers {
        state
            .tx_verified_headers
            .send(header)
            .await
            .map_err(|_| DagError::ShuttingDown)?;
    }

    Ok(())
}
