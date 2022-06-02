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
use std::{collections::HashSet, sync::Arc, time::Duration};
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
    active_authority: &ActiveAuthority<A>,
    degree: usize,
    start_seq: Option<TxSequenceNumber>,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // A copy of the committee
    let committee = &active_authority.net.committee;

    // Number of tasks at most "degree" and no more than committee - 1
    let target_num_tasks: usize = usize::min(committee.voting_rights.len() - 1, degree);

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
        let mut k = 0;
        while gossip_tasks.len() < target_num_tasks {
            // Find out what is the earliest time that we are allowed to reconnect
            // to at least 2f+1 nodes.
            let next_connect = active_authority
                .minimum_wait_for_majority_honest_available()
                .await;
            debug!(
                "Waiting for {:?}",
                next_connect - tokio::time::Instant::now()
            );
            tokio::time::sleep_until(next_connect).await;

            let name_result = select_gossip_peer(
                active_authority.state.name,
                peer_names.clone(),
                active_authority,
            )
            .await;
            if name_result.is_err() {
                continue;
            }
            let name = name_result.unwrap();

            peer_names.insert(name);
            gossip_tasks.push(async move {
                let peer_gossip = PeerGossip::new(name, active_authority, start_seq);
                // Add more duration if we make more than 1 to ensure overlap
                debug!("Starting gossip from peer {:?}", name);
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
                + committee.weight(&active_authority.state.name);
            if total_stake_used >= committee.quorum_threshold() {
                break;
            }
        }

        // If we have no peers no need to wait for one
        if gossip_tasks.is_empty() {
            continue;
        }

        // Let the peer gossip task finish
        let (finished_name, _result) = gossip_tasks.select_next_some().await;
        if let Err(err) = _result {
            active_authority.set_failure_backoff(finished_name).await;
            active_authority.state.metrics.gossip_task_error_count.inc();
            error!("Peer {:?} returned error: {:?}", finished_name, err);
        } else {
            active_authority.set_success_backoff(finished_name).await;
            active_authority
                .state
                .metrics
                .gossip_task_success_count
                .inc();
            debug!("End gossip from peer {:?}", finished_name);
        }
        peer_names.remove(&finished_name);
    }
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
        PeerGossip {
            peer_name,
            client: active_authority.net.authority_clients[&peer_name].clone(),
            state: active_authority.state.clone(),
            max_seq: start_seq,
            aggregator: active_authority.net.clone(),
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
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Batch(_signed_batch)) )) => {},

                        // Upon receiving a transaction digest, store it if it is not processed already.
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digest))))) => {
                            if !self.state.database.effects_exists(&digest)? {
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
                    if !self.state.database.effects_exists(&digest)? {
                        // Download the certificate
                        let response = self.client.handle_transaction_info_request(TransactionInfoRequest::from(digest)).await?;
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
