// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::accumulator::Accumulator;
use crate::base_types::{
    random_object_ref, ExecutionData, ExecutionDigests, VerifiedExecutionData,
};
use crate::committee::{EpochId, ProtocolVersion, StakeUnit};
use crate::crypto::{
    default_hash, get_key_pair, AccountKeyPair, AggregateAuthoritySignature, AuthoritySignInfo,
    AuthoritySignInfoTrait, AuthorityStrongQuorumSignInfo, RandomnessRound,
};
use crate::digests::Digest;
use crate::effects::{TestEffectsBuilder, TransactionEffectsAPI};
use crate::error::SuiResult;
use crate::gas::GasCostSummary;
use crate::message_envelope::{Envelope, Message, TrustedEnvelope, VerifiedEnvelope};
use crate::signature::GenericSignature;
use crate::storage::ReadStore;
use crate::sui_serde::AsProtocolVersion;
use crate::sui_serde::BigInt;
use crate::sui_serde::Readable;
use crate::transaction::{Transaction, TransactionData};
use crate::{base_types::AuthorityName, committee::Committee, error::SuiError};
use anyhow::Result;
use fastcrypto::hash::MultisetHash;
use mysten_metrics::histogram::Histogram as MystenHistogram;
use once_cell::sync::OnceCell;
use prometheus::Histogram;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use shared_crypto::intent::{Intent, IntentScope};
use std::fmt::{Debug, Display, Formatter};
use std::slice::Iter;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sui_protocol_config::ProtocolConfig;
use tap::TapFallible;
use tracing::warn;

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointRequestV2 {
    /// if a sequence number is specified, return the checkpoint with that sequence number;
    /// otherwise if None returns the latest checkpoint stored (authenticated or pending,
    /// depending on the value of `certified` flag)
    pub sequence_number: Option<CheckpointSequenceNumber>,
    // A flag, if true also return the contents of the
    // checkpoint besides the meta-data.
    pub request_content: bool,
    // If true, returns certified checkpoint, otherwise returns pending checkpoint
    pub certified: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointSummaryResponse {
    Certified(CertifiedCheckpointSummary),
    Pending(CheckpointSummary),
}

impl CheckpointSummaryResponse {
    pub fn content_digest(&self) -> CheckpointContentsDigest {
        match self {
            Self::Certified(s) => s.content_digest,
            Self::Pending(s) => s.content_digest,
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointResponse {
    pub checkpoint: Option<CertifiedCheckpointSummary>,
    pub contents: Option<CheckpointContents>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointResponseV2 {
    pub checkpoint: Option<CheckpointSummaryResponse>,
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
    ///
    /// This is implemented with BCS-serialized `CheckpointVersionSpecificData`.
    pub version_specific_data: Vec<u8>,
}

impl Message for CheckpointSummary {
    type DigestType = CheckpointDigest;
    const SCOPE: IntentScope = IntentScope::CheckpointSummary;

    fn digest(&self) -> Self::DigestType {
        CheckpointDigest::new(default_hash(self))
    }
}

impl CheckpointSummary {
    pub fn new(
        protocol_config: &ProtocolConfig,
        epoch: EpochId,
        sequence_number: CheckpointSequenceNumber,
        network_total_transactions: u64,
        transactions: &CheckpointContents,
        previous_digest: Option<CheckpointDigest>,
        epoch_rolling_gas_cost_summary: GasCostSummary,
        end_of_epoch_data: Option<EndOfEpochData>,
        timestamp_ms: CheckpointTimestamp,
        randomness_rounds: Vec<RandomnessRound>,
    ) -> CheckpointSummary {
        let content_digest = *transactions.digest();

        let version_specific_data = match protocol_config
            .checkpoint_summary_version_specific_data_as_option()
        {
            None | Some(0) => Vec::new(),
            Some(1) => bcs::to_bytes(&CheckpointVersionSpecificData::V1(
                CheckpointVersionSpecificDataV1 { randomness_rounds },
            ))
            .expect("version specific data should serialize"),
            _ => unimplemented!("unrecognized version_specific_data version for CheckpointSummary"),
        };

        Self {
            epoch,
            sequence_number,
            network_total_transactions,
            content_digest,
            previous_digest,
            epoch_rolling_gas_cost_summary,
            end_of_epoch_data,
            timestamp_ms,
            version_specific_data,
            checkpoint_commitments: Default::default(),
        }
    }

    pub fn verify_epoch(&self, epoch: EpochId) -> SuiResult {
        fp_ensure!(
            self.epoch == epoch,
            SuiError::WrongEpoch {
                expected_epoch: epoch,
                actual_epoch: self.epoch,
            }
        );
        Ok(())
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

    pub fn report_checkpoint_age(&self, metrics: &Histogram, metrics_deprecated: &MystenHistogram) {
        SystemTime::now()
            .duration_since(self.timestamp())
            .map(|latency| {
                metrics.observe(latency.as_secs_f64());
                metrics_deprecated.report(latency.as_millis() as u64);
            })
            .tap_err(|err| {
                warn!(
                    checkpoint_seq = self.sequence_number,
                    "unable to compute checkpoint age: {}", err
                )
            })
            .ok();
    }

    pub fn is_last_checkpoint_of_epoch(&self) -> bool {
        self.end_of_epoch_data.is_some()
    }

    pub fn version_specific_data(
        &self,
        config: &ProtocolConfig,
    ) -> Result<Option<CheckpointVersionSpecificData>> {
        match config.checkpoint_summary_version_specific_data_as_option() {
            None | Some(0) => Ok(None),
            Some(1) => Ok(Some(bcs::from_bytes(&self.version_specific_data)?)),
            _ => unimplemented!("unrecognized version_specific_data version in CheckpointSummary"),
        }
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
    pub fn verify_authority_signatures(&self, committee: &Committee) -> SuiResult {
        self.data().verify_epoch(self.auth_sig().epoch)?;
        self.auth_sig().verify_secure(
            self.data(),
            Intent::sui_app(IntentScope::CheckpointSummary),
            committee,
        )
    }

    pub fn try_into_verified(self, committee: &Committee) -> SuiResult<VerifiedCheckpoint> {
        self.verify_authority_signatures(committee)?;
        Ok(VerifiedCheckpoint::new_from_verified(self))
    }

    pub fn verify_with_contents(
        &self,
        committee: &Committee,
        contents: Option<&CheckpointContents>,
    ) -> SuiResult {
        self.verify_authority_signatures(committee)?;

        if let Some(contents) = contents {
            let content_digest = *contents.digest();
            fp_ensure!(
                content_digest == self.data().content_digest,
                SuiError::GenericAuthorityError{error:format!("Checkpoint contents digest mismatch: summary={:?}, received content digest {:?}, received {} transactions", self.data(), content_digest, contents.size())}
            );
        }

        Ok(())
    }

    pub fn into_summary_and_sequence(self) -> (CheckpointSequenceNumber, CheckpointSummary) {
        let summary = self.into_data();
        (summary.sequence_number, summary)
    }

    pub fn get_validator_signature(self) -> AggregateAuthoritySignature {
        self.auth_sig().signature.clone()
    }
}

impl SignedCheckpointSummary {
    pub fn verify_authority_signatures(&self, committee: &Committee) -> SuiResult {
        self.data().verify_epoch(self.auth_sig().epoch)?;
        self.auth_sig().verify_secure(
            self.data(),
            Intent::sui_app(IntentScope::CheckpointSummary),
            committee,
        )
    }

    pub fn try_into_verified(
        self,
        committee: &Committee,
    ) -> SuiResult<VerifiedEnvelope<CheckpointSummary, AuthoritySignInfo>> {
        self.verify_authority_signatures(committee)?;
        Ok(VerifiedEnvelope::<CheckpointSummary, AuthoritySignInfo>::new_from_verified(self))
    }
}

impl VerifiedCheckpoint {
    pub fn into_summary_and_sequence(self) -> (CheckpointSequenceNumber, CheckpointSummary) {
        self.into_inner().into_summary_and_sequence()
    }
}

/// This is a message validators publish to consensus in order to sign checkpoint
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointSignatureMessage {
    pub summary: SignedCheckpointSummary,
}

impl CheckpointSignatureMessage {
    pub fn verify(&self, committee: &Committee) -> SuiResult {
        self.summary.verify_authority_signatures(committee)
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
    pub fn new_with_digests_and_signatures<T>(
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

    pub fn new_with_causally_ordered_execution_data<'a, T>(contents: T) -> Self
    where
        T: IntoIterator<Item = &'a VerifiedExecutionData>,
    {
        let (transactions, user_signatures): (Vec<_>, Vec<_>) = contents
            .into_iter()
            .map(|data| {
                (
                    data.digests(),
                    data.transaction.inner().data().tx_signatures().to_owned(),
                )
            })
            .unzip();
        assert_eq!(transactions.len(), user_signatures.len());
        Self::V1(CheckpointContentsV1 {
            digest: Default::default(),
            transactions,
            user_signatures,
        })
    }

    pub fn new_with_digests_only_for_tests<T>(contents: T) -> Self
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

        transactions.into_iter().zip(user_signatures)
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

    pub fn inner(&self) -> &[ExecutionDigests] {
        &self.as_v1().transactions
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
        let (transactions, user_signatures): (Vec<_>, Vec<_>) = contents
            .into_iter()
            .map(|data| {
                let sig = data.transaction.data().tx_signatures().to_owned();
                (data, sig)
            })
            .unzip();
        assert_eq!(transactions.len(), user_signatures.len());
        Self {
            transactions,
            user_signatures,
        }
    }
    pub fn from_contents_and_execution_data(
        contents: CheckpointContents,
        execution_data: impl Iterator<Item = ExecutionData>,
    ) -> Self {
        let transactions: Vec<_> = execution_data.collect();
        Self {
            transactions,
            user_signatures: contents.into_v1().user_signatures,
        }
    }
    pub fn from_checkpoint_contents<S>(store: S, contents: CheckpointContents) -> Option<Self>
    where
        S: ReadStore,
    {
        let mut transactions = Vec::with_capacity(contents.size());
        for tx in contents.iter() {
            if let (Some(t), Some(e)) = (
                store.get_transaction(&tx.transaction),
                store.get_transaction_effects(&tx.transaction),
            ) {
                transactions.push(ExecutionData::new((*t).clone().into_inner(), e))
            } else {
                return None;
            }
        }
        Some(Self {
            transactions,
            user_signatures: contents.into_v1().user_signatures,
        })
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

    pub fn random_for_testing() -> Self {
        let (a, key): (_, AccountKeyPair) = get_key_pair();
        let transaction = Transaction::from_data_and_signer(
            TransactionData::new_transfer(
                a,
                random_object_ref(),
                a,
                random_object_ref(),
                100000000000,
                100,
            ),
            vec![&key],
        );
        let effects = TestEffectsBuilder::new(transaction.data()).build();
        let exe_data = ExecutionData {
            transaction,
            effects,
        };
        FullCheckpointContents::new_with_causally_ordered_transactions(vec![exe_data])
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

    pub fn transactions(&self) -> &[VerifiedExecutionData] {
        &self.transactions
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

    pub fn into_checkpoint_contents(self) -> CheckpointContents {
        self.into_inner().into_checkpoint_contents()
    }

    pub fn into_checkpoint_contents_digest(self) -> CheckpointContentsDigest {
        *self.into_inner().into_checkpoint_contents().digest()
    }

    pub fn num_of_transactions(&self) -> usize {
        self.transactions.len()
    }
}

/// Holds data in CheckpointSummary that is serialized into the `version_specific_data` field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointVersionSpecificData {
    V1(CheckpointVersionSpecificDataV1),
}

impl CheckpointVersionSpecificData {
    pub fn as_v1(&self) -> &CheckpointVersionSpecificDataV1 {
        match self {
            Self::V1(v) => v,
        }
    }

    pub fn into_v1(self) -> CheckpointVersionSpecificDataV1 {
        match self {
            Self::V1(v) => v,
        }
    }

    pub fn empty_for_tests() -> CheckpointVersionSpecificData {
        CheckpointVersionSpecificData::V1(CheckpointVersionSpecificDataV1 {
            randomness_rounds: Vec::new(),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointVersionSpecificDataV1 {
    /// Lists the rounds for which RandomnessStateUpdate transactions are present in the checkpoint.
    pub randomness_rounds: Vec<RandomnessRound>,
}

#[cfg(test)]
mod tests {
    use crate::digests::{ConsensusCommitDigest, TransactionDigest, TransactionEffectsDigest};
    use crate::messages_consensus::ConsensusDeterminedVersionAssignments;
    use crate::transaction::VerifiedTransaction;
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

        let set = CheckpointContents::new_with_digests_only_for_tests([ExecutionDigests::random()]);

        // TODO: duplicated in a test below.

        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public().into();

                SignedCheckpointSummary::new(
                    committee.epoch,
                    CheckpointSummary::new(
                        &ProtocolConfig::get_for_max_version_UNSAFE(),
                        committee.epoch,
                        1,
                        0,
                        &set,
                        None,
                        GasCostSummary::default(),
                        None,
                        0,
                        Vec::new(),
                    ),
                    k,
                    name,
                )
            })
            .collect();

        signed_checkpoints.iter().for_each(|c| {
            c.verify_authority_signatures(&committee)
                .expect("signature ok")
        });

        // fails when not signed by member of committee
        signed_checkpoints
            .iter()
            .for_each(|c| assert!(c.verify_authority_signatures(&committee2).is_err()));
    }

    #[test]
    fn test_certified_checkpoint() {
        let mut rng = StdRng::from_seed(RNG_SEED);
        let (keys, committee) = make_committee_key(&mut rng);

        let set = CheckpointContents::new_with_digests_only_for_tests([ExecutionDigests::random()]);

        let summary = CheckpointSummary::new(
            &ProtocolConfig::get_for_max_version_UNSAFE(),
            committee.epoch,
            1,
            0,
            &set,
            None,
            GasCostSummary::default(),
            None,
            0,
            Vec::new(),
        );

        let sign_infos: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public().into();

                SignedCheckpointSummary::sign(committee.epoch, &summary, k, name)
            })
            .collect();

        let checkpoint_cert =
            CertifiedCheckpointSummary::new(summary, sign_infos, &committee).expect("Cert is OK");

        // Signature is correct on proposal, and with same transactions
        assert!(checkpoint_cert
            .verify_with_contents(&committee, Some(&set))
            .is_ok());

        // Make a bad proposal
        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public().into();
                let set = CheckpointContents::new_with_digests_only_for_tests([
                    ExecutionDigests::random(),
                ]);

                SignedCheckpointSummary::new(
                    committee.epoch,
                    CheckpointSummary::new(
                        &ProtocolConfig::get_for_max_version_UNSAFE(),
                        committee.epoch,
                        1,
                        0,
                        &set,
                        None,
                        GasCostSummary::default(),
                        None,
                        0,
                        Vec::new(),
                    ),
                    k,
                    name,
                )
            })
            .collect();

        let summary = signed_checkpoints[0].data().clone();
        let sign_infos = signed_checkpoints
            .into_iter()
            .map(|v| v.into_sig())
            .collect();
        assert!(
            CertifiedCheckpointSummary::new(summary, sign_infos, &committee)
                .unwrap()
                .verify_authority_signatures(&committee)
                .is_err()
        )
    }

    // Generate a CheckpointSummary from the input transaction digest. All the other fields in the generated
    // CheckpointSummary will be the same. The generated CheckpointSummary can be used to test how input
    // transaction digest affects CheckpointSummary.
    fn generate_test_checkpoint_summary_from_digest(
        digest: TransactionDigest,
    ) -> CheckpointSummary {
        CheckpointSummary::new(
            &ProtocolConfig::get_for_max_version_UNSAFE(),
            1,
            2,
            10,
            &CheckpointContents::new_with_digests_only_for_tests([ExecutionDigests::new(
                digest,
                TransactionEffectsDigest::ZERO,
            )]),
            None,
            GasCostSummary::default(),
            None,
            100,
            Vec::new(),
        )
    }

    // Tests that ConsensusCommitPrologue with different consensus commit digest will result in different checkpoint content.
    #[test]
    fn test_checkpoint_summary_with_different_consensus_digest() {
        // First, tests that same consensus commit digest will produce the same checkpoint content.
        {
            let t1 = VerifiedTransaction::new_consensus_commit_prologue_v3(
                1,
                2,
                100,
                ConsensusCommitDigest::default(),
                ConsensusDeterminedVersionAssignments::empty_for_testing(),
            );
            let t2 = VerifiedTransaction::new_consensus_commit_prologue_v3(
                1,
                2,
                100,
                ConsensusCommitDigest::default(),
                ConsensusDeterminedVersionAssignments::empty_for_testing(),
            );
            let c1 = generate_test_checkpoint_summary_from_digest(*t1.digest());
            let c2 = generate_test_checkpoint_summary_from_digest(*t2.digest());
            assert_eq!(c1.digest(), c2.digest());
        }

        // Next, tests that different consensus commit digests will produce the different checkpoint contents.
        {
            let t1 = VerifiedTransaction::new_consensus_commit_prologue_v3(
                1,
                2,
                100,
                ConsensusCommitDigest::default(),
                ConsensusDeterminedVersionAssignments::empty_for_testing(),
            );
            let t2 = VerifiedTransaction::new_consensus_commit_prologue_v3(
                1,
                2,
                100,
                ConsensusCommitDigest::random(),
                ConsensusDeterminedVersionAssignments::empty_for_testing(),
            );
            let c1 = generate_test_checkpoint_summary_from_digest(*t1.digest());
            let c2 = generate_test_checkpoint_summary_from_digest(*t2.digest());
            assert_ne!(c1.digest(), c2.digest());
        }
    }
}
