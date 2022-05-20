// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::MAX_ITEMS_LIMIT;
use crate::{
    authority_aggregator::AuthorityAggregator, authority_client::AuthorityAPI,
    safe_client::SafeClient,
};
use futures::{
    channel::mpsc::{Receiver as MpscReceiver, Sender as MpscSender},
    SinkExt, StreamExt,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use sui_types::crypto::PublicKeyBytes;
use sui_types::{
    base_types::{AuthorityName, ObjectID, TransactionDigest},
    batch::UpdateItem,
    error::SuiError,
    messages::{
        BatchInfoRequest, BatchInfoResponseItem, ObjectInfoRequest, ObjectInfoRequestKind,
        TransactionInfoRequest, TransactionInfoResponse,
    },
    object::Object,
};

use tracing::{debug, error, info};

/// Follows one authority
struct Follower {
    // Authority being followed
    name: AuthorityName,

    client: SafeClient,

    state: Arc<FullNodeState>,

    downloader_channel: MpscSender<(AuthorityName, TransactionDigest)>,
}

use super::{FullNode, FullNodeState};

/// Spawns tasks to follow a quorum of minumum stake min_stake_target
pub async fn follow_multiple(
    full_node: &FullNode,
    min_stake_target: usize,
    downloader_channel: MpscSender<(AuthorityName, TransactionDigest)>,
) {
    let committee = &full_node.state.committee;
    if min_stake_target > committee.total_votes {
        error!("Stake target cannot be greater than total_votes")
    }
    info!("Follower coordinator starting. Target stake {min_stake_target}");

    let mut authority_names = HashSet::new();
    let mut num_tasks = 0;
    let mut stake = 0;
    while stake < min_stake_target {
        let name = full_node.state.committee.sample();
        if authority_names.contains(name) {
            continue;
        }

        authority_names.insert(*name);
        let d_channel = downloader_channel.clone();

        info!("Will follow authority {:?}", name);

        let follower = Follower::new(*name, full_node, d_channel);

        tokio::task::spawn(async move {
            follower.spawn().await;
        });

        num_tasks += 1;
        stake += committee.weight(name);
    }

    info!(
        "Spawned {num_tasks} follower tasks to achieve {stake} stake out of {} total",
        committee.total_votes
    );

    // drop orig channel
    drop(downloader_channel);
}

impl Follower {
    pub fn new(
        peer_name: AuthorityName,
        full_node: &FullNode,
        downloader_channel: MpscSender<(AuthorityName, TransactionDigest)>,
    ) -> Follower {
        Self {
            name: peer_name,
            client: full_node.aggregator.authority_clients[&peer_name].clone(),
            state: full_node.state.clone(),

            downloader_channel,
        }
    }

    pub async fn spawn(mut self) {
        let peer_name = self.name;
        info!("Spawn follower for {:?}", peer_name);

        let _ = tokio::task::spawn(async move { self.follow().await }).await;
    }

    async fn follow(&mut self) -> Result<(), SuiError> {
        let mut start = 0;
        let length = MAX_ITEMS_LIMIT;

        loop {
            info!("Follower listener started for {:?}", self.name);
            let mut batch_listen_chann = match self
                .client
                .handle_batch_stream(BatchInfoRequest {
                    start: Some(start),
                    length,
                })
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    error!(
                        "Follower listener error for authority: {:?}, err: {e}",
                        self.name
                    );
                    break;
                }
            };

            info!(
                "Follower batch listener for authority: {:?} starting at sequence: {:?}, for length: {}.",
                self.name, start, length
            );

            while let Some(item) = batch_listen_chann.next().await {
                match item {
                    Ok(BatchInfoResponseItem(UpdateItem::Transaction((tx_seq, tx_digest)))) => {
                        debug!(?tx_seq, digest=?tx_digest,
                            "Received single tx_seq {tx_seq}, digest {:?} from authority {:?}",
                            tx_digest, self.name
                        );
                        if !self.state.store.effects_exists(&tx_digest)? {
                            self.downloader_channel
                                .send((self.name, tx_digest))
                                .await.unwrap_or_else(|e| panic!("Unable to send tx {:?} to downloader, from authority {:?}, err {:?} ", tx_digest, self.name, e));
                        } else {
                            debug!("Ignoring previously seen tx tx_seq {tx_seq}, digest {:?} from authority {:?}", tx_digest, self.name);
                        }
                    }
                    Ok(BatchInfoResponseItem(UpdateItem::Batch(batch))) => {
                        // Strictly informational for now
                        debug!("Received batch {:?}", batch);

                        // TODO: This should be persisted to disk to avoid re-fetching
                        start = batch.batch.next_sequence_number;
                    }
                    Err(err) => {
                        error!("{:?}", err);
                        return Err(err);
                    }
                }
            }

            // If we ever get here, we should restart loop because stream closed
        }

        Ok(())
    }
}

/// Downloads certs, objects etc and updates state
#[derive(Clone)]
pub struct Downloader {
    pub aggregator: Arc<AuthorityAggregator>,
    pub state: Arc<FullNodeState>,
}

impl Downloader {
    //TODO: Downloader needs to be more robust to cover cases of missing dependencies, deleted objects and byzantine authorities
    pub async fn start_downloader(
        self,
        downloader_channel: MpscReceiver<(AuthorityName, TransactionDigest)>,
    ) {
        tokio::task::spawn(async move {
            match downloader_task(&self, downloader_channel).await {
                Ok(_) => todo!(),
                Err(e) => error!("Downloader task failed with err {e}"),
            };
        });
    }

    // TODO: update to download dependencies
    async fn update_state(
        &self,
        name: AuthorityName,
        tx_resp: TransactionInfoResponse,
    ) -> Result<(), SuiError> {
        let signed_effects = tx_resp
            .clone()
            .signed_effects
            .ok_or(SuiError::ByzantineAuthoritySuspicion { authority: name })?;
        let certificate = tx_resp
            .clone()
            .certified_transaction
            .ok_or(SuiError::ByzantineAuthoritySuspicion { authority: name })?;

        let seq_number = self
            .state
            .next_tx_seq_number
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        debug!(authority = ?name, digest = ?certificate.digest(), sequence = seq_number,
            "Updating state from authority {:?}, with tx resp {:?}",
            name, tx_resp
        );

        // Get all the objects which were changed
        let mutated_and_created_objects = signed_effects.effects.mutated_and_created();
        let mut modified_objects = HashMap::new();

        // TODO: make this parallel
        // Download the objects
        for r in mutated_and_created_objects {
            if let Some(obj) = self.download_latest_object(name, r.0 .0).await? {
                modified_objects.insert(obj.compute_object_reference(), obj);
            } else {
                // This can happen when the authority view of the object is changing faster than we can download
                // TODO: try asking other authorites in case one still has the object
                error!(
                    "Download for object {} failed. Object not present in authority",
                    r.0 .0
                );
            }
        }

        // Get the active inputs to the TX
        let mut active_input_objects = vec![];

        for obj_kind in certificate.data.input_objects()? {
            if let Some(obj) = self
                .download_latest_object(name, obj_kind.object_id())
                .await?
            {
                active_input_objects.push((obj_kind, obj));
            } else {
                // This can happen when the authority view of the object is changing faster than we can download
                // TODO: try asking other authorites in case one still has the object
                error!(
                    "Download for object {} failed. Object not present in authority",
                    obj_kind.object_id()
                );
            }
        }

        // TODO: is it safe to continue if some objects are missing?
        self.state
            .record_certificate(
                active_input_objects,
                modified_objects,
                certificate,
                signed_effects.effects.to_unsigned_effects(),
                seq_number,
            )
            .await?;
        Ok(())
    }

    // TODO: better error handling and dependencies fetching
    // TODO: There is a chance that the object has changed by the time we fetch it. Can we do better?
    pub async fn download_latest_object(
        &self,
        name: AuthorityName,
        obj: ObjectID,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self.aggregator.authority_clients[&name]
            .clone()
            .handle_object_info_request(ObjectInfoRequest {
                object_id: obj,
                request_kind: ObjectInfoRequestKind::LatestObjectInfo(None),
            })
            .await?
            .object_and_lock
            .map(|o| o.object))
    }
}

pub async fn downloader_task(
    d: &Downloader,
    mut recv: MpscReceiver<(PublicKeyBytes, TransactionDigest)>,
) -> Result<(), SuiError> {
    info!("Full node downlader started...");
    loop {
        while let Some((name, digest)) = recv.next().await {
            if !d.state.store.effects_exists(&digest)? {
                let client = d.aggregator.authority_clients[&name].clone();
                // Download the certificate
                let response = client
                    .handle_transaction_info_request(TransactionInfoRequest::from(digest))
                    .await?;
                if response.clone().certified_transaction.is_some() {
                    d.update_state(name, response).await?;
                } else {
                    let err: Result<(), SuiError> =
                        Err(SuiError::ByzantineAuthoritySuspicion { authority: name });
                    error!("Error when downloading {:?}", err);
                    return err;
                }
            } else {
                debug!(
                    "Downloader ignoring previously seen tx digest {:?} from authority {:?}",
                    digest, name
                );
            }
        }
    }
}
