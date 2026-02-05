// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};

use super::{CommitSyncerClient, NetworkClient, ObserverNetworkClient, PeerId, SynchronizerClient};
use crate::{
    commit::CommitRange,
    error::{ConsensusError, ConsensusResult},
};

/// Unified client that can communicate with both validators and observers.
/// This acts as a facade over NetworkClient and ObserverNetworkClient, routing
/// requests to the appropriate underlying client based on the PeerId type.
pub(crate) struct UnifiedClient<V, O>
where
    V: NetworkClient,
    O: ObserverNetworkClient,
{
    validator_client: Option<Arc<V>>,
    observer_client: Option<Arc<O>>,
}

impl<V, O> UnifiedClient<V, O>
where
    V: NetworkClient,
    O: ObserverNetworkClient,
{
    /// Creates a new UnifiedClient for a validator node that can only talk to other validators.
    pub fn new_validator(client: Arc<V>) -> Self {
        Self {
            validator_client: Some(client),
            observer_client: None,
        }
    }

    /// Creates a new UnifiedClient for an observer node that can only talk to validators.
    pub fn new_observer(client: Arc<O>) -> Self {
        Self {
            validator_client: None,
            observer_client: Some(client),
        }
    }

    /// Creates a new UnifiedClient that can talk to both validators and observers.
    /// This is useful for observer nodes that need to communicate with both validator
    /// nodes and other observer nodes.
    pub fn new_hybrid(validator_client: Arc<V>, observer_client: Arc<O>) -> Self {
        Self {
            validator_client: Some(validator_client),
            observer_client: Some(observer_client),
        }
    }
}

#[async_trait]
impl<V, O> SynchronizerClient for UnifiedClient<V, O>
where
    V: NetworkClient,
    O: ObserverNetworkClient,
{
    async fn fetch_blocks(
        &self,
        peer: PeerId,
        block_refs: Vec<BlockRef>,
        highest_accepted_rounds: Vec<Round>,
        breadth_first: bool,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        match peer {
            PeerId::Authority(authority) => {
                let client =
                    self.validator_client
                        .as_ref()
                        .ok_or(ConsensusError::NetworkRequest(
                            "Validator client not available".to_string(),
                        ))?;
                client
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
                let client =
                    self.observer_client
                        .as_ref()
                        .ok_or(ConsensusError::NetworkRequest(
                            "Observer client not available".to_string(),
                        ))?;
                client.fetch_blocks(node_id, block_refs, timeout).await
            }
        }
    }

    async fn fetch_latest_blocks(
        &self,
        peer: PeerId,
        authorities: Vec<AuthorityIndex>,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        match peer {
            PeerId::Authority(authority) => {
                let client =
                    self.validator_client
                        .as_ref()
                        .ok_or(ConsensusError::NetworkRequest(
                            "Validator client not available".to_string(),
                        ))?;
                client
                    .fetch_latest_blocks(authority, authorities, timeout)
                    .await
            }
            PeerId::Observer(_) => Err(ConsensusError::NetworkRequest(
                "fetch_latest_blocks is only supported for validator peers".to_string(),
            )),
        }
    }

    async fn get_latest_rounds(
        &self,
        peer: PeerId,
        timeout: Duration,
    ) -> ConsensusResult<(Vec<Round>, Vec<Round>)> {
        match peer {
            PeerId::Authority(authority) => {
                let client =
                    self.validator_client
                        .as_ref()
                        .ok_or(ConsensusError::NetworkRequest(
                            "Validator client not available".to_string(),
                        ))?;
                client.get_latest_rounds(authority, timeout).await
            }
            PeerId::Observer(_) => Err(ConsensusError::NetworkRequest(
                "get_latest_rounds is only supported for validator peers".to_string(),
            )),
        }
    }
}

#[async_trait]
impl<V, O> CommitSyncerClient for UnifiedClient<V, O>
where
    V: NetworkClient,
    O: ObserverNetworkClient,
{
    async fn fetch_commits(
        &self,
        peer: PeerId,
        commit_range: CommitRange,
        timeout: Duration,
    ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
        match peer {
            PeerId::Authority(authority) => {
                let client =
                    self.validator_client
                        .as_ref()
                        .ok_or(ConsensusError::NetworkRequest(
                            "Validator client not available".to_string(),
                        ))?;
                client.fetch_commits(authority, commit_range, timeout).await
            }
            PeerId::Observer(node_id) => {
                let client =
                    self.observer_client
                        .as_ref()
                        .ok_or(ConsensusError::NetworkRequest(
                            "Observer client not available".to_string(),
                        ))?;
                client.fetch_commits(node_id, commit_range, timeout).await
            }
        }
    }
}
