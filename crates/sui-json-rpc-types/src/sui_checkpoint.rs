// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Page;
use fastcrypto::encoding::Base64;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::EpochId;
use sui_types::crypto::AggregateAuthoritySignature;
use sui_types::digests::CheckpointDigest;
use sui_types::gas::GasCostSummary;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::{
    CheckpointCommitment, CheckpointContents, CheckpointSequenceNumber, CheckpointSummary,
    CheckpointTimestamp, EndOfEpochData,
};
use sui_types::sui_serde::BigInt;
pub type CheckpointPage = Page<Checkpoint, BigInt<u64>>;

#[serde_as]
#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    /// Checkpoint's epoch ID
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "BigInt<u64>")]
    pub epoch: EpochId,
    /// Checkpoint sequence number
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "BigInt<u64>")]
    pub sequence_number: CheckpointSequenceNumber,
    /// Checkpoint digest
    pub digest: CheckpointDigest,
    /// Total number of transactions committed since genesis, including those in this
    /// checkpoint.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "BigInt<u64>")]
    pub network_total_transactions: u64,
    /// Digest of the previous checkpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_digest: Option<CheckpointDigest>,
    /// The running total gas costs of all transactions included in the current epoch so far
    /// until this checkpoint.
    pub epoch_rolling_gas_cost_summary: GasCostSummary,
    /// Timestamp of the checkpoint - number of milliseconds from the Unix epoch
    /// Checkpoint timestamps are monotonic, but not strongly monotonic - subsequent
    /// checkpoints can have same timestamp if they originate from the same underlining consensus commit
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "BigInt<u64>")]
    pub timestamp_ms: CheckpointTimestamp,
    /// Present only on the final checkpoint of the epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_of_epoch_data: Option<EndOfEpochData>,
    /// Transaction digests
    pub transactions: Vec<TransactionDigest>,

    /// Commitments to checkpoint state
    pub checkpoint_commitments: Vec<CheckpointCommitment>,
    /// Validator Signature
    #[schemars(with = "Base64")]
    //#[serde_as(as = "Readable<Base64, Bytes>")]
    pub validator_signature: AggregateAuthoritySignature,
}

impl
    From<(
        CheckpointSummary,
        CheckpointContents,
        AggregateAuthoritySignature,
    )> for Checkpoint
{
    fn from(
        (summary, contents, signature): (
            CheckpointSummary,
            CheckpointContents,
            AggregateAuthoritySignature,
        ),
    ) -> Self {
        let digest = summary.digest();
        let CheckpointSummary {
            epoch,
            sequence_number,
            network_total_transactions,
            previous_digest,
            epoch_rolling_gas_cost_summary,
            timestamp_ms,
            end_of_epoch_data,
            ..
        } = summary;

        Checkpoint {
            epoch,
            sequence_number,
            digest,
            network_total_transactions,
            previous_digest,
            epoch_rolling_gas_cost_summary,
            timestamp_ms,
            end_of_epoch_data,
            transactions: contents.iter().map(|digest| digest.transaction).collect(),
            // TODO: populate commitment for rpc clients. Most likely, rpc clients don't need this
            // info (if they need it, they need to get signed BCS data anyway in order to trust
            // it).
            checkpoint_commitments: Default::default(),
            validator_signature: signature,
        }
    }
}

#[serde_as]
#[derive(Clone, Copy, Debug, JsonSchema, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CheckpointId {
    SequenceNumber(
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "BigInt<u64>")]
        CheckpointSequenceNumber,
    ),
    Digest(CheckpointDigest),
}

impl From<CheckpointSequenceNumber> for CheckpointId {
    fn from(seq: CheckpointSequenceNumber) -> Self {
        Self::SequenceNumber(seq)
    }
}

impl From<CheckpointDigest> for CheckpointId {
    fn from(digest: CheckpointDigest) -> Self {
        Self::Digest(digest)
    }
}
