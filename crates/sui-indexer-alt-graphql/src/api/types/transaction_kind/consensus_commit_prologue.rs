// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use fastcrypto::encoding::{Base58, Encoding};
use sui_types::{
    digests::{AdditionalConsensusStateDigest, ConsensusCommitDigest},
    messages_consensus::{
        ConsensusCommitPrologue as NativeConsensusCommitPrologueV1,
        ConsensusCommitPrologueV2 as NativeConsensusCommitPrologueV2,
        ConsensusCommitPrologueV3 as NativeConsensusCommitPrologueV3,
        ConsensusCommitPrologueV4 as NativeConsensusCommitPrologueV4,
    },
};

use crate::{
    api::{
        scalars::{date_time::DateTime, uint53::UInt53},
        types::epoch::Epoch,
    },
    error::RpcError,
    scope::Scope,
};

#[derive(Clone)]
pub(crate) struct ConsensusCommitPrologueTransaction {
    pub(crate) scope: Scope,
    pub(crate) epoch: u64,
    pub(crate) round: u64,
    pub(crate) sub_dag_index: Option<u64>,
    pub(crate) commit_timestamp_ms: u64,
    pub(crate) consensus_commit_digest: Option<ConsensusCommitDigest>,
    pub(crate) additional_state_digest: Option<AdditionalConsensusStateDigest>,
}

/// System transaction that runs at the beginning of a checkpoint, and is responsible for setting the current value of the clock, based on the timestamp from consensus.
#[Object]
impl ConsensusCommitPrologueTransaction {
    /// Epoch of the commit prologue transaction.
    ///
    /// Present in V1, V2, V3, V4.
    async fn epoch(&self) -> Option<Epoch> {
        Some(Epoch::with_id(self.scope.clone(), self.epoch))
    }

    /// Consensus round of the commit.
    ///
    /// Present in V1, V2, V3, V4.
    async fn round(&self) -> Option<UInt53> {
        Some(self.round.into())
    }

    /// Unix timestamp from consensus.
    ///
    /// Present in V1, V2, V3, V4.
    async fn commit_timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        Ok(Some(DateTime::from_ms(self.commit_timestamp_ms as i64)?))
    }

    /// Digest of consensus output, encoded as a Base58 string.
    ///
    /// Present in V2, V3, V4.
    async fn consensus_commit_digest(&self) -> Option<String> {
        self.consensus_commit_digest
            .as_ref()
            .map(|digest| Base58::encode(digest.inner()))
    }

    /// The sub DAG index of the consensus commit. This field is populated if there
    /// are multiple consensus commits per round.
    ///
    /// Present in V3, V4.
    async fn sub_dag_index(&self) -> Option<UInt53> {
        self.sub_dag_index.map(|idx| idx.into())
    }

    /// Digest of any additional state computed by the consensus handler.
    /// Used to detect forking bugs as early as possible.
    ///
    /// Present in V4.
    async fn additional_state_digest(&self) -> Option<String> {
        self.additional_state_digest
            .as_ref()
            .map(|digest| digest.to_string())
    }
}

impl ConsensusCommitPrologueTransaction {
    pub(crate) fn from_v1(native: NativeConsensusCommitPrologueV1, scope: Scope) -> Self {
        Self {
            scope,
            epoch: native.epoch,
            round: native.round,
            sub_dag_index: None,
            commit_timestamp_ms: native.commit_timestamp_ms,
            consensus_commit_digest: None,
            additional_state_digest: None,
        }
    }

    pub(crate) fn from_v2(native: NativeConsensusCommitPrologueV2, scope: Scope) -> Self {
        Self {
            scope,
            epoch: native.epoch,
            round: native.round,
            sub_dag_index: None,
            commit_timestamp_ms: native.commit_timestamp_ms,
            consensus_commit_digest: Some(native.consensus_commit_digest),
            additional_state_digest: None,
        }
    }

    pub(crate) fn from_v3(native: NativeConsensusCommitPrologueV3, scope: Scope) -> Self {
        Self {
            scope,
            epoch: native.epoch,
            round: native.round,
            sub_dag_index: native.sub_dag_index,
            commit_timestamp_ms: native.commit_timestamp_ms,
            consensus_commit_digest: Some(native.consensus_commit_digest),
            additional_state_digest: None,
        }
    }

    pub(crate) fn from_v4(native: NativeConsensusCommitPrologueV4, scope: Scope) -> Self {
        Self {
            scope,
            epoch: native.epoch,
            round: native.round,
            sub_dag_index: native.sub_dag_index,
            commit_timestamp_ms: native.commit_timestamp_ms,
            consensus_commit_digest: Some(native.consensus_commit_digest),
            additional_state_digest: Some(native.additional_state_digest),
        }
    }
}
