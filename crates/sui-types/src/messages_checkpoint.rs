// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{
    ExecutionData, ExecutionDigests, FullObjectRef, VerifiedExecutionData, random_object_ref,
};
use crate::base_types::{ObjectID, SequenceNumber};
use crate::committee::{EpochId, ProtocolVersion, StakeUnit};
use crate::crypto::{
    AccountKeyPair, AggregateAuthoritySignature, AuthoritySignInfo, AuthoritySignInfoTrait,
    AuthorityStrongQuorumSignInfo, RandomnessRound, default_hash, get_key_pair,
};
use crate::digests::{CheckpointArtifactsDigest, Digest, ObjectDigest};
use crate::effects::{TestEffectsBuilder, TransactionEffects, TransactionEffectsAPI};
use crate::error::SuiResult;
use crate::full_checkpoint_content::CheckpointData;
use crate::gas::GasCostSummary;
use crate::global_state_hash::GlobalStateHash;
use crate::message_envelope::{Envelope, Message, TrustedEnvelope, VerifiedEnvelope};
use crate::signature::GenericSignature;
use crate::sui_serde::AsProtocolVersion;
use crate::sui_serde::BigInt;
use crate::sui_serde::Readable;
use crate::transaction::{Transaction, TransactionData};
use crate::{base_types::AuthorityName, committee::Committee, error::SuiErrorKind};
use anyhow::Result;
use fastcrypto::hash::Blake2b256;
use fastcrypto::hash::MultisetHash;
use fastcrypto::merkle::MerkleTree;
use mysten_metrics::histogram::Histogram as MystenHistogram;
use once_cell::sync::OnceCell;
use prometheus::Histogram;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use shared_crypto::intent::{Intent, IntentScope};
use std::collections::{BTreeMap, BTreeSet};
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
        GlobalStateHash::default().digest().into()
    }
}

/// CheckpointArtifact is a type that represents various artifacts of a checkpoint.
/// We hash all the artifacts together to get the checkpoint artifacts digest
/// that is included in the checkpoint summary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CheckpointArtifact {
    /// The post-checkpoint state of all objects modified in the checkpoint.
    /// It also includes objects that were deleted or wrapped in the checkpoint.
    ObjectStates(BTreeMap<ObjectID, (SequenceNumber, ObjectDigest)>),
    // In the future, we can add more artifacts e.g., execution digests, events, etc.
}

impl CheckpointArtifact {
    pub fn digest(&self) -> SuiResult<Digest> {
        match self {
            Self::ObjectStates(object_states) => {
                let tree = MerkleTree::<Blake2b256>::build_from_unserialized(
                    object_states
                        .iter()
                        .map(|(id, (seq, digest))| (id, seq, digest)),
                )
                .map_err(|e| SuiErrorKind::GenericAuthorityError {
                    error: format!("Failed to build Merkle tree: {}", e),
                })?;
                let root = tree.root().bytes();
                Ok(Digest::new(root))
            }
        }
    }

    pub fn artifact_type(&self) -> &'static str {
        match self {
            Self::ObjectStates(_) => "ObjectStates",
            // Future variants...
        }
    }
}

#[derive(Debug)]
pub struct CheckpointArtifacts {
    /// An ordered list of artifacts.
    artifacts: BTreeSet<CheckpointArtifact>,
}

impl CheckpointArtifacts {
    pub fn new() -> Self {
        Self {
            artifacts: BTreeSet::new(),
        }
    }

    pub fn add_artifact(&mut self, artifact: CheckpointArtifact) -> SuiResult<()> {
        if self
            .artifacts
            .iter()
            .any(|existing| existing.artifact_type() == artifact.artifact_type())
        {
            return Err(SuiErrorKind::GenericAuthorityError {
                error: format!("Artifact {} already exists", artifact.artifact_type()),
            }
            .into());
        }
        self.artifacts.insert(artifact);
        Ok(())
    }

    pub fn from_object_states(
        object_states: BTreeMap<ObjectID, (SequenceNumber, ObjectDigest)>,
    ) -> Self {
        CheckpointArtifacts {
            artifacts: BTreeSet::from([CheckpointArtifact::ObjectStates(object_states)]),
        }
    }

    /// Get the object states if present
    pub fn object_states(&self) -> SuiResult<&BTreeMap<ObjectID, (SequenceNumber, ObjectDigest)>> {
        self.artifacts
            .iter()
            .find(|artifact| matches!(artifact, CheckpointArtifact::ObjectStates(_)))
            .map(|artifact| match artifact {
                CheckpointArtifact::ObjectStates(states) => states,
            })
            .ok_or(
                SuiErrorKind::GenericAuthorityError {
                    error: "Object states not found in checkpoint artifacts".to_string(),
                }
                .into(),
            )
    }

    pub fn digest(&self) -> SuiResult<CheckpointArtifactsDigest> {
        // Already sorted by BTreeSet!
        let digests = self
            .artifacts
            .iter()
            .map(|a| a.digest())
            .collect::<Result<Vec<_>, _>>()?;

        CheckpointArtifactsDigest::from_artifact_digests(digests)
    }
}

impl Default for CheckpointArtifacts {
    fn default() -> Self {
        Self::new()
    }
}

impl From<&[&TransactionEffects]> for CheckpointArtifacts {
    fn from(effects: &[&TransactionEffects]) -> Self {
        let mut latest_object_states = BTreeMap::new();
        for e in effects {
            for (id, seq, digest) in e.written() {
                if let Some((old_seq, _)) = latest_object_states.insert(id, (seq, digest)) {
                    assert!(
                        old_seq < seq,
                        "Object states should be monotonically increasing"
                    );
                }
            }
        }

        CheckpointArtifacts::from_object_states(latest_object_states)
    }
}

impl From<&[TransactionEffects]> for CheckpointArtifacts {
    fn from(effects: &[TransactionEffects]) -> Self {
        let effect_refs: Vec<&TransactionEffects> = effects.iter().collect();
        Self::from(effect_refs.as_slice())
    }
}

impl From<&CheckpointData> for CheckpointArtifacts {
    fn from(checkpoint_data: &CheckpointData) -> Self {
        let effects = checkpoint_data
            .transactions
            .iter()
            .map(|tx| &tx.effects)
            .collect::<Vec<_>>();

        Self::from(effects.as_slice())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub enum CheckpointCommitment {
    ECMHLiveObjectSetDigest(ECMHLiveObjectSetDigest),
    CheckpointArtifactsDigest(CheckpointArtifactsDigest),
}

impl From<ECMHLiveObjectSetDigest> for CheckpointCommitment {
    fn from(d: ECMHLiveObjectSetDigest) -> Self {
        Self::ECMHLiveObjectSetDigest(d)
    }
}

impl From<CheckpointArtifactsDigest> for CheckpointCommitment {
    fn from(d: CheckpointArtifactsDigest) -> Self {
        Self::CheckpointArtifactsDigest(d)
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
        checkpoint_commitments: Vec<CheckpointCommitment>,
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
            checkpoint_commitments,
        }
    }

    pub fn verify_epoch(&self, epoch: EpochId) -> SuiResult {
        fp_ensure!(
            self.epoch == epoch,
            SuiErrorKind::WrongEpoch {
                expected_epoch: epoch,
                actual_epoch: self.epoch,
            }
            .into()
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

    pub fn checkpoint_artifacts_digest(&self) -> SuiResult<&CheckpointArtifactsDigest> {
        self.checkpoint_commitments
            .iter()
            .find_map(|c| match c {
                CheckpointCommitment::CheckpointArtifactsDigest(digest) => Some(digest),
                _ => None,
            })
            .ok_or(
                SuiErrorKind::GenericAuthorityError {
                    error: "Checkpoint artifacts digest not found in checkpoint commitments"
                        .to_string(),
                }
                .into(),
            )
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
                SuiErrorKind::GenericAuthorityError{error:format!("Checkpoint contents digest mismatch: summary={:?}, received content digest {:?}, received {} transactions", self.data(), content_digest, contents.size())}.into()
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
    V2(CheckpointContentsV2),
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CheckpointContentsV2 {
    #[serde(skip)]
    digest: OnceCell<CheckpointContentsDigest>,

    transactions: Vec<CheckpointTransactionContents>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CheckpointTransactionContents {
    pub digest: ExecutionDigests,

    /// Each signature is paired with the version of the AddressAliases object
    /// that was used to verify it.
    pub user_signatures: Vec<(GenericSignature, Option<SequenceNumber>)>,
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

    pub fn new_v2(
        effects: &[TransactionEffects],
        signatures: Vec<Vec<(GenericSignature, Option<SequenceNumber>)>>,
    ) -> Self {
        assert_eq!(effects.len(), signatures.len());
        Self::V2(CheckpointContentsV2 {
            digest: Default::default(),
            transactions: effects
                .iter()
                .zip(signatures)
                .map(|(e, s)| CheckpointTransactionContents {
                    digest: e.execution_digests(),
                    user_signatures: s,
                })
                .collect(),
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

    fn into_v1(self) -> CheckpointContentsV1 {
        let digest = *self.digest();
        match self {
            Self::V1(c) => c,
            Self::V2(c) => CheckpointContentsV1 {
                // Preserve V2 digest when generating a V1 view of a CheckpointContentsV2.
                digest: OnceCell::with_value(digest),
                transactions: c.transactions.iter().map(|t| t.digest).collect(),
                user_signatures: c
                    .transactions
                    .iter()
                    .map(|t| {
                        t.user_signatures
                            .iter()
                            .map(|(s, _)| s.to_owned())
                            .collect()
                    })
                    .collect(),
            },
        }
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &ExecutionDigests> + '_ {
        match self {
            Self::V1(v) => itertools::Either::Left(v.transactions.iter()),
            Self::V2(v) => itertools::Either::Right(v.transactions.iter().map(|t| &t.digest)),
        }
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

    pub fn inner(&self) -> CheckpointContentsView<'_> {
        match self {
            Self::V1(c) => CheckpointContentsView::V1 {
                transactions: &c.transactions,
                user_signatures: &c.user_signatures,
            },
            Self::V2(c) => CheckpointContentsView::V2(&c.transactions),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Self::V1(c) => c.transactions.len(),
            Self::V2(c) => c.transactions.len(),
        }
    }

    pub fn digest(&self) -> &CheckpointContentsDigest {
        match self {
            Self::V1(c) => c
                .digest
                .get_or_init(|| CheckpointContentsDigest::new(default_hash(self))),
            Self::V2(c) => c
                .digest
                .get_or_init(|| CheckpointContentsDigest::new(default_hash(self))),
        }
    }
}

// Enables slice-style access to CheckpointContents tx digests without extra clones.
pub enum CheckpointContentsView<'a> {
    V1 {
        transactions: &'a [ExecutionDigests],
        user_signatures: &'a [Vec<GenericSignature>],
    },
    V2(&'a [CheckpointTransactionContents]),
}

impl CheckpointContentsView<'_> {
    pub fn len(&self) -> usize {
        match self {
            Self::V1 { transactions, .. } => transactions.len(),
            Self::V2(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get_digests(&self, index: usize) -> Option<&ExecutionDigests> {
        match self {
            Self::V1 { transactions, .. } => transactions.get(index),
            Self::V2(v) => v.get(index).map(|t| &t.digest),
        }
    }

    pub fn first_digests(&self) -> Option<&ExecutionDigests> {
        self.get_digests(0)
    }

    pub fn digests_iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = &ExecutionDigests> + ExactSizeIterator {
        match self {
            Self::V1 { transactions, .. } => itertools::Either::Left(transactions.iter()),
            Self::V2(v) => itertools::Either::Right(v.iter().map(|t| &t.digest)),
        }
    }

    /// Returns the user_signatures for a transaction at the given index along with
    /// the version of the AddressAliases object that was used to verify it.
    pub fn user_signatures(
        &self,
        index: usize,
    ) -> Option<Vec<(GenericSignature, Option<SequenceNumber>)>> {
        match self {
            Self::V1 {
                user_signatures, ..
            } => user_signatures
                .get(index)
                .map(|sigs| sigs.iter().map(|sig| (sig.clone(), None)).collect()),
            Self::V2(v) => v.get(index).map(|t| t.user_signatures.clone()),
        }
    }
}

impl std::ops::Index<usize> for CheckpointContentsView<'_> {
    type Output = ExecutionDigests;

    fn index(&self, index: usize) -> &Self::Output {
        match self {
            Self::V1 { transactions, .. } => &transactions[index],
            Self::V2(v) => &v[index].digest,
        }
    }
}

/// Same as CheckpointContents, but contains full contents of all transactions, effects,
/// and user signatures associated with the checkpoint.
// NOTE: This data structure is used for state sync of checkpoints. Therefore we attempt
// to estimate its size in CheckpointBuilder in order to limit the maximum serialized
// size of a checkpoint sent over the network. If this struct is modified,
// CheckpointBuilder::split_checkpoint_chunks should also be updated accordingly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionedFullCheckpointContents {
    V1(FullCheckpointContents),
    V2(FullCheckpointContentsV2),
}

impl VersionedFullCheckpointContents {
    pub fn from_contents_and_execution_data(
        contents: CheckpointContents,
        execution_data: impl Iterator<Item = ExecutionData>,
    ) -> Self {
        let transactions: Vec<_> = execution_data.collect();
        match contents {
            CheckpointContents::V1(v1) => Self::V1(FullCheckpointContents {
                transactions,
                user_signatures: v1.user_signatures,
            }),
            CheckpointContents::V2(v2) => Self::V2(FullCheckpointContentsV2 {
                transactions,
                user_signatures: v2
                    .transactions
                    .into_iter()
                    .map(|tx| tx.user_signatures)
                    .collect(),
            }),
        }
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

    pub fn into_v1(self) -> FullCheckpointContents {
        match self {
            Self::V1(c) => c,
            Self::V2(c) => FullCheckpointContents {
                transactions: c.transactions,
                user_signatures: c
                    .user_signatures
                    .into_iter()
                    .map(|sigs| sigs.into_iter().map(|(sig, _)| sig).collect())
                    .collect(),
            },
        }
    }

    pub fn into_checkpoint_contents(self) -> CheckpointContents {
        match self {
            Self::V1(c) => c.into_checkpoint_contents(),
            Self::V2(c) => c.into_checkpoint_contents(),
        }
    }

    pub fn checkpoint_contents(&self) -> CheckpointContents {
        match self {
            Self::V1(c) => c.checkpoint_contents(),
            Self::V2(c) => c.checkpoint_contents(),
        }
    }

    pub fn iter(&self) -> Iter<'_, ExecutionData> {
        match self {
            Self::V1(c) => c.iter(),
            Self::V2(c) => c.transactions.iter(),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Self::V1(c) => c.transactions.len(),
            Self::V2(c) => c.transactions.len(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullCheckpointContentsV2 {
    transactions: Vec<ExecutionData>,
    user_signatures: Vec<Vec<(GenericSignature, Option<SequenceNumber>)>>,
}

impl FullCheckpointContentsV2 {
    pub fn checkpoint_contents(&self) -> CheckpointContents {
        CheckpointContents::V2(CheckpointContentsV2 {
            digest: Default::default(),
            transactions: self
                .transactions
                .iter()
                .zip(&self.user_signatures)
                .map(|(tx, sigs)| CheckpointTransactionContents {
                    digest: tx.digests(),
                    user_signatures: sigs.clone(),
                })
                .collect(),
        })
    }

    pub fn into_checkpoint_contents(self) -> CheckpointContents {
        CheckpointContents::V2(CheckpointContentsV2 {
            digest: Default::default(),
            transactions: self
                .transactions
                .into_iter()
                .zip(self.user_signatures)
                .map(|(tx, sigs)| CheckpointTransactionContents {
                    digest: tx.digests(),
                    user_signatures: sigs,
                })
                .collect(),
        })
    }
}

/// Deprecated version of full checkpoint contents corresponding to CheckpointContentsV1.
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

    pub fn iter(&self) -> Iter<'_, ExecutionData> {
        self.transactions.iter()
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

    pub fn random_for_testing() -> Self {
        let (a, key): (_, AccountKeyPair) = get_key_pair();
        let transaction = Transaction::from_data_and_signer(
            TransactionData::new_transfer(
                a,
                FullObjectRef::from_fastpath_ref(random_object_ref()),
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

impl IntoIterator for VersionedFullCheckpointContents {
    type Item = ExecutionData;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::V1(c) => c.transactions.into_iter(),
            Self::V2(c) => c.transactions.into_iter(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifiedUserSignatures {
    V1(Vec<Vec<GenericSignature>>),
    V2(Vec<Vec<(GenericSignature, Option<SequenceNumber>)>>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedCheckpointContents {
    transactions: Vec<VerifiedExecutionData>,
    /// This field 'pins' user signatures for the checkpoint
    /// The length of this vector is same as length of transactions vector
    /// System transactions has empty signatures
    user_signatures: VerifiedUserSignatures,
}

impl VerifiedCheckpointContents {
    pub fn new_unchecked(contents: VersionedFullCheckpointContents) -> Self {
        match contents {
            VersionedFullCheckpointContents::V1(c) => Self {
                transactions: c
                    .transactions
                    .into_iter()
                    .map(VerifiedExecutionData::new_unchecked)
                    .collect(),
                user_signatures: VerifiedUserSignatures::V1(c.user_signatures),
            },
            VersionedFullCheckpointContents::V2(c) => Self {
                transactions: c
                    .transactions
                    .into_iter()
                    .map(VerifiedExecutionData::new_unchecked)
                    .collect(),
                user_signatures: VerifiedUserSignatures::V2(c.user_signatures),
            },
        }
    }

    pub fn iter(&self) -> Iter<'_, VerifiedExecutionData> {
        self.transactions.iter()
    }

    pub fn transactions(&self) -> &[VerifiedExecutionData] {
        &self.transactions
    }

    pub fn into_inner(self) -> VersionedFullCheckpointContents {
        let transactions: Vec<_> = self
            .transactions
            .into_iter()
            .map(|tx| tx.into_inner())
            .collect();

        match self.user_signatures {
            VerifiedUserSignatures::V1(user_signatures) => {
                VersionedFullCheckpointContents::V1(FullCheckpointContents {
                    transactions,
                    user_signatures,
                })
            }
            VerifiedUserSignatures::V2(user_signatures) => {
                VersionedFullCheckpointContents::V2(FullCheckpointContentsV2 {
                    transactions,
                    user_signatures,
                })
            }
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
    use rand::SeedableRng;
    use rand::prelude::StdRng;

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
        assert!(
            checkpoint_cert
                .verify_with_contents(&committee, Some(&set))
                .is_ok()
        );

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

    #[test]
    fn test_artifacts() {
        let mut artifacts = CheckpointArtifacts::new();
        let o = CheckpointArtifact::ObjectStates(BTreeMap::new());
        assert!(artifacts.add_artifact(o.clone()).is_ok());
        assert!(artifacts.add_artifact(o.clone()).is_err());
    }
}
