// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::AuthorityState,
    authority_aggregator::{AuthorityAggregator, ConfirmationTransactionHandler},
    authority_client::AuthorityAPI,
    safe_client::SafeClient,
};
use async_trait::async_trait;
use futures::stream::FuturesOrdered;
use futures::{stream::FuturesUnordered, StreamExt};
use std::future::Future;
use std::ops::Deref;
use std::{collections::HashSet, sync::Arc, time::Duration};
use sui_storage::follower_store::FollowerStore;
use sui_types::committee::StakeUnit;
use sui_types::{
    base_types::AuthorityName,
    batch::{TxSequenceNumber, UpdateItem},
    error::{SuiError, SuiResult},
    messages::{
        BatchInfoRequest, BatchInfoResponseItem, ConfirmationTransaction, TransactionInfoRequest,
        TransactionInfoResponse,
    },
};
use tracing::{debug, error, info};

#[cfg(test)]
mod configurable_batch_action_client;

#[cfg(test)]
pub(crate) mod tests;

struct PeerGossip<A> {
    peer_name: AuthorityName,
    client: SafeClient<A>,
    state: Arc<AuthorityState>,
    follower_store: Arc<FollowerStore>,
    max_seq: Option<TxSequenceNumber>,
    aggregator: Arc<AuthorityAggregator<A>>,
}

const EACH_ITEM_DELAY_MS: u64 = 1_000;
const REQUEST_FOLLOW_NUM_DIGESTS: u64 = 100_000;
const REFRESH_FOLLOWER_PERIOD_SECS: u64 = 60;

use super::ActiveAuthority;

pub async fn gossip_process<A>(active_authority: &ActiveAuthority<A>, degree: usize)
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    gossip_process_with_start_seq(active_authority, degree, None).await
}

pub async fn gossip_process_with_start_seq<A>(
    _active_authority: &ActiveAuthority<A>,
    degree: usize,
    start_seq: Option<TxSequenceNumber>,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Make a clone of the active authority and committee, and keep using it until epoch changes.
    let mut local_active = Arc::new(_active_authority.clone());
    let mut committee = local_active.state.committee.load().deref().clone();

    // Number of tasks at most "degree" and no more than committee - 1
    let mut target_num_tasks = usize::min(committee.voting_rights.len() - 1, degree);

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
        if _active_authority.state.committee.load().epoch != committee.epoch {
            // If epoch has changed, we need to make a new copy of the active authority,
            // and update all local variables.
            // We also need to remove any authority that's no longer a valid validator
            // from the list of peer names.
            // It's ok to keep the existing gossip tasks running even for peers that are no longer
            // validators, and let them end naturally.
            local_active = Arc::new(_active_authority.clone());
            committee = local_active.state.committee.load().deref().clone();
            target_num_tasks = usize::min(committee.voting_rights.len() - 1, degree);
            peer_names = peer_names
                .into_iter()
                .filter(|name| committee.voting_rights.contains_key(name))
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
            gossip_tasks.push(async move {
                let peer_gossip = PeerGossip::new(name, &local_active_ref_copy, start_seq);
                // Add more duration if we make more than 1 to ensure overlap
                debug!(peer = ?name, "Starting gossip from peer");
                peer_gossip
                    .start(Duration::from_secs(REFRESH_FOLLOWER_PERIOD_SECS + k * 15))
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

struct LocalConfirmationTransactionHandler {
    state: Arc<AuthorityState>,
}

#[async_trait]
impl ConfirmationTransactionHandler for LocalConfirmationTransactionHandler {
    async fn handle(&self, cert: ConfirmationTransaction) -> SuiResult<TransactionInfoResponse> {
        self.state.handle_confirmation_transaction(cert).await
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
    let mut tries_remaining = active_authority.state.committee.load().voting_rights.len();
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

impl<A> PeerGossip<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        peer_name: AuthorityName,
        active_authority: &ActiveAuthority<A>,
        start_seq: Option<TxSequenceNumber>,
    ) -> PeerGossip<A> {
        // TODO: for validator gossip, we should always use None as the start_seq, but we should
        // consult the start_seq we retrieved from the db to make sure that the peer is giving
        // us new txes.
        let start_seq = match active_authority
            .follower_store
            .get_next_sequence(&peer_name)
        {
            Err(e) => {
                error!("Could not load next sequence from follower store, defaulting to None. Error: {}", e);
                // It might seem like a good idea to return start_seq here, but if we are running
                // as a full node start_seq will be Some(0), and if the gossip process is repeatedly
                // restarting, we would in that case repeatedly re-request all txes from the
                // beginning of the epoch which could DoS the validators we are following.
                None
            }
            Ok(s) => s.or(start_seq),
        };

        Self {
            peer_name,
            client: active_authority.net.load().authority_clients[&peer_name].clone(),
            state: active_authority.state.clone(),
            follower_store: active_authority.follower_store.clone(),
            max_seq: start_seq,
            aggregator: active_authority.net.load().clone(),
        }
    }

    pub async fn start(mut self, duration: Duration) -> (AuthorityName, Result<(), SuiError>) {
        let peer_name = self.peer_name;
        let result = self.peer_gossip_for_duration(duration).await;
        (peer_name, result)
    }

    async fn peer_gossip_for_duration(&mut self, duration: Duration) -> Result<(), SuiError> {
        // Global timeout, we do not exceed this time in this task.
        let mut timeout = Box::pin(tokio::time::sleep(duration));
        let mut queue = FuturesOrdered::new();

        let req = BatchInfoRequest {
            start: self.max_seq,
            length: REQUEST_FOLLOW_NUM_DIGESTS,
        };

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
                            self.follower_store.record_next_sequence(&self.peer_name, next_seq)?;
                        },

                        // Upon receiving a transaction digest, store it if it is not processed already.
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digest))))) => {
                            if !self.state.database.effects_exists(&digest.transaction)? {
                                queue.push(async move {
                                    tokio::time::sleep(Duration::from_millis(EACH_ITEM_DELAY_MS)).await;
                                    digest
                                });
                                self.state.metrics.gossip_queued_count.inc();

                            }
                            self.max_seq = Some(seq + 1);
                        },

                        // Return any errors.
                        Some(Err( err )) => {
                            return Err(err);
                        },

                        // The stream has closed, re-request:
                        None => {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            let req = BatchInfoRequest {
                                start: self.max_seq,
                                length: REQUEST_FOLLOW_NUM_DIGESTS,
                            };
                            streamx = Box::pin(self.client.handle_batch_stream(req).await?);
                        },
                    }
                },
                digest = &mut queue.next() , if !queue.is_empty() => {
                    let digest = digest.unwrap();
                    if !self.state.database.effects_exists(&digest.transaction)? {
                        // Download the certificate
                        let response = self.client.handle_transaction_info_request(TransactionInfoRequest::from(digest.transaction)).await?;
                        self.process_response(response).await?;
                    }
                }
            };
        }
        Ok(())
    }

    async fn process_response(&self, response: TransactionInfoResponse) -> Result<(), SuiError> {
        if let Some(certificate) = response.certified_transaction {
            // Process the certificate from one authority to ourselves
            self.aggregator
                .sync_authority_source_to_destination(
                    ConfirmationTransaction { certificate },
                    self.peer_name,
                    LocalConfirmationTransactionHandler {
                        state: self.state.clone(),
                    },
                )
                .await?;
            self.state.metrics.gossip_sync_count.inc();
            Ok(())
        } else {
            // The authority did not return the certificate, despite returning info
            // But it should know the certificate!
            Err(SuiError::ByzantineAuthoritySuspicion {
                authority: self.peer_name,
            })
        }
    }
}
