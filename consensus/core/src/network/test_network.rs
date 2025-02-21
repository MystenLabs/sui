// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::AuthorityIndex;
use futures::stream;
use parking_lot::Mutex;

use crate::{
    block::{BlockRef, VerifiedBlock},
    commit::{CommitRange, TrustedCommit},
    error::ConsensusResult,
    network::{BlockStream, NetworkService},
    Round,
};

use super::ExtendedSerializedBlock;

pub(crate) struct TestService {
    pub(crate) handle_send_block: Vec<(AuthorityIndex, ExtendedSerializedBlock)>,
    pub(crate) handle_fetch_blocks: Vec<(AuthorityIndex, Vec<BlockRef>)>,
    pub(crate) handle_subscribe_blocks: Vec<(AuthorityIndex, Round)>,
    pub(crate) handle_fetch_commits: Vec<(AuthorityIndex, CommitRange)>,
    pub(crate) own_blocks: Vec<ExtendedSerializedBlock>,
}

impl TestService {
    pub(crate) fn new() -> Self {
        Self {
            handle_send_block: Vec::new(),
            handle_fetch_blocks: Vec::new(),
            handle_subscribe_blocks: Vec::new(),
            handle_fetch_commits: Vec::new(),
            own_blocks: Vec::new(),
        }
    }

    #[cfg_attr(msim, allow(dead_code))]
    pub(crate) fn add_own_blocks(&mut self, blocks: Vec<ExtendedSerializedBlock>) {
        self.own_blocks.extend(blocks);
    }
}

#[async_trait]
impl NetworkService for Mutex<TestService> {
    async fn handle_send_block(
        &self,
        peer: AuthorityIndex,
        block: ExtendedSerializedBlock,
    ) -> ConsensusResult<()> {
        let mut state = self.lock();
        state.handle_send_block.push((peer, block));
        Ok(())
    }

    async fn handle_subscribe_blocks(
        &self,
        peer: AuthorityIndex,
        last_received: Round,
    ) -> ConsensusResult<BlockStream> {
        let mut state = self.lock();
        state.handle_subscribe_blocks.push((peer, last_received));
        let own_blocks = state
            .own_blocks
            .iter()
            // Let index in own_blocks be the round, and skip blocks <= last_received round.
            .skip(last_received as usize + 1)
            .cloned()
            .collect::<Vec<_>>();
        Ok(Box::pin(stream::iter(own_blocks)))
    }

    async fn handle_fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
        _highest_accepted_rounds: Vec<Round>,
    ) -> ConsensusResult<Vec<Bytes>> {
        self.lock().handle_fetch_blocks.push((peer, block_refs));
        Ok(vec![])
    }

    async fn handle_fetch_commits(
        &self,
        peer: AuthorityIndex,
        commit_range: CommitRange,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)> {
        self.lock().handle_fetch_commits.push((peer, commit_range));
        Ok((vec![], vec![]))
    }

    async fn handle_fetch_latest_blocks(
        &self,
        _peer: AuthorityIndex,
        _authorities: Vec<AuthorityIndex>,
    ) -> ConsensusResult<Vec<Bytes>> {
        unimplemented!("Unimplemented")
    }

    async fn handle_get_latest_rounds(
        &self,
        _peer: AuthorityIndex,
    ) -> ConsensusResult<(Vec<Round>, Vec<Round>)> {
        unimplemented!("Unimplemented")
    }
}
