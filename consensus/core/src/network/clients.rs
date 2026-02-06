// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};

use super::{ObserverNetworkClient, PeerId, ValidatorNetworkClient};
use crate::{commit::CommitRange, error::ConsensusResult};

/// Concrete client implementation for synchronizer operations.
/// Wraps validator and observer network clients and routes requests based on peer type.
pub(crate) struct SynchronizerClient<V, O>
where
    V: ValidatorNetworkClient,
    O: ObserverNetworkClient,
{
    validator_client: Arc<V>,
    observer_client: Arc<O>,
}

impl<V, O> SynchronizerClient<V, O>
where
    V: ValidatorNetworkClient,
    O: ObserverNetworkClient,
{
    pub fn new(validator_client: Arc<V>, observer_client: Arc<O>) -> Self {
        Self {
            validator_client,
            observer_client,
        }
    }

    pub async fn fetch_blocks(
        &self,
        peer: PeerId,
        block_refs: Vec<BlockRef>,
        highest_accepted_rounds: Vec<Round>,
        breadth_first: bool,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        match peer {
            PeerId::Authority(authority) => {
                self.validator_client
                    .fetch_blocks(
                        authority,
                        block_refs,
                        highest_accepted_rounds,
                        breadth_first,
                        timeout,
                    )
                    .await
            }
            PeerId::Observer(node_id) => {
                self.observer_client
                    .fetch_blocks(node_id, block_refs, timeout)
                    .await
            }
        }
    }

    pub async fn fetch_latest_blocks(
        &self,
        peer: AuthorityIndex,
        authorities: Vec<AuthorityIndex>,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        self.validator_client
            .fetch_latest_blocks(peer, authorities, timeout)
            .await
    }
}

/// Concrete client implementation for commit syncer operations.
/// Wraps validator and observer network clients and routes requests based on peer type.
pub(crate) struct CommitSyncerClient<V, O>
where
    V: ValidatorNetworkClient,
    O: ObserverNetworkClient,
{
    validator_client: Arc<V>,
    observer_client: Arc<O>,
}

impl<V, O> CommitSyncerClient<V, O>
where
    V: ValidatorNetworkClient,
    O: ObserverNetworkClient,
{
    pub fn new(validator_client: Arc<V>, observer_client: Arc<O>) -> Self {
        Self {
            validator_client,
            observer_client,
        }
    }

    pub async fn fetch_commits(
        &self,
        peer: PeerId,
        commit_range: CommitRange,
        timeout: Duration,
    ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
        match peer {
            PeerId::Authority(authority) => {
                self.validator_client
                    .fetch_commits(authority, commit_range, timeout)
                    .await
            }
            PeerId::Observer(node_id) => {
                self.observer_client
                    .fetch_commits(node_id, commit_range, timeout)
                    .await
            }
        }
    }

    pub async fn fetch_blocks(
        &self,
        peer: PeerId,
        block_refs: Vec<BlockRef>,
        highest_accepted_rounds: Vec<Round>,
        breadth_first: bool,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        match peer {
            PeerId::Authority(authority) => {
                self.validator_client
                    .fetch_blocks(
                        authority,
                        block_refs,
                        highest_accepted_rounds,
                        breadth_first,
                        timeout,
                    )
                    .await
            }
            PeerId::Observer(node_id) => {
                self.observer_client
                    .fetch_blocks(node_id, block_refs, timeout)
                    .await
            }
        }
    }
}
