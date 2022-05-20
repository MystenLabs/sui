// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::{stream::FuturesUnordered, StreamExt};
use std::{collections::HashSet, sync::Arc, time::Duration};
use sui_types::{
    base_types::AuthorityName,
    batch::{TxSequenceNumber, UpdateItem},
    error::SuiError,
    messages::{
        BatchInfoRequest, BatchInfoResponseItem, ConfirmationTransaction, TransactionInfoRequest,
    },
};

use crate::{
    authority::AuthorityState, authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI, safe_client::SafeClient,
};

use futures::stream::FuturesOrdered;
use tracing::{debug, error, info};

#[cfg(test)]
mod tests;

struct PeerGossip {
    peer_name: AuthorityName,
    client: SafeClient,
    state: Arc<AuthorityState>,
    max_seq: Option<TxSequenceNumber>,
    aggregator: Arc<AuthorityAggregator>,
}

const EACH_ITEM_DELAY_MS: u64 = 1_000;
const REQUEST_FOLLOW_NUM_DIGESTS: u64 = 100_000;
const REFRESH_FOLLOWER_PERIOD_SECS: u64 = 60;

use super::ActiveAuthority;

pub async fn gossip_process(active_authority: &ActiveAuthority, degree: usize) {
    // A copy of the committee
    let committee = &active_authority.net.committee;

    // Number of tasks at most "degree" and no more than committee - 1
    let target_num_tasks: usize = usize::min(
        active_authority.state.committee.voting_rights.len() - 1,
        degree,
    );

    // If we do not expect to connect to anyone
    if target_num_tasks == 0 {
        info!("Turn off gossip mechanism");
        return;
    }
    info!("Turn on gossip mechanism");

    // Keep track of names of active peers
    let mut peer_names = HashSet::new();
    let mut gossip_tasks = FuturesUnordered::new();

    // TODO: provide a clean way to get out of the loop.
    loop {
        debug!("Seek new peers");

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

        let mut k = 0;
        while gossip_tasks.len() < target_num_tasks {
            let name = active_authority.state.committee.sample();
            if peer_names.contains(name)
                || *name == active_authority.state.name
                || !active_authority.can_contact(*name).await
            {
                // Are we likely to never terminate because of this condition?
                // - We check we have nodes left by stake
                // - We check that we have at least 2/3 of nodes that can be contacted.
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
            peer_names.insert(*name);
            gossip_tasks.push(async move {
                let peer_gossip = PeerGossip::new(*name, active_authority);
                // Add more duration if we make more than 1 to ensure overlap
                debug!("Start gossip from peer {:?}", *name);
                peer_gossip
                    .spawn(Duration::from_secs(REFRESH_FOLLOWER_PERIOD_SECS + k * 15))
                    .await
            });
            k += 1;

            // If we have already used all the good stake, then stop here and
            // wait for some node to become available.
            let total_stake_used: usize = peer_names
                .iter()
                .map(|name| committee.weight(name))
                .sum::<usize>()
                + committee.weight(&active_authority.state.name);
            if committee.quorum_threshold() <= total_stake_used {
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
            error!("Peer {:?} returned error: {}", finished_name, err);
        } else {
            active_authority.set_success_backoff(finished_name).await;
            debug!("End gossip from peer {:?}", finished_name);
        }
        peer_names.remove(&finished_name);
    }
}

impl PeerGossip {
    pub fn new(peer_name: AuthorityName, active_authority: &ActiveAuthority) -> PeerGossip {
        PeerGossip {
            peer_name,
            client: active_authority.net.authority_clients[&peer_name].clone(),
            state: active_authority.state.clone(),
            max_seq: None,
            aggregator: active_authority.net.clone(),
        }
    }

    pub async fn spawn(mut self, duration: Duration) -> (AuthorityName, Result<(), SuiError>) {
        let peer_name = self.peer_name;
        let result = tokio::task::spawn(async move { self.gossip_timeout(duration).await }).await;

        // Return a join error.
        if result.is_err() {
            return (
                peer_name,
                Err(SuiError::GenericAuthorityError {
                    error: "Gossip Join Error".to_string(),
                }),
            );
        };

        // Return the internal result
        let result = result.unwrap();
        // minimum_time.await;
        (peer_name, result)
    }

    async fn gossip_timeout(&mut self, duration: Duration) -> Result<(), SuiError> {
        // Global timeout, we do not exceed this time in this task.
        let mut timeout = Box::pin(tokio::time::sleep(duration));
        let mut queue = FuturesOrdered::new();

        let req = BatchInfoRequest {
            start: self.max_seq,
            length: REQUEST_FOLLOW_NUM_DIGESTS,
        };

        // Get a client
        let mut streamx = Box::pin(self.client.handle_batch_stream(req).await?);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    // No matter what happens we do not spend too much time
                    // for any peer.

                    break },

                items = &mut streamx.next() => {
                    match items {
                        // Upon receiving a batch
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Batch(_signed_batch)) )) => {
                            // Update the longer term sequence_number only after a batch that is signed
                            self.max_seq = Some(_signed_batch.batch.next_sequence_number);
                        },
                        // Upon receiving a transaction digest we store it, if it is not processed already.
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Transaction((_seq, _digest))))) => {
                            if !self.state.database.effects_exists(&_digest)? {
                                queue.push(async move {
                                    tokio::time::sleep(Duration::from_millis(EACH_ITEM_DELAY_MS)).await;
                                    _digest
                                });

                            }

                        },
                        // When an error occurs we simply send back the error
                        Some(Err( err )) => {
                            return Err(err);
                        },
                        // The stream has closed, re-request:
                        None => {

                            let req = BatchInfoRequest {
                                start: self.max_seq,
                                length: REQUEST_FOLLOW_NUM_DIGESTS,
                            };

                            // Get a client
                            streamx = Box::pin(self.client.handle_batch_stream(req).await?);
                        },
                    }
                },

                digest = &mut queue.next() , if !queue.is_empty() => {
                    let digest = digest.unwrap();
                    if !self.state.database.effects_exists(&digest)? {
                        // We still do not have a transaction others have after some time

                        // Download the certificate
                        let response = self.client.handle_transaction_info_request(TransactionInfoRequest::from(digest)).await?;
                        if let Some(certificate) = response.certified_transaction {

                            // Process the certificate from one authority to ourselves
                            self.aggregator.sync_authority_source_to_destination(
                                ConfirmationTransaction { certificate },
                                self.peer_name,
                                self.state.name).await?;
                        }
                        else {
                            // The authority did not return the certificate, despite returning info
                            // But it should know the certificate!
                            return Err(SuiError::ByzantineAuthoritySuspicion { authority :  self.peer_name });
                        }
                    }
                },
            };
        }

        Ok(())
    }
}
