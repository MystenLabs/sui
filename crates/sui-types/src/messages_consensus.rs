// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{AuthorityName, ConsensusObjectSequenceKey, ObjectRef, TransactionDigest};
use crate::base_types::{ConciseableName, ObjectID, SequenceNumber};
use crate::digests::ConsensusCommitDigest;
use crate::execution::ExecutionTimeObservationKey;
use crate::messages_checkpoint::{CheckpointSequenceNumber, CheckpointSignatureMessage};
use crate::supported_protocol_versions::{
    Chain, SupportedProtocolVersions, SupportedProtocolVersionsWithHashes,
};
use crate::transaction::{CertifiedTransaction, Transaction};
use byteorder::{BigEndian, ReadBytesExt};
use fastcrypto::error::FastCryptoResult;
use fastcrypto::groups::bls12381;
use fastcrypto_tbls::dkg_v1;
use fastcrypto_zkp::bn254::zk_login::{JwkId, JWK};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The index of an authority in the consensus committee.
/// The value should be the same in Sui committee.
pub type AuthorityIndex = u32;

/// Consensus round number in u64 instead of u32 for compatibility with Narwhal.
pub type Round = u64;

/// The index of a transaction in a consensus block.
pub type TransactionIndex = u16;

/// Non-decreasing timestamp produced by consensus in ms.
pub type TimestampMs = u64;

/// Only commit_timestamp_ms is passed to the move call currently.
/// However we include epoch and round to make sure each ConsensusCommitPrologue has a unique tx digest.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ConsensusCommitPrologue {
    /// Epoch of the commit prologue transaction
    pub epoch: u64,
    /// Consensus round of the commit. Using u64 for compatibility.
    pub round: u64,
    /// Unix timestamp from consensus commit.
    pub commit_timestamp_ms: TimestampMs,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ConsensusCommitPrologueV2 {
    /// Epoch of the commit prologue transaction
    pub epoch: u64,
    /// Consensus round of the commit
    pub round: u64,
    /// Unix timestamp from consensus commit.
    pub commit_timestamp_ms: TimestampMs,
    /// Digest of consensus output
    pub consensus_commit_digest: ConsensusCommitDigest,
}

/// Uses an enum to allow for future expansion of the ConsensusDeterminedVersionAssignments.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ConsensusDeterminedVersionAssignments {
    // Cancelled transaction version assignment.
    CancelledTransactions(Vec<(TransactionDigest, Vec<(ObjectID, SequenceNumber)>)>),
    CancelledTransactionsV2(
        Vec<(
            TransactionDigest,
            Vec<(ConsensusObjectSequenceKey, SequenceNumber)>,
        )>,
    ),
}

impl ConsensusDeterminedVersionAssignments {
    pub fn empty_for_testing() -> Self {
        Self::CancelledTransactions(Vec::new())
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ConsensusCommitPrologueV3 {
    /// Epoch of the commit prologue transaction
    pub epoch: u64,
    /// Consensus round of the commit
    pub round: u64,
    /// The sub DAG index of the consensus commit. This field will be populated if there
    /// are multiple consensus commits per round.
    pub sub_dag_index: Option<u64>,
    /// Unix timestamp from consensus commit.
    pub commit_timestamp_ms: TimestampMs,
    /// Digest of consensus output
    pub consensus_commit_digest: ConsensusCommitDigest,
    /// Stores consensus handler determined shared object version assignments.
    pub consensus_determined_version_assignments: ConsensusDeterminedVersionAssignments,
}

// In practice, JWKs are about 500 bytes of json each, plus a bit more for the ID.
// 4096 should give us plenty of space for any imaginable JWK while preventing DoSes.
static MAX_TOTAL_JWK_SIZE: usize = 4096;

pub fn check_total_jwk_size(id: &JwkId, jwk: &JWK) -> bool {
    id.iss.len() + id.kid.len() + jwk.kty.len() + jwk.alg.len() + jwk.e.len() + jwk.n.len()
        <= MAX_TOTAL_JWK_SIZE
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsensusTransaction {
    /// Encodes an u64 unique tracking id to allow us trace a message between Sui and consensus.
    /// Use an byte array instead of u64 to ensure stable serialization.
    pub tracking_id: [u8; 8],
    pub kind: ConsensusTransactionKind,
}

#[derive(Serialize, Deserialize, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub enum ConsensusTransactionKey {
    Certificate(TransactionDigest),
    CheckpointSignature(AuthorityName, CheckpointSequenceNumber),
    EndOfPublish(AuthorityName),
    CapabilityNotification(AuthorityName, u64 /* generation */),
    // Key must include both id and jwk, because honest validators could be given multiple jwks for
    // the same id by malfunctioning providers.
    NewJWKFetched(Box<(AuthorityName, JwkId, JWK)>),
    RandomnessDkgMessage(AuthorityName),
    RandomnessDkgConfirmation(AuthorityName),
    ExecutionTimeObservation(AuthorityName, u64 /* generation */),
}

impl Debug for ConsensusTransactionKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Certificate(digest) => write!(f, "Certificate({:?})", digest),
            Self::CheckpointSignature(name, seq) => {
                write!(f, "CheckpointSignature({:?}, {:?})", name.concise(), seq)
            }
            Self::EndOfPublish(name) => write!(f, "EndOfPublish({:?})", name.concise()),
            Self::CapabilityNotification(name, generation) => write!(
                f,
                "CapabilityNotification({:?}, {:?})",
                name.concise(),
                generation
            ),
            Self::NewJWKFetched(key) => {
                let (authority, id, jwk) = &**key;
                write!(
                    f,
                    "NewJWKFetched({:?}, {:?}, {:?})",
                    authority.concise(),
                    id,
                    jwk
                )
            }
            Self::RandomnessDkgMessage(name) => {
                write!(f, "RandomnessDkgMessage({:?})", name.concise())
            }
            Self::RandomnessDkgConfirmation(name) => {
                write!(f, "RandomnessDkgConfirmation({:?})", name.concise())
            }
            Self::ExecutionTimeObservation(name, generation) => {
                write!(
                    f,
                    "ExecutionTimeObservation({:?}, {generation:?})",
                    name.concise()
                )
            }
        }
    }
}

/// Used to advertise capabilities of each authority via consensus. This allows validators to
/// negotiate the creation of the ChangeEpoch transaction.
#[derive(Serialize, Deserialize, Clone, Hash)]
pub struct AuthorityCapabilitiesV1 {
    /// Originating authority - must match consensus transaction source.
    pub authority: AuthorityName,
    /// Generation number set by sending authority. Used to determine which of multiple
    /// AuthorityCapabilities messages from the same authority is the most recent.
    ///
    /// (Currently, we just set this to the current time in milliseconds since the epoch, but this
    /// should not be interpreted as a timestamp.)
    pub generation: u64,

    /// ProtocolVersions that the authority supports.
    pub supported_protocol_versions: SupportedProtocolVersions,

    /// The ObjectRefs of all versions of system packages that the validator possesses.
    /// Used to determine whether to do a framework/movestdlib upgrade.
    pub available_system_packages: Vec<ObjectRef>,
}

impl Debug for AuthorityCapabilitiesV1 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorityCapabilities")
            .field("authority", &self.authority.concise())
            .field("generation", &self.generation)
            .field(
                "supported_protocol_versions",
                &self.supported_protocol_versions,
            )
            .field("available_system_packages", &self.available_system_packages)
            .finish()
    }
}

impl AuthorityCapabilitiesV1 {
    pub fn new(
        authority: AuthorityName,
        supported_protocol_versions: SupportedProtocolVersions,
        available_system_packages: Vec<ObjectRef>,
    ) -> Self {
        let generation = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Sui did not exist prior to 1970")
            .as_millis()
            .try_into()
            .expect("This build of sui is not supported in the year 500,000,000");
        Self {
            authority,
            generation,
            supported_protocol_versions,
            available_system_packages,
        }
    }
}

/// Used to advertise capabilities of each authority via consensus. This allows validators to
/// negotiate the creation of the ChangeEpoch transaction.
#[derive(Serialize, Deserialize, Clone, Hash)]
pub struct AuthorityCapabilitiesV2 {
    /// Originating authority - must match transaction source authority from consensus.
    pub authority: AuthorityName,
    /// Generation number set by sending authority. Used to determine which of multiple
    /// AuthorityCapabilities messages from the same authority is the most recent.
    ///
    /// (Currently, we just set this to the current time in milliseconds since the epoch, but this
    /// should not be interpreted as a timestamp.)
    pub generation: u64,

    /// ProtocolVersions that the authority supports.
    pub supported_protocol_versions: SupportedProtocolVersionsWithHashes,

    /// The ObjectRefs of all versions of system packages that the validator possesses.
    /// Used to determine whether to do a framework/movestdlib upgrade.
    pub available_system_packages: Vec<ObjectRef>,
}

impl Debug for AuthorityCapabilitiesV2 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorityCapabilities")
            .field("authority", &self.authority.concise())
            .field("generation", &self.generation)
            .field(
                "supported_protocol_versions",
                &self.supported_protocol_versions,
            )
            .field("available_system_packages", &self.available_system_packages)
            .finish()
    }
}

impl AuthorityCapabilitiesV2 {
    pub fn new(
        authority: AuthorityName,
        chain: Chain,
        supported_protocol_versions: SupportedProtocolVersions,
        available_system_packages: Vec<ObjectRef>,
    ) -> Self {
        let generation = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Sui did not exist prior to 1970")
            .as_millis()
            .try_into()
            .expect("This build of sui is not supported in the year 500,000,000");
        Self {
            authority,
            generation,
            supported_protocol_versions:
                SupportedProtocolVersionsWithHashes::from_supported_versions(
                    supported_protocol_versions,
                    chain,
                ),
            available_system_packages,
        }
    }
}

/// Used to share estimates of transaction execution times with other validators for
/// congestion control.
#[derive(Debug, Default, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ExecutionTimeObservation {
    /// Originating authority - must match transaction source authority from consensus.
    pub authority: AuthorityName,
    /// Generation number set by sending authority. Used to determine which of multiple
    /// ExecutionTimeObservation messages from the same authority is the most recent.
    pub generation: u64,

    /// Estimated execution durations by key.
    pub estimates: Vec<(ExecutionTimeObservationKey, Duration)>,
}

impl ExecutionTimeObservation {
    pub fn new(
        authority: AuthorityName,
        estimates: Vec<(ExecutionTimeObservationKey, Duration)>,
    ) -> Self {
        let generation = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Sui did not exist prior to 1970")
            .as_micros()
            .try_into()
            .expect("This build of sui is not supported in the year 500,000");
        Self {
            authority,
            generation,
            estimates,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ConsensusTransactionKind {
    CertifiedTransaction(Box<CertifiedTransaction>),
    CheckpointSignature(Box<CheckpointSignatureMessage>),
    EndOfPublish(AuthorityName),

    CapabilityNotification(AuthorityCapabilitiesV1),

    NewJWKFetched(AuthorityName, JwkId, JWK),
    RandomnessStateUpdate(u64, Vec<u8>), // deprecated
    // DKG is used to generate keys for use in the random beacon protocol.
    // `RandomnessDkgMessage` is sent out at start-of-epoch to initiate the process.
    // Contents are a serialized `fastcrypto_tbls::dkg::Message`.
    RandomnessDkgMessage(AuthorityName, Vec<u8>),
    // `RandomnessDkgConfirmation` is the second DKG message, sent as soon as a threshold amount of
    // `RandomnessDkgMessages` have been received locally, to complete the key generation process.
    // Contents are a serialized `fastcrypto_tbls::dkg::Confirmation`.
    RandomnessDkgConfirmation(AuthorityName, Vec<u8>),

    CapabilityNotificationV2(AuthorityCapabilitiesV2),

    UserTransaction(Box<Transaction>),

    ExecutionTimeObservation(ExecutionTimeObservation),
}

impl ConsensusTransactionKind {
    pub fn is_dkg(&self) -> bool {
        matches!(
            self,
            ConsensusTransactionKind::RandomnessDkgMessage(_, _)
                | ConsensusTransactionKind::RandomnessDkgConfirmation(_, _)
        )
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum VersionedDkgMessage {
    V0(), // deprecated
    V1(dkg_v1::Message<bls12381::G2Element, bls12381::G2Element>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionedDkgConfirmation {
    V0(), // deprecated
    V1(dkg_v1::Confirmation<bls12381::G2Element>),
}

impl Debug for VersionedDkgMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionedDkgMessage::V0() => write!(f, "Deprecated VersionedDkgMessage version 0"),
            VersionedDkgMessage::V1(msg) => write!(
                f,
                "DKG V1 Message with sender={}, vss_pk.degree={}, encrypted_shares.len()={}",
                msg.sender,
                msg.vss_pk.degree(),
                msg.encrypted_shares.len(),
            ),
        }
    }
}

impl VersionedDkgMessage {
    pub fn sender(&self) -> u16 {
        match self {
            VersionedDkgMessage::V0() => panic!("BUG: invalid VersionedDkgMessage version"),
            VersionedDkgMessage::V1(msg) => msg.sender,
        }
    }

    pub fn create(
        dkg_version: u64,
        party: Arc<dkg_v1::Party<bls12381::G2Element, bls12381::G2Element>>,
    ) -> FastCryptoResult<VersionedDkgMessage> {
        assert_eq!(dkg_version, 1, "BUG: invalid DKG version");
        let msg = party.create_message(&mut rand::thread_rng())?;
        Ok(VersionedDkgMessage::V1(msg))
    }

    pub fn unwrap_v1(self) -> dkg_v1::Message<bls12381::G2Element, bls12381::G2Element> {
        match self {
            VersionedDkgMessage::V1(msg) => msg,
            _ => panic!("BUG: expected V1 message"),
        }
    }

    pub fn is_valid_version(&self, dkg_version: u64) -> bool {
        matches!((self, dkg_version), (VersionedDkgMessage::V1(_), 1))
    }
}

impl VersionedDkgConfirmation {
    pub fn sender(&self) -> u16 {
        match self {
            VersionedDkgConfirmation::V0() => {
                panic!("BUG: invalid VersionedDkgConfimation version")
            }
            VersionedDkgConfirmation::V1(msg) => msg.sender,
        }
    }

    pub fn num_of_complaints(&self) -> usize {
        match self {
            VersionedDkgConfirmation::V0() => {
                panic!("BUG: invalid VersionedDkgConfimation version")
            }
            VersionedDkgConfirmation::V1(msg) => msg.complaints.len(),
        }
    }

    pub fn unwrap_v1(&self) -> &dkg_v1::Confirmation<bls12381::G2Element> {
        match self {
            VersionedDkgConfirmation::V1(msg) => msg,
            _ => panic!("BUG: expected V1 confirmation"),
        }
    }

    pub fn is_valid_version(&self, dkg_version: u64) -> bool {
        matches!((self, dkg_version), (VersionedDkgConfirmation::V1(_), 1))
    }
}

impl ConsensusTransaction {
    pub fn new_certificate_message(
        authority: &AuthorityName,
        certificate: CertifiedTransaction,
    ) -> Self {
        let mut hasher = DefaultHasher::new();
        let tx_digest = certificate.digest();
        tx_digest.hash(&mut hasher);
        authority.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::CertifiedTransaction(Box::new(certificate)),
        }
    }

    pub fn new_user_transaction_message(authority: &AuthorityName, tx: Transaction) -> Self {
        let mut hasher = DefaultHasher::new();
        let tx_digest = tx.digest();
        tx_digest.hash(&mut hasher);
        authority.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::UserTransaction(Box::new(tx)),
        }
    }

    pub fn new_checkpoint_signature_message(data: CheckpointSignatureMessage) -> Self {
        let mut hasher = DefaultHasher::new();
        data.summary.auth_sig().signature.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::CheckpointSignature(Box::new(data)),
        }
    }

    pub fn new_end_of_publish(authority: AuthorityName) -> Self {
        let mut hasher = DefaultHasher::new();
        authority.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::EndOfPublish(authority),
        }
    }

    pub fn new_capability_notification(capabilities: AuthorityCapabilitiesV1) -> Self {
        let mut hasher = DefaultHasher::new();
        capabilities.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::CapabilityNotification(capabilities),
        }
    }

    pub fn new_capability_notification_v2(capabilities: AuthorityCapabilitiesV2) -> Self {
        let mut hasher = DefaultHasher::new();
        capabilities.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::CapabilityNotificationV2(capabilities),
        }
    }

    pub fn new_mysticeti_certificate(
        round: u64,
        offset: u64,
        certificate: CertifiedTransaction,
    ) -> Self {
        let mut hasher = DefaultHasher::new();
        let tx_digest = certificate.digest();
        tx_digest.hash(&mut hasher);
        round.hash(&mut hasher);
        offset.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::CertifiedTransaction(Box::new(certificate)),
        }
    }

    pub fn new_jwk_fetched(authority: AuthorityName, id: JwkId, jwk: JWK) -> Self {
        let mut hasher = DefaultHasher::new();
        id.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::NewJWKFetched(authority, id, jwk),
        }
    }

    pub fn new_randomness_dkg_message(
        authority: AuthorityName,
        versioned_message: &VersionedDkgMessage,
    ) -> Self {
        let message =
            bcs::to_bytes(versioned_message).expect("message serialization should not fail");
        let mut hasher = DefaultHasher::new();
        message.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::RandomnessDkgMessage(authority, message),
        }
    }
    pub fn new_randomness_dkg_confirmation(
        authority: AuthorityName,
        versioned_confirmation: &VersionedDkgConfirmation,
    ) -> Self {
        let confirmation =
            bcs::to_bytes(versioned_confirmation).expect("message serialization should not fail");
        let mut hasher = DefaultHasher::new();
        confirmation.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::RandomnessDkgConfirmation(authority, confirmation),
        }
    }

    pub fn new_execution_time_observation(observation: ExecutionTimeObservation) -> Self {
        let mut hasher = DefaultHasher::new();
        observation.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::ExecutionTimeObservation(observation),
        }
    }

    pub fn get_tracking_id(&self) -> u64 {
        (&self.tracking_id[..])
            .read_u64::<BigEndian>()
            .unwrap_or_default()
    }

    pub fn key(&self) -> ConsensusTransactionKey {
        match &self.kind {
            ConsensusTransactionKind::CertifiedTransaction(cert) => {
                ConsensusTransactionKey::Certificate(*cert.digest())
            }
            ConsensusTransactionKind::CheckpointSignature(data) => {
                ConsensusTransactionKey::CheckpointSignature(
                    data.summary.auth_sig().authority,
                    data.summary.sequence_number,
                )
            }
            ConsensusTransactionKind::EndOfPublish(authority) => {
                ConsensusTransactionKey::EndOfPublish(*authority)
            }
            ConsensusTransactionKind::CapabilityNotification(cap) => {
                ConsensusTransactionKey::CapabilityNotification(cap.authority, cap.generation)
            }
            ConsensusTransactionKind::CapabilityNotificationV2(cap) => {
                ConsensusTransactionKey::CapabilityNotification(cap.authority, cap.generation)
            }
            ConsensusTransactionKind::NewJWKFetched(authority, id, key) => {
                ConsensusTransactionKey::NewJWKFetched(Box::new((
                    *authority,
                    id.clone(),
                    key.clone(),
                )))
            }
            ConsensusTransactionKind::RandomnessStateUpdate(_, _) => {
                unreachable!("there should never be a RandomnessStateUpdate with SequencedConsensusTransactionKind::External")
            }
            ConsensusTransactionKind::RandomnessDkgMessage(authority, _) => {
                ConsensusTransactionKey::RandomnessDkgMessage(*authority)
            }
            ConsensusTransactionKind::RandomnessDkgConfirmation(authority, _) => {
                ConsensusTransactionKey::RandomnessDkgConfirmation(*authority)
            }
            ConsensusTransactionKind::UserTransaction(tx) => {
                // Use the same key format as ConsensusTransactionKind::CertifiedTransaction,
                // because existing usages of ConsensusTransactionKey should not differentiate
                // between CertifiedTransaction and UserTransaction.
                ConsensusTransactionKey::Certificate(*tx.digest())
            }
            ConsensusTransactionKind::ExecutionTimeObservation(msg) => {
                ConsensusTransactionKey::ExecutionTimeObservation(msg.authority, msg.generation)
            }
        }
    }

    pub fn is_executable_transaction(&self) -> bool {
        matches!(self.kind, ConsensusTransactionKind::CertifiedTransaction(_))
            || matches!(self.kind, ConsensusTransactionKind::UserTransaction(_))
    }

    pub fn is_user_transaction(&self) -> bool {
        matches!(self.kind, ConsensusTransactionKind::UserTransaction(_))
    }

    pub fn is_end_of_publish(&self) -> bool {
        matches!(self.kind, ConsensusTransactionKind::EndOfPublish(_))
    }
}

#[test]
fn test_jwk_compatibility() {
    // Ensure that the JWK and JwkId structs in fastcrypto do not change formats.
    // If this test breaks DO NOT JUST UPDATE THE EXPECTED BYTES. Instead, add a local JWK or
    // JwkId struct that mirrors the fastcrypto struct, use it in AuthenticatorStateUpdate, and
    // add Into/From as necessary.
    let jwk = JWK {
        kty: "a".to_string(),
        e: "b".to_string(),
        n: "c".to_string(),
        alg: "d".to_string(),
    };

    let expected_jwk_bytes = vec![1, 97, 1, 98, 1, 99, 1, 100];
    let jwk_bcs = bcs::to_bytes(&jwk).unwrap();
    assert_eq!(jwk_bcs, expected_jwk_bytes);

    let id = JwkId {
        iss: "abc".to_string(),
        kid: "def".to_string(),
    };

    let expected_id_bytes = vec![3, 97, 98, 99, 3, 100, 101, 102];
    let id_bcs = bcs::to_bytes(&id).unwrap();
    assert_eq!(id_bcs, expected_id_bytes);
}
