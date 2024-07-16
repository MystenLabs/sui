// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::{date_time::DateTime, epoch::Epoch, uint53::UInt53};
use async_graphql::*;
use fastcrypto::encoding::{Base58, Encoding};
use sui_types::{
    digests::ConsensusCommitDigest,
    messages_checkpoint::CheckpointTimestamp,
    messages_consensus::{
        ConsensusCommitPrologue as NativeConsensusCommitPrologueTransactionV1,
        ConsensusCommitPrologueV2 as NativeConsensusCommitPrologueTransactionV2,
        ConsensusCommitPrologueV3 as NativeConsensusCommitPrologueTransactionV3,
        ConsensusDeterminedVersionAssignments,
    },
};

/// Other transaction kinds are usually represented by directly wrapping their native
/// representation. This kind has two native versions in the protocol, so the same cannot be done.
/// V2 has all the fields of V1 and one extra (consensus commit digest). The GraphQL representation
/// of this type is a struct containing all the common fields, as they are in the native type, and
/// an optional `consensus_commit_digest`.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ConsensusCommitPrologueTransaction {
    epoch: u64,
    round: u64,
    /// The sub DAG index of the consensus commit. This field will be populated if there
    /// are multiple consensus commits per round.
    sub_dag_index: Option<u64>,
    commit_timestamp_ms: CheckpointTimestamp,
    consensus_commit_digest: Option<ConsensusCommitDigest>,
    /// The checkpoint sequence number this was viewed at.
    checkpoint_viewed_at: u64,
    /// Stores consensus handler determined shared object version assignments for transactions
    /// within the consensus commit.
    /// Note that currently it only stores cancelled transactions' version assignments.
    consensus_determined_version_assignments: Option<ConsensusDeterminedVersionAssignments>,
}

/// System transaction that runs at the beginning of a checkpoint, and is responsible for setting
/// the current value of the clock, based on the timestamp from consensus.
#[Object]
impl ConsensusCommitPrologueTransaction {
    /// Epoch of the commit prologue transaction.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        Epoch::query(ctx, Some(self.epoch), self.checkpoint_viewed_at)
            .await
            .extend()
    }

    /// Consensus round of the commit.
    async fn round(&self) -> UInt53 {
        self.round.into()
    }

    /// Unix timestamp from consensus.
    async fn commit_timestamp(&self) -> Result<DateTime, Error> {
        Ok(DateTime::from_ms(self.commit_timestamp_ms as i64)?)
    }

    /// Digest of consensus output, encoded as a Base58 string (only available from V2 of the
    /// transaction).
    async fn consensus_commit_digest(&self) -> Option<String> {
        self.consensus_commit_digest
            .map(|digest| Base58::encode(digest.inner()))
    }
}

impl ConsensusCommitPrologueTransaction {
    pub(crate) fn from_v1(
        ccp: NativeConsensusCommitPrologueTransactionV1,
        checkpoint_viewed_at: u64,
    ) -> Self {
        Self {
            epoch: ccp.epoch,
            round: ccp.round,
            sub_dag_index: None,
            commit_timestamp_ms: ccp.commit_timestamp_ms,
            consensus_commit_digest: None,
            checkpoint_viewed_at,
            consensus_determined_version_assignments: None,
        }
    }

    pub(crate) fn from_v2(
        ccp: NativeConsensusCommitPrologueTransactionV2,
        checkpoint_viewed_at: u64,
    ) -> Self {
        Self {
            epoch: ccp.epoch,
            round: ccp.round,
            sub_dag_index: None,
            commit_timestamp_ms: ccp.commit_timestamp_ms,
            consensus_commit_digest: Some(ccp.consensus_commit_digest),
            checkpoint_viewed_at,
            consensus_determined_version_assignments: None,
        }
    }

    pub(crate) fn from_v3(
        ccp: NativeConsensusCommitPrologueTransactionV3,
        checkpoint_viewed_at: u64,
    ) -> Self {
        Self {
            epoch: ccp.epoch,
            round: ccp.round,
            sub_dag_index: ccp.sub_dag_index,
            commit_timestamp_ms: ccp.commit_timestamp_ms,
            consensus_commit_digest: Some(ccp.consensus_commit_digest),
            checkpoint_viewed_at,
            consensus_determined_version_assignments: Some(
                ccp.consensus_determined_version_assignments,
            ),
        }
    }
}
