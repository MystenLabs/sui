// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};

use super::{
    ObserverNetworkClient, PeerId, ValidatorNetworkClient, observer::TonicObserverClient,
    tonic_network::TonicValidatorClient,
};
use crate::{
    commit::CommitRange,
    context::Context,
    error::{ConsensusError, ConsensusResult},
};

/// Concrete client implementation for synchronizer operations.
/// Wraps validator and observer network clients and routes requests based on whether this node is
/// a validator and the peer is an authority.
pub(crate) struct SynchronizerClient<
    V: ValidatorNetworkClient = TonicValidatorClient,
    O: ObserverNetworkClient = TonicObserverClient,
> {
    context: Arc<Context>,
    validator_client: Option<Arc<V>>,
    observer_client: Option<Arc<O>>,
}

impl<V, O> SynchronizerClient<V, O>
where
    V: ValidatorNetworkClient,
    O: ObserverNetworkClient,
{
    pub fn new(
        context: Arc<Context>,
        validator_client: Option<Arc<V>>,
        observer_client: Option<Arc<O>>,
    ) -> Self {
        Self {
            context,
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
        // A validator node will always talk via the validator interface to another authority.
        // Otherwise, the only way to communicate with the other peer is via the observer interface.
        if self.context.is_validator()
            && let PeerId::Validator(authority) = peer
        {
            let client = self.validator_client.as_ref().ok_or_else(|| {
                ConsensusError::NetworkConfig("Validator client not available".to_string())
            })?;
            client
                .fetch_blocks(
                    authority,
                    block_refs,
                    highest_accepted_rounds,
                    breadth_first,
                    timeout,
                )
                .await
        } else {
            let client = self.observer_client.as_ref().ok_or_else(|| {
                ConsensusError::NetworkConfig("Observer client not available".to_string())
            })?;
            client.fetch_blocks(peer, block_refs, timeout).await
        }
    }

    pub async fn fetch_latest_blocks(
        &self,
        peer: AuthorityIndex,
        authorities: Vec<AuthorityIndex>,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        // fetch_latest_blocks is a validator-only operation: observers do not have a direct
        // counterpart for this RPC, so it unconditionally requires the validator client.
        let client = self.validator_client.as_ref().ok_or_else(|| {
            ConsensusError::NetworkConfig("Validator client not available".to_string())
        })?;
        client.fetch_latest_blocks(peer, authorities, timeout).await
    }
}

/// Concrete client implementation for commit syncer operations.
/// Wraps validator and observer network clients and routes requests based on whether this node is
/// a validator and the peer is an authority.
pub(crate) struct CommitSyncerClient<
    V: ValidatorNetworkClient = TonicValidatorClient,
    O: ObserverNetworkClient = TonicObserverClient,
> {
    context: Arc<Context>,
    validator_client: Option<Arc<V>>,
    observer_client: Option<Arc<O>>,
}

impl<V, O> CommitSyncerClient<V, O>
where
    V: ValidatorNetworkClient,
    O: ObserverNetworkClient,
{
    pub fn new(
        context: Arc<Context>,
        validator_client: Option<Arc<V>>,
        observer_client: Option<Arc<O>>,
    ) -> Self {
        Self {
            context,
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
        // A validator node will always talk via the validator interface to another authority.
        // Otherwise, the only way to communicate with the other peer is via the observer interface.
        if self.context.is_validator()
            && let PeerId::Validator(authority) = peer
        {
            let client = self.validator_client.as_ref().ok_or_else(|| {
                ConsensusError::NetworkConfig("Validator client not available".to_string())
            })?;
            client.fetch_commits(authority, commit_range, timeout).await
        } else {
            let client = self.observer_client.as_ref().ok_or_else(|| {
                ConsensusError::NetworkConfig("Observer client not available".to_string())
            })?;
            client.fetch_commits(peer, commit_range, timeout).await
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
        // A validator node will always talk via the validator interface to another authority.
        // Otherwise, the only way to communicate with the other peer is via the observer interface.
        if self.context.is_validator()
            && let PeerId::Validator(authority) = peer
        {
            let client = self.validator_client.as_ref().ok_or_else(|| {
                ConsensusError::NetworkConfig("Validator client not available".to_string())
            })?;
            client
                .fetch_blocks(
                    authority,
                    block_refs,
                    highest_accepted_rounds,
                    breadth_first,
                    timeout,
                )
                .await
        } else {
            let client = self.observer_client.as_ref().ok_or_else(|| {
                ConsensusError::NetworkConfig("Observer client not available".to_string())
            })?;
            client.fetch_blocks(peer, block_refs, timeout).await
        }
    }
}
