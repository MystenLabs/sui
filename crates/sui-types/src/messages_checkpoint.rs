// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::accumulator::Accumulator;
use crate::base_types::{ExecutionData, ExecutionDigests, VerifiedExecutionData};
use crate::committee::{EpochId, ProtocolVersion, StakeUnit};
use crate::crypto::{
    default_hash, AggregateAuthoritySignature, AuthoritySignInfo, AuthorityStrongQuorumSignInfo,
};
use crate::digests::Digest;
use crate::error::SuiResult;
use crate::gas::GasCostSummary;
use crate::message_envelope::{Envelope, Message, TrustedEnvelope, VerifiedEnvelope};
use crate::messages::TransactionEffectsAPI;
use crate::signature::GenericSignature;
use crate::storage::ReadStore;
use crate::sui_serde::AsProtocolVersion;
use crate::sui_serde::BigInt;
use crate::sui_serde::Readable;
use crate::{base_types::AuthorityName, committee::Committee, error::SuiError};
use anyhow::Result;
use fastcrypto::hash::MultisetHash;
use once_cell::sync::OnceCell;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use shared_crypto::intent::IntentScope;
use std::fmt::{Debug, Display, Formatter};
use std::slice::Iter;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use crate::digests::CheckpointContentsDigest;
pub use crate::digests::CheckpointDigest;

pub type CheckpointSequenceNumber = u64;
pub type CheckpointTimestamp = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointRequest {
    /// if a sequence number is specified, return the checkpoint with that sequence number;
    /// otherwise if None returns the latest authenticated checkpoint stored.
    pub sequence_number: Option<CheckpointSequenceNumber>,
    // A flag, if true also return the contents of the
    // checkpoint besides the meta-data.
    pub request_content: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointResponse {
    pub checkpoint: Option<CertifiedCheckpointSummary>,
    pub contents: Option<CheckpointContents>,
}

// The constituent parts of checkpoints, signed and certified

/// The Sha256 digest of an EllipticCurveMultisetHash committing to the live object set.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ECMHLiveObjectSetDigest {
    #[schemars(with = "[u8; 32]")]
    pub digest: Digest,
}

impl From<fastcrypto::hash::Digest<32>> for ECMHLiveObjectSetDigest {
    fn from(digest: fastcrypto::hash::Digest<32>) -> Self {
        Self {
            digest: Digest::new(digest.digest),
        }
    }
}

impl Default for ECMHLiveObjectSetDigest {
    fn default() -> Self {
        Accumulator::default().digest().into()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub enum CheckpointCommitment {
    ECMHLiveObjectSetDigest(ECMHLiveObjectSetDigest),
    // Other commitment types (e.g. merkle roots) go here.
}

impl From<ECMHLiveObjectSetDigest> for CheckpointCommitment {
    fn from(d: ECMHLiveObjectSetDigest) -> Self {
        Self::ECMHLiveObjectSetDigest(d)
    }
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EndOfEpochData {
    /// next_epoch_committee is `Some` if and only if the current checkpoint is
    /// the last checkpoint of an epoch.
    /// Therefore next_epoch_committee can be used to pick the last checkpoint of an epoch,
    /// which is often useful to get epoch level summary stats like total gas cost of an epoch,
    /// or the total number of transactions from genesis to the end of an epoch.
    /// The committee is stored as a vector of validator pub key and stake pairs. The vector
    /// should be sorted based on the Committee data structure.
    #[schemars(with = "Vec<(AuthorityName, BigInt<u64>)>")]
    #[serde_as(as = "Vec<(_, Readable<BigInt<u64>, _>)>")]
    pub next_epoch_committee: Vec<(AuthorityName, StakeUnit)>,

    /// The protocol version that is in effect during the epoch that starts immediately after this
    /// checkpoint.
    #[schemars(with = "AsProtocolVersion")]
    #[serde_as(as = "Readable<AsProtocolVersion, _>")]
    pub next_epoch_protocol_version: ProtocolVersion,

    /// Commitments to epoch specific state (e.g. live object set)
    pub epoch_commitments: Vec<CheckpointCommitment>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointSummary {
    pub epoch: EpochId,
    pub sequence_number: CheckpointSequenceNumber,
    /// Total number of transactions committed since genesis, including those in this
    /// checkpoint.
    pub network_total_transactions: u64,
    pub content_digest: CheckpointContentsDigest,
    pub previous_digest: Option<CheckpointDigest>,
    /// The running total gas costs of all transactions included in the current epoch so far
    /// until this checkpoint.
    pub epoch_rolling_gas_cost_summary: GasCostSummary,

    /// Timestamp of the checkpoint - number of milliseconds from the Unix epoch
    /// Checkpoint timestamps are monotonic, but not strongly monotonic - subsequent
    /// checkpoints can have same timestamp if they originate from the same underlining consensus commit
    pub timestamp_ms: CheckpointTimestamp,

    /// Commitments to checkpoint-specific state (e.g. txns in checkpoint, objects read/written in
    /// checkpoint).
    pub checkpoint_commitments: Vec<CheckpointCommitment>,

    /// Present only on the final checkpoint of the epoch.
    pub end_of_epoch_data: Option<EndOfEpochData>,

    /// CheckpointSummary is not an evolvable structure - it must be readable by any version of the
    /// code. Therefore, in order to allow extensions to be added to CheckpointSummary, we allow
    /// opaque data to be added to checkpoints which can be deserialized based on the current
    /// protocol version.
    pub version_specific_data: Vec<u8>,
}

impl Message for CheckpointSummary {
    type DigestType = CheckpointDigest;
    const SCOPE: IntentScope = IntentScope::CheckpointSummary;

    fn digest(&self) -> Self::DigestType {
        CheckpointDigest::new(default_hash(self))
    }

    fn verify(&self, sig_epoch: Option<EpochId>) -> SuiResult {
        // Signatures over CheckpointSummaries from other epochs are not valid.
        if let Some(sig_epoch) = sig_epoch {
            fp_ensure!(
                self.epoch == sig_epoch,
                SuiError::from("Epoch in the summary doesn't match with the signature")
            );
        }
        Ok(())
    }
}

impl CheckpointSummary {
    pub fn new(
        epoch: EpochId,
        sequence_number: CheckpointSequenceNumber,
        network_total_transactions: u64,
        transactions: &CheckpointContents,
        previous_digest: Option<CheckpointDigest>,
        epoch_rolling_gas_cost_summary: GasCostSummary,
        end_of_epoch_data: Option<EndOfEpochData>,
        timestamp_ms: CheckpointTimestamp,
    ) -> CheckpointSummary {
        let content_digest = *transactions.digest();

        Self {
            epoch,
            sequence_number,
            network_total_transactions,
            content_digest,
            previous_digest,
            epoch_rolling_gas_cost_summary,
            end_of_epoch_data,
            timestamp_ms,
            version_specific_data: Vec::new(),
            checkpoint_commitments: Default::default(),
        }
    }

    pub fn sequence_number(&self) -> &CheckpointSequenceNumber {
        &self.sequence_number
    }

    pub fn timestamp(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(self.timestamp_ms)
    }

    pub fn next_epoch_committee(&self) -> Option<&[(AuthorityName, StakeUnit)]> {
        self.end_of_epoch_data
            .as_ref()
            .map(|e| e.next_epoch_committee.as_slice())
    }
}

impl Display for CheckpointSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CheckpointSummary {{ epoch: {:?}, seq: {:?}, content_digest: {},
            epoch_rolling_gas_cost_summary: {:?}}}",
            self.epoch,
            self.sequence_number,
            self.content_digest,
            self.epoch_rolling_gas_cost_summary,
        )
    }
}

// Checkpoints are signed by an authority and 2f+1 form a
// certificate that others can use to catch up. The actual
// content of the digest must at the very least commit to
// the set of transactions contained in the certificate but
// we might extend this to contain roots of merkle trees,
// or other authenticated data structures to support light
// clients and more efficient sync protocols.

pub type CheckpointSummaryEnvelope<S> = Envelope<CheckpointSummary, S>;
pub type CertifiedCheckpointSummary = CheckpointSummaryEnvelope<AuthorityStrongQuorumSignInfo>;
pub type SignedCheckpointSummary = CheckpointSummaryEnvelope<AuthoritySignInfo>;

pub type VerifiedCheckpoint = VerifiedEnvelope<CheckpointSummary, AuthorityStrongQuorumSignInfo>;
pub type TrustedCheckpoint = TrustedEnvelope<CheckpointSummary, AuthorityStrongQuorumSignInfo>;

impl CertifiedCheckpointSummary {
    pub fn verify_with_contents(
        &self,
        committee: &Committee,
        contents: Option<&CheckpointContents>,
    ) -> SuiResult {
        self.verify_signature(committee)?;

        if let Some(contents) = contents {
            let content_digest = *contents.digest();
            fp_ensure!(
                content_digest == self.data().content_digest,
                SuiError::GenericAuthorityError{error:format!("Checkpoint contents digest mismatch: summary={:?}, received content digest {:?}, received {} transactions", self.data(), content_digest, contents.size())}
            );
        }

        Ok(())
    }
}

impl VerifiedCheckpoint {
    pub fn into_summary_and_sequence(self) -> (CheckpointSequenceNumber, CheckpointSummary) {
        let summary = self.into_inner().into_data();
        (summary.sequence_number, summary)
    }

    pub fn get_validator_signature(self) -> AggregateAuthoritySignature {
        self.auth_sig().signature.clone()
    }
}

/// This is a message validators publish to consensus in order to sign checkpoint
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointSignatureMessage {
    pub summary: SignedCheckpointSummary,
}

impl CheckpointSignatureMessage {
    pub fn verify(&self, committee: &Committee) -> SuiResult {
        self.summary.verify_signature(committee)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CheckpointContents {
    V1(CheckpointContentsV1),
}

/// CheckpointContents are the transactions included in an upcoming checkpoint.
/// They must have already been causally ordered. Since the causal order algorithm
/// is the same among validators, we expect all honest validators to come up with
/// the same order for each checkpoint content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CheckpointContentsV1 {
    #[serde(skip)]
    digest: OnceCell<CheckpointContentsDigest>,

    transactions: Vec<ExecutionDigests>,
    /// This field 'pins' user signatures for the checkpoint
    /// The length of this vector is same as length of transactions vector
    /// System transactions has empty signatures
    user_signatures: Vec<Vec<GenericSignature>>,
}

impl CheckpointContents {
    pub fn new_with_causally_ordered_transactions<T>(contents: T) -> Self
    where
        T: IntoIterator<Item = ExecutionDigests>,
    {
        let transactions: Vec<_> = contents.into_iter().collect();
        let user_signatures = transactions.iter().map(|_| vec![]).collect();
        Self::V1(CheckpointContentsV1 {
            digest: Default::default(),
            transactions,
            user_signatures,
        })
    }

    pub fn new_with_causally_ordered_transactions_and_signatures<T>(
        contents: T,
        user_signatures: Vec<Vec<GenericSignature>>,
    ) -> Self
    where
        T: IntoIterator<Item = ExecutionDigests>,
    {
        let transactions: Vec<_> = contents.into_iter().collect();
        assert_eq!(transactions.len(), user_signatures.len());
        Self::V1(CheckpointContentsV1 {
            digest: Default::default(),
            transactions,
            user_signatures,
        })
    }

    fn as_v1(&self) -> &CheckpointContentsV1 {
        match self {
            Self::V1(v) => v,
        }
    }

    fn into_v1(self) -> CheckpointContentsV1 {
        match self {
            Self::V1(v) => v,
        }
    }

    pub fn iter(&self) -> Iter<'_, ExecutionDigests> {
        self.as_v1().transactions.iter()
    }

    pub fn into_iter_with_signatures(
        self,
    ) -> impl Iterator<Item = (ExecutionDigests, Vec<GenericSignature>)> {
        let CheckpointContentsV1 {
            transactions,
            user_signatures,
            ..
        } = self.into_v1();

        transactions.into_iter().zip(user_signatures.into_iter())
    }

    /// Return an iterator that enumerates the transactions in the contents.
    /// The iterator item is a tuple of (sequence_number, &ExecutionDigests),
    /// where the sequence_number indicates the index of the transaction in the
    /// global ordering of executed transactions since genesis.
    pub fn enumerate_transactions(
        &self,
        ckpt: &CheckpointSummary,
    ) -> impl Iterator<Item = (u64, &ExecutionDigests)> {
        let start = ckpt.network_total_transactions - self.size() as u64;

        (0u64..)
            .zip(self.iter())
            .map(move |(i, digests)| (i + start, digests))
    }

    pub fn into_inner(self) -> Vec<ExecutionDigests> {
        self.into_v1().transactions
    }

    pub fn size(&self) -> usize {
        self.as_v1().transactions.len()
    }

    pub fn digest(&self) -> &CheckpointContentsDigest {
        self.as_v1()
            .digest
            .get_or_init(|| CheckpointContentsDigest::new(default_hash(self)))
    }
}

/// Same as CheckpointContents, but contains full contents of all Transactions and
/// TransactionEffects associated with the checkpoint.
// NOTE: This data structure is used for state sync of checkpoints. Therefore we attempt
// to estimate its size in CheckpointBuilder in order to limit the maximum serialized
// size of a checkpoint sent over the network. If this struct is modified,
// CheckpointBuilder::split_checkpoint_chunks should also be updated accordingly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullCheckpointContents {
    transactions: Vec<ExecutionData>,
    /// This field 'pins' user signatures for the checkpoint
    /// The length of this vector is same as length of transactions vector
    /// System transactions has empty signatures
    user_signatures: Vec<Vec<GenericSignature>>,
}

impl FullCheckpointContents {
    pub fn new_with_causally_ordered_transactions<T>(contents: T) -> Self
    where
        T: IntoIterator<Item = ExecutionData>,
    {
        let transactions: Vec<_> = contents.into_iter().collect();
        let user_signatures = transactions.iter().map(|_| vec![]).collect();
        Self {
            transactions,
            user_signatures,
        }
    }

    pub fn from_checkpoint_contents<S>(
        store: S,
        contents: CheckpointContents,
    ) -> Result<Option<Self>, <S as ReadStore>::Error>
    where
        S: ReadStore,
    {
        let mut transactions = Vec::with_capacity(contents.size());
        for tx in contents.iter() {
            if let (Some(t), Some(e)) = (
                store.get_transaction_block(&tx.transaction)?,
                store.get_transaction_effects(&tx.effects)?,
            ) {
                transactions.push(ExecutionData::new(t.into_inner(), e))
            } else {
                return Ok(None);
            }
        }
        Ok(Some(Self {
            transactions,
            user_signatures: contents.into_v1().user_signatures,
        }))
    }

    pub fn iter(&self) -> Iter<'_, ExecutionData> {
        self.transactions.iter()
    }

    /// Verifies that this checkpoint's digest matches the given digest, and that all internal
    /// Transaction and TransactionEffects digests are consistent.
    pub fn verify_digests(&self, digest: CheckpointContentsDigest) -> Result<()> {
        let self_digest = *self.checkpoint_contents().digest();
        fp_ensure!(
            digest == self_digest,
            anyhow::anyhow!(
                "checkpoint contents digest {self_digest} does not match expected digest {digest}"
            )
        );
        for tx in self.iter() {
            let transaction_digest = tx.transaction.digest();
            fp_ensure!(
                tx.effects.transaction_digest() == transaction_digest,
                anyhow::anyhow!(
                    "transaction digest {transaction_digest} does not match expected digest {}",
                    tx.effects.transaction_digest()
                )
            );
        }
        Ok(())
    }

    pub fn checkpoint_contents(&self) -> CheckpointContents {
        CheckpointContents::V1(CheckpointContentsV1 {
            digest: Default::default(),
            transactions: self.transactions.iter().map(|tx| tx.digests()).collect(),
            user_signatures: self.user_signatures.clone(),
        })
    }

    pub fn into_checkpoint_contents(self) -> CheckpointContents {
        CheckpointContents::V1(CheckpointContentsV1 {
            digest: Default::default(),
            transactions: self
                .transactions
                .into_iter()
                .map(|tx| tx.digests())
                .collect(),
            user_signatures: self.user_signatures,
        })
    }

    pub fn size(&self) -> usize {
        self.transactions.len()
    }
}

impl IntoIterator for FullCheckpointContents {
    type Item = ExecutionData;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.transactions.into_iter()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedCheckpointContents {
    transactions: Vec<VerifiedExecutionData>,
    /// This field 'pins' user signatures for the checkpoint
    /// The length of this vector is same as length of transactions vector
    /// System transactions has empty signatures
    user_signatures: Vec<Vec<GenericSignature>>,
}

impl VerifiedCheckpointContents {
    pub fn new_unchecked(contents: FullCheckpointContents) -> Self {
        Self {
            transactions: contents
                .transactions
                .into_iter()
                .map(VerifiedExecutionData::new_unchecked)
                .collect(),
            user_signatures: contents.user_signatures,
        }
    }

    pub fn iter(&self) -> Iter<'_, VerifiedExecutionData> {
        self.transactions.iter()
    }

    pub fn into_inner(self) -> FullCheckpointContents {
        FullCheckpointContents {
            transactions: self
                .transactions
                .into_iter()
                .map(|tx| tx.into_inner())
                .collect(),
            user_signatures: self.user_signatures,
        }
    }
}

#[cfg(test)]
mod tests {
    use fastcrypto::traits::KeyPair;
    use rand::prelude::StdRng;
    use rand::SeedableRng;

    use super::*;
    use crate::utils::make_committee_key;

    // TODO use the file name as a seed
    const RNG_SEED: [u8; 32] = [
        21, 23, 199, 200, 234, 250, 252, 178, 94, 15, 202, 178, 62, 186, 88, 137, 233, 192, 130,
        157, 179, 179, 65, 9, 31, 249, 221, 123, 225, 112, 199, 247,
    ];

    #[test]
    fn test_signed_checkpoint() {
        let mut rng = StdRng::from_seed(RNG_SEED);
        let (keys, committee) = make_committee_key(&mut rng);
        let (_, committee2) = make_committee_key(&mut rng);

        let set = CheckpointContents::new_with_causally_ordered_transactions(
            [ExecutionDigests::random()].into_iter(),
        );

        // TODO: duplicated in a test below.

        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public().into();

                SignedCheckpointSummary::new(
                    committee.epoch,
                    CheckpointSummary::new(
                        committee.epoch,
                        1,
                        0,
                        &set,
                        None,
                        GasCostSummary::default(),
                        None,
                        0,
                    ),
                    k,
                    name,
                )
            })
            .collect();

        signed_checkpoints
            .iter()
            .for_each(|c| c.verify_signature(&committee).expect("signature ok"));

        // fails when not signed by member of committee
        signed_checkpoints
            .iter()
            .for_each(|c| assert!(c.verify_signature(&committee2).is_err()));
    }

    #[test]
    fn test_certified_checkpoint() {
        let mut rng = StdRng::from_seed(RNG_SEED);
        let (keys, committee) = make_committee_key(&mut rng);

        let set = CheckpointContents::new_with_causally_ordered_transactions(
            [ExecutionDigests::random()].into_iter(),
        );

        let summary = CheckpointSummary::new(
            committee.epoch,
            1,
            0,
            &set,
            None,
            GasCostSummary::default(),
            None,
            0,
        );

        let sign_infos: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public().into();

                SignedCheckpointSummary::sign(committee.epoch, &summary, k, name)
            })
            .collect();

        let checkpoint_cert =
            CertifiedCheckpointSummary::new(summary, &sign_infos, &committee).expect("Cert is OK");

        // Signature is correct on proposal, and with same transactions
        assert!(checkpoint_cert
            .verify_with_contents(&committee, Some(&set))
            .is_ok());

        // Make a bad proposal
        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public().into();
                let set = CheckpointContents::new_with_causally_ordered_transactions(
                    [ExecutionDigests::random()].into_iter(),
                );

                SignedCheckpointSummary::new(
                    committee.epoch,
                    CheckpointSummary::new(
                        committee.epoch,
                        1,
                        0,
                        &set,
                        None,
                        GasCostSummary::default(),
                        None,
                        0,
                    ),
                    k,
                    name,
                )
            })
            .collect();

        let summary = signed_checkpoints[0].data().clone();
        let sign_infos: Vec<_> = signed_checkpoints
            .into_iter()
            .map(|v| v.into_sig())
            .collect();
        assert!(
            CertifiedCheckpointSummary::new(summary, &sign_infos, &committee)
                .unwrap()
                .verify_signature(&committee)
                .is_err()
        )
    }
}
