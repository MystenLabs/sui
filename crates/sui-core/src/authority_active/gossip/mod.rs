// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{authority::AuthorityState, authority_client::AuthorityAPI, safe_client::SafeClient};
use async_trait::async_trait;
use futures::{
    future::BoxFuture,
    stream::{FuturesOrdered, FuturesUnordered},
    StreamExt,
};
use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntGauge, Registry,
};
use std::future::Future;
use std::ops::Deref;
use std::{collections::HashSet, sync::Arc, time::Duration};
use sui_types::committee::StakeUnit;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests},
    batch::{TxSequenceNumber, UpdateItem},
    error::{SuiError, SuiResult},
    messages::{
        BatchInfoRequest, BatchInfoResponseItem, TransactionInfoRequest,
        VerifiedTransactionInfoResponse,
    },
};
use tap::TapFallible;
use tracing::{debug, error, info, trace};

#[cfg(test)]
mod configurable_batch_action_client;

#[cfg(test)]
pub(crate) mod tests;

pub(crate) struct Follower<A> {
    pub peer_name: AuthorityName,
    client: SafeClient<A>,
    state: Arc<AuthorityState>,
}

const REQUEST_FOLLOW_NUM_DIGESTS: u64 = 100_000;
const REFRESH_FOLLOWER_PERIOD_SECS: u64 = 60;

use super::ActiveAuthority;

/// See the `new` function for description for each metrics.
#[derive(Clone, Debug)]
pub struct GossipMetrics {
    pub concurrent_followed_validators: IntGauge,
    pub reconnect_interval_ms: IntGauge,
    pub total_tx_received: IntCounter,
    pub total_batch_received: IntCounter,
    pub wait_for_finality_latency_sec: Histogram,
    pub total_attempts_cert_downloads: IntCounter,
    pub total_successful_attempts_cert_downloads: IntCounter,
    pub follower_stream_duration: Histogram,
}

const WAIT_FOR_FINALITY_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];
const FOLLOWER_STREAM_DURATION_SEC_BUCKETS: &[f64] = &[
    0.1, 1., 5., 10., 20., 30., 40., 50., 60., 90., 120., 180., 240., 300.,
];

impl GossipMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            concurrent_followed_validators: register_int_gauge_with_registry!(
                "gossip_concurrent_followed_validators",
                "Number of validators being followed concurrently at the moment.",
                registry,
            )
            .unwrap(),
            reconnect_interval_ms: register_int_gauge_with_registry!(
                "gossip_reconnect_interval_ms",
                "Interval to start the next gossip/node sync task, in millisec",
                registry,
            )
            .unwrap(),
            total_tx_received: register_int_counter_with_registry!(
                "gossip_total_tx_received",
                "Total number of transactions received through gossip/node sync",
                registry,
            )
            .unwrap(),
            total_batch_received: register_int_counter_with_registry!(
                "gossip_total_batch_received",
                "Total number of signed batches received through gossip/node sync",
                registry,
            )
            .unwrap(),
            wait_for_finality_latency_sec: register_histogram_with_registry!(
                "gossip_wait_for_finality_latency_sec",
                "Latency histogram for gossip/node sync process to wait for txs to become final, in seconds",
                WAIT_FOR_FINALITY_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            total_attempts_cert_downloads: register_int_counter_with_registry!(
                "gossip_total_attempts_cert_downloads",
                "Total number of certs/effects download attempts through gossip/node sync process",
                registry,
            )
            .unwrap(),
            total_successful_attempts_cert_downloads: register_int_counter_with_registry!(
                "gossip_total_successful_attempts_cert_downloads",
                "Total number of success certs/effects downloads through gossip/node sync process",
                registry,
            )
            .unwrap(),
            follower_stream_duration: register_histogram_with_registry!(
                "follower_stream_duration",
                "Latency histogram of the duration of the follower streams to peers, in seconds",
                FOLLOWER_STREAM_DURATION_SEC_BUCKETS.to_vec(),
                registry,
            )
                .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}

pub async fn gossip_process<A>(active_authority: &ActiveAuthority<A>, degree: usize)
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    follower_process(
        active_authority,
        degree,
        GossipDigestHandler::new(active_authority.gossip_metrics.clone()),
    )
    .await;
}

async fn follower_process<A, Handler: DigestHandler<A> + Clone>(
    active_authority: &ActiveAuthority<A>,
    degree: usize,
    handler: Handler,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Make a clone of the active authority and committee, and keep using it until epoch changes.
    let mut local_active = Arc::new(active_authority.clone());
    let mut committee = local_active.state.committee.load().deref().clone();

    // Number of tasks at most "degree" and no more than committee - 1
    let mut target_num_tasks = usize::min(committee.num_members() - 1, degree);

    // If we do not expect to connect to anyone
    if target_num_tasks == 0 {
        info!("Turning off gossip mechanism");
        return;
    }
    info!("Turning on gossip mechanism");

    // Keep track of names of active peers
    let mut peer_names = HashSet::new();
    let mut gossip_tasks = FuturesUnordered::new();
    let metrics_concurrent_followed_validators = &active_authority
        .gossip_metrics
        .concurrent_followed_validators;
    let metrics_reconnect_interval_ms = &active_authority.gossip_metrics.reconnect_interval_ms;
    loop {
        if active_authority.state.committee.load().epoch != committee.epoch {
            // If epoch has changed, we need to make a new copy of the active authority,
            // and update all local variables.
            // We also need to remove any authority that's no longer a valid validator
            // from the list of peer names.
            // It's ok to keep the existing gossip tasks running even for peers that are no longer
            // validators, and let them end naturally.
            local_active = Arc::new(active_authority.clone());
            committee = local_active.state.committee.load().deref().clone();
            target_num_tasks = usize::min(committee.num_members() - 1, degree);
            peer_names.retain(|name| committee.authority_exists(name));
        }

        let mut k = 0;
        while gossip_tasks.len() < target_num_tasks {
            // Find out what is the earliest time that we are allowed to reconnect
            // to at least 2f+1 nodes.
            let next_connect = local_active
                .minimum_wait_for_majority_honest_available()
                .await;
            let wait_duration = next_connect - tokio::time::Instant::now();
            debug!("Waiting for {:?}", wait_duration);
            metrics_reconnect_interval_ms.set(wait_duration.as_millis() as i64);

            tokio::time::sleep_until(next_connect).await;

            let name_result =
                select_gossip_peer(local_active.state.name, peer_names.clone(), &local_active)
                    .await;
            if name_result.is_err() {
                continue;
            }
            let name = name_result.unwrap();

            peer_names.insert(name);
            let local_active_ref_copy = local_active.clone();
            let handler_clone = handler.clone();
            gossip_tasks.push(async move {
                let follower = Follower::new(name, &local_active_ref_copy);
                // Add more duration if we make more than 1 to ensure overlap
                debug!(peer = ?name, "Starting gossip from peer");
                follower
                    .start(
                        Duration::from_secs(REFRESH_FOLLOWER_PERIOD_SECS + k * 15),
                        handler_clone,
                    )
                    .await
            });
            k += 1;

            // If we have already used all the good stake, then stop here and
            // wait for some node to become available.
            let total_stake_used = peer_names
                .iter()
                .map(|name| committee.weight(name))
                .sum::<StakeUnit>()
                + committee.weight(&local_active.state.name);
            if total_stake_used >= committee.quorum_threshold() {
                break;
            }
        }

        // If we have no peers no need to wait for one
        if gossip_tasks.is_empty() {
            continue;
        }

        metrics_concurrent_followed_validators.set(gossip_tasks.len() as i64);
        wait_for_one_gossip_task_to_finish(&local_active, &mut peer_names, &mut gossip_tasks).await;
    }
}

async fn wait_for_one_gossip_task_to_finish<A>(
    active_authority: &ActiveAuthority<A>,
    peer_names: &mut HashSet<AuthorityName>,
    gossip_tasks: &mut FuturesUnordered<
        impl Future<Output = (AuthorityName, Result<(), SuiError>)>,
    >,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let (finished_name, result) = gossip_tasks.select_next_some().await;
    if let Err(err) = result {
        active_authority.set_failure_backoff(finished_name).await;
        active_authority.state.metrics.gossip_task_error_count.inc();
        error!(peer = ?finished_name, "Peer returned error: {:?}", err);
    } else {
        active_authority.set_success_backoff(finished_name).await;
        active_authority
            .state
            .metrics
            .gossip_task_success_count
            .inc();
        debug!(peer = ?finished_name, "End gossip from peer");
    }
    peer_names.remove(&finished_name);
}

pub async fn select_gossip_peer<A>(
    my_name: AuthorityName,
    peer_names: HashSet<AuthorityName>,
    active_authority: &ActiveAuthority<A>,
) -> Result<AuthorityName, SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Make sure we exit loop by limiting the number of tries to choose peer
    // where n is the total number of committee members.
    let mut tries_remaining = active_authority.state.committee.load().num_members();
    while tries_remaining > 0 {
        let name = *active_authority.state.committee.load().sample();
        if peer_names.contains(&name)
            || name == my_name
            || !active_authority.can_contact(name).await
        {
            tries_remaining -= 1;
            tokio::time::sleep(Duration::from_millis(10)).await;
            continue;
        }
        return Ok(name);
    }
    Err(SuiError::GenericAuthorityError {
        error: "Could not connect to any peer".to_string(),
    })
}

#[async_trait]
pub(crate) trait DigestHandler<A> {
    type DigestResult: Future<Output = SuiResult>;

    /// handle_digest
    async fn handle_digest(
        &self,
        follower: &Follower<A>,
        digest: ExecutionDigests,
    ) -> SuiResult<Self::DigestResult>;

    fn get_metrics(&self) -> &GossipMetrics;
}

#[derive(Clone)]
struct GossipDigestHandler {
    metrics: GossipMetrics,
}

impl GossipDigestHandler {
    fn new(metrics: GossipMetrics) -> Self {
        Self { metrics }
    }

    async fn process_response(
        state: Arc<AuthorityState>,
        peer_name: AuthorityName,
        response: VerifiedTransactionInfoResponse,
    ) -> Result<(), SuiError> {
        if let Some(certificate) = response.certified_transaction {
            let digest = *certificate.digest();
            state
                .add_pending_certificates(vec![(digest, Some(certificate))])
                .tap_err(|e| error!(?digest, "add_pending_certificates failed: {}", e))?;

            state.metrics.gossip_sync_count.inc();
            Ok(())
        } else {
            // The authority did not return the certificate, despite returning info
            // But it should know the certificate!
            Err(SuiError::ByzantineAuthoritySuspicion {
                authority: peer_name,
                reason: "Gossip peer is expected to have certificate".to_string(),
            })
        }
    }
}

#[async_trait]
impl<A> DigestHandler<A> for GossipDigestHandler
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    type DigestResult = BoxFuture<'static, SuiResult>;

    async fn handle_digest(
        &self,
        follower: &Follower<A>,
        digest: ExecutionDigests,
    ) -> SuiResult<Self::DigestResult> {
        let state = follower.state.clone();
        let client = follower.client.clone();
        let name = follower.peer_name;
        Ok(Box::pin(async move {
            if !state.database.effects_exists(&digest.transaction)? {
                // Download the certificate
                let response = client
                    .handle_transaction_info_request(TransactionInfoRequest::from(
                        digest.transaction,
                    ))
                    .await?;
                Self::process_response(state, name, response).await?;
            }
            Ok(())
        }))
    }

    fn get_metrics(&self) -> &GossipMetrics {
        &self.metrics
    }
}

impl<A> Follower<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(peer_name: AuthorityName, active_authority: &ActiveAuthority<A>) -> Self {
        debug!(peer = ?peer_name, "Restarting follower");

        Self {
            peer_name,
            client: active_authority.net.load().authority_clients[&peer_name].clone(),
            state: active_authority.state.clone(),
        }
    }

    pub async fn start<Handler: DigestHandler<A>>(
        self,
        duration: Duration,
        handler: Handler,
    ) -> (AuthorityName, Result<(), SuiError>) {
        let peer_name = self.peer_name;
        let result = self.follow_peer_for_duration(duration, handler).await;
        (peer_name, result)
    }

    async fn follow_peer_for_duration<'a, Handler: DigestHandler<A>>(
        &self,
        duration: Duration,
        handler: Handler,
    ) -> SuiResult {
        let peer = self.peer_name;
        // Global timeout, we do not exceed this time in this task.
        let mut timeout = Box::pin(tokio::time::sleep(duration));
        let mut results = FuturesOrdered::new();

        let req = BatchInfoRequest {
            start: None,
            length: REQUEST_FOLLOW_NUM_DIGESTS,
        };
        let mut streamx = Box::pin(self.client.handle_batch_stream(req).await?);
        let metrics = handler.get_metrics();
        let mut timer = metrics.follower_stream_duration.start_timer();

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    // No matter what happens we do not spend too much time on any peer.
                    break;
                },

                items = &mut streamx.next() => {
                    match items {
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) )) => {
                            metrics.total_batch_received.inc();

                            let next_seq = signed_batch.data().next_sequence_number;
                            debug!(?peer, batch_next_seq = ?next_seq, "Received signed batch");
                        },

                        // Upon receiving a transaction digest, store it if it is not processed already.
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digests))))) => {
                            trace!(?peer, ?digests, ?seq, "received tx from peer");
                            metrics.total_tx_received.inc();

                            let fut = handler.handle_digest(self, digests).await?;
                            results.push_back(async move {
                                fut.await?;
                                Ok::<(TxSequenceNumber, ExecutionDigests), SuiError>((seq, digests))
                            });

                            self.state.metrics.gossip_queued_count.inc();
                        },

                        // Return any errors.
                        Some(Err( err )) => {
                            return Err(err);
                        },

                        // The stream has closed, re-request:
                        None => {
                            timer.stop_and_record();
                            timer = metrics.follower_stream_duration.start_timer();
                            info!(peer = ?self.peer_name, "Gossip stream was closed. Restarting");
                            self.client.metrics_total_times_reconnect_follower_stream.inc();
                            tokio::time::sleep(Duration::from_secs(REFRESH_FOLLOWER_PERIOD_SECS / 12)).await;
                            let req = BatchInfoRequest {
                                start: None,
                                length: REQUEST_FOLLOW_NUM_DIGESTS,
                            };
                            streamx = Box::pin(self.client.handle_batch_stream(req).await?);
                        },
                    }
                },

                result = &mut results.next() , if !results.is_empty() => {
                    let (seq, digests) = result.unwrap()?;
                    trace!(?peer, ?seq, ?digests, "digest handler finished");
                }
            };
        }
        timer.stop_and_record();
        Ok(())
    }
}
