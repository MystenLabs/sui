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
use tracing::{error, info};

#[cfg(test)]
mod tests;

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
    // Number of tasks at most "degree" and no more than committee - 1
    let target_num_tasks: usize = usize::min(
        active_authority.state.committee.voting_rights.len() - 1,
        degree,
    );

    // Keep track of names of active peers
    let mut peer_names = HashSet::new();
    let mut gossip_tasks = FuturesUnordered::new();

    // TODO: provide a clean way to get out of the loop.
    loop {
        let mut k = 0;
        while gossip_tasks.len() < target_num_tasks {
            let name = active_authority.state.committee.sample();
            if peer_names.contains(name) || *name == active_authority.state.name {
                continue;
            }
            peer_names.insert(*name);
            gossip_tasks.push(async move {
                let peer_gossip = PeerGossip::new(*name, active_authority);
                // Add more duration if we make more than 1 to ensure overlap
                info!("Gossip: Start gossip from peer {:?}", *name);
                peer_gossip
                    .spawn(Duration::from_secs(REFRESH_FOLLOWER_PERIOD_SECS + k * 15))
                    .await
            });
            k += 1;
        }

        // Let the peer gossip task finish
        debug_assert!(!gossip_tasks.is_empty());
        let (finished_name, _result) = gossip_tasks.select_next_some().await;
        if let Err(err) = _result {
            error!(
                "Gossip: Peer {:?} finished with error: {}",
                finished_name, err
            );
        } else {
            info!("Gossip: End gossip from peer {:?}", finished_name);
        }
        peer_names.remove(&finished_name);
    }
}

impl<A> PeerGossip<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(peer_name: AuthorityName, active_authority: &ActiveAuthority<A>) -> PeerGossip<A> {
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
        let result = tokio::task::spawn(async move { self.gossip_timeout(duration).await })
            .await
            .map(|_| ())
            .map_err(|_err| SuiError::GenericAuthorityError {
                error: "Gossip Join Error".to_string(),
            });

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
                            // Update the longer term seqeunce_number only after a batch that is signed
                            self.max_seq = Some(_signed_batch.batch.next_sequence_number);
                        },
                        // Upon receiving a trasnaction digest we store it, if it is not processed already.
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Transaction((_seq, _digest))))) => {
                            if !self.state._database.effects_exists(&_digest)? {
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
                    if !self.state._database.effects_exists(&digest)? {
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
