// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::AuthorityState,
    authority_aggregator::{AuthorityAggregator, CertificateHandler},
    authority_client::AuthorityAPI,
    node_sync::NodeSyncDigestHandler,
    safe_client::SafeClient,
};
use async_trait::async_trait;
use futures::{
    stream::{FuturesOrdered, FuturesUnordered},
    StreamExt,
};
use std::future::Future;
use std::ops::Deref;
use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};
use sui_storage::{follower_store::FollowerStore, node_sync_store::NodeSyncStore};
use sui_types::committee::StakeUnit;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests},
    batch::{TxSequenceNumber, UpdateItem},
    error::{SuiError, SuiResult},
    messages::{
        BatchInfoRequest, BatchInfoResponseItem, TransactionInfoRequest, TransactionInfoResponse,
    },
};
use tracing::{debug, error, info, trace};

#[cfg(test)]
mod configurable_batch_action_client;

#[cfg(test)]
pub(crate) mod tests;

use sui_types::messages::CertifiedTransaction;

#[derive(Copy, Clone)]
enum GossipType {
    /// Must get the full sequence of the peers it is connecting to. This is used for the full node sync logic
    /// where a full node follows all validators.
    Full,
    /// Just follow the latest updates. This is used by validators to do a best effort follow of others.
    BestEffort,
}

pub(crate) struct Follower<A> {
    pub peer_name: AuthorityName,
    client: SafeClient<A>,
    state: Arc<AuthorityState>,
    follower_store: Arc<FollowerStore>,
    max_seq: Option<TxSequenceNumber>,
    aggregator: Arc<AuthorityAggregator<A>>,
}

const REQUEST_FOLLOW_NUM_DIGESTS: u64 = 100_000;
const REFRESH_FOLLOWER_PERIOD_SECS: u64 = 60;

use super::ActiveAuthority;

pub async fn gossip_process<A>(active_authority: &ActiveAuthority<A>, degree: usize)
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    follower_process(
        active_authority,
        degree,
        GossipDigestHandler::new(),
        GossipType::BestEffort,
    )
    .await;
}

pub async fn node_sync_process<A>(
    active_authority: &ActiveAuthority<A>,
    degree: usize,
    node_sync_store: Arc<NodeSyncStore>,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let state = active_authority.state.clone();
    let aggregator = active_authority.net.load().clone();
    follower_process(
        active_authority,
        degree,
        NodeSyncDigestHandler::new(state, aggregator, node_sync_store),
        GossipType::Full,
    )
    .await;
}

async fn follower_process<A, Handler: DigestHandler<A> + Clone>(
    active_authority: &ActiveAuthority<A>,
    degree: usize,
    handler: Handler,
    gossip_type: GossipType,
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
            peer_names = peer_names
                .into_iter()
                .filter(|name| committee.authority_exists(name))
                .collect();
        }
        let mut k = 0;
        while gossip_tasks.len() < target_num_tasks {
            // Find out what is the earliest time that we are allowed to reconnect
            // to at least 2f+1 nodes.
            let next_connect = local_active
                .minimum_wait_for_majority_honest_available()
                .await;
            debug!(
                "Waiting for {:?}",
                next_connect - tokio::time::Instant::now()
            );
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
                let follower = Follower::new(name, &local_active_ref_copy, gossip_type);
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
    let (finished_name, _result) = gossip_tasks.select_next_some().await;
    if let Err(err) = _result {
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

pub struct LocalCertificateHandler {
    pub state: Arc<AuthorityState>,
}

#[async_trait]
impl CertificateHandler for LocalCertificateHandler {
    async fn handle(
        &self,
        certificate: CertifiedTransaction,
    ) -> SuiResult<TransactionInfoResponse> {
        self.state.handle_certificate(certificate).await
    }

    fn destination_name(&self) -> String {
        format!("{:?}", self.state.name)
    }
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
    /// handle_digest
    async fn handle_digest(&self, follower: &Follower<A>, digest: ExecutionDigests) -> SuiResult;
}

#[derive(Clone, Copy)]
struct GossipDigestHandler {}

impl GossipDigestHandler {
    fn new() -> Self {
        Self {}
    }

    async fn process_response<A>(
        follower: &Follower<A>,
        response: TransactionInfoResponse,
    ) -> Result<(), SuiError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        if let Some(certificate) = response.certified_transaction {
            // Process the certificate from one authority to ourselves
            follower
                .aggregator
                .sync_authority_source_to_destination(
                    certificate,
                    follower.peer_name,
                    &LocalCertificateHandler {
                        state: follower.state.clone(),
                    },
                )
                .await?;
            follower.state.metrics.gossip_sync_count.inc();
            Ok(())
        } else {
            // The authority did not return the certificate, despite returning info
            // But it should know the certificate!
            Err(SuiError::ByzantineAuthoritySuspicion {
                authority: follower.peer_name,
            })
        }
    }
}

#[async_trait]
impl<A> DigestHandler<A> for GossipDigestHandler
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    async fn handle_digest(&self, follower: &Follower<A>, digest: ExecutionDigests) -> SuiResult {
        if !follower
            .state
            .database
            .effects_exists(&digest.transaction)?
        {
            // Download the certificate
            let response = follower
                .client
                .handle_transaction_info_request(TransactionInfoRequest::from(digest.transaction))
                .await?;
            Self::process_response(follower, response).await?;
        }
        Ok(())
    }
}

impl<A> Follower<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        peer_name: AuthorityName,
        active_authority: &ActiveAuthority<A>,
        gossip_type: GossipType,
    ) -> Self {
        let start_seq = match active_authority
            .follower_store
            .get_next_sequence(&peer_name)
        {
            Err(_e) => {
                // If there was no start sequence found for this peer, it is likely a new peer
                // that has just joined the network, start at 0.
                info!(peer = ?peer_name, "New gossip peer has joined");

                // TODO: How does this interfere with reconfigurations, etc?
                //       Do we start the seq number at zero for each new epoch?
                0
            }
            Ok(s) => s.unwrap_or(0),
        };

        let max_seq = match gossip_type {
            GossipType::BestEffort => None,
            GossipType::Full => Some(start_seq),
        };

        debug!(peer = ?peer_name, ?start_seq, "Restarting follower at sequence");

        Self {
            peer_name,
            client: active_authority.net.load().authority_clients[&peer_name].clone(),
            state: active_authority.state.clone(),
            follower_store: active_authority.follower_store.clone(),
            max_seq,
            aggregator: active_authority.net.load().clone(),
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
        let mut batch_seq_to_record = VecDeque::new();

        let mut latest_seq = self.max_seq;

        let req = BatchInfoRequest {
            start: latest_seq,
            length: REQUEST_FOLLOW_NUM_DIGESTS,
        };

        let mut last_seq_in_cur_batch: TxSequenceNumber = 0;
        let mut streamx = Box::pin(self.client.handle_batch_stream(req).await?);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    // No matter what happens we do not spend too much time on any peer.
                    break;
                },

                items = &mut streamx.next() => {
                    match items {
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) )) => {
                            let next_seq = signed_batch.batch.next_sequence_number;
                            debug!(?peer, batch_next_seq = ?next_seq, "Received signed batch");
                            batch_seq_to_record.push_back((next_seq, last_seq_in_cur_batch));
                            if let Some(max_seq) = latest_seq {
                                if next_seq < max_seq {
                                    info!("Gossip sequence number unexpected: found {:?} but previously received {:?}", next_seq, max_seq);
                                }
                            }
                        },

                        // Upon receiving a transaction digest, store it if it is not processed already.
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digests))))) => {
                            trace!(?peer, ?digests, ?seq, "received tx from peer");

                            // track the last observed sequence in a batch, so we can tell when the
                            // batch has been fully processed.
                            last_seq_in_cur_batch = seq;

                            let fut = handler.handle_digest(self, digests);
                            results.push(async move {
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
                            tokio::time::sleep(Duration::from_secs(REFRESH_FOLLOWER_PERIOD_SECS / 12)).await;
                            let req = BatchInfoRequest {
                                start: latest_seq,
                                length: REQUEST_FOLLOW_NUM_DIGESTS,
                            };
                            streamx = Box::pin(self.client.handle_batch_stream(req).await?);
                        },
                    }
                },

                result = &mut results.next() , if !results.is_empty() => {
                    let (seq, digests) = result.unwrap()?;
                    trace!(?peer, ?seq, ?digests, "digest handler finished");

                    while let Some((batch_seq, last_seq_in_batch)) = batch_seq_to_record.front() {
                        if seq < *last_seq_in_batch {
                            break;
                        }
                        self.follower_store.record_next_sequence(&self.peer_name, *batch_seq)?;

                        // Here we always set the minimum next sequence number to make progress
                        // over the sequence of the peer, even if we started with a best effort
                        // None initial sequence number for gossip.
                        latest_seq = Some(*batch_seq);
                        batch_seq_to_record.pop_front();
                    }
                }
            };
        }
        Ok(())
    }
}
