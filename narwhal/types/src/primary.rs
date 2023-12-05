// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    error::{DagError, DagResult},
    serde::NarwhalBitmap,
};
use config::{AuthorityIdentifier, Committee, Epoch, Stake, WorkerCache, WorkerId};
use crypto::{
    to_intent_message, AggregateSignature, AggregateSignatureBytes,
    NarwhalAuthorityAggregateSignature, NarwhalAuthoritySignature, NetworkPublicKey, PublicKey,
    RandomnessPartialSignature, Signature,
};
use derive_builder::Builder;
use enum_dispatch::enum_dispatch;
use fastcrypto::{
    hash::{Digest, Hash, HashFunction},
    signature_service::SignatureService,
    traits::{AggregateAuthenticator, Signer, VerifyingKey},
};
use indexmap::IndexMap;
use mysten_util_mem::MallocSizeOf;
use once_cell::sync::{Lazy, OnceCell};
use proptest_derive::Arbitrary;
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, SystemTime},
};
use sui_protocol_config::ProtocolConfig;
use tracing::warn;

/// The round number.
pub type Round = u64;

/// The epoch UNIX timestamp in milliseconds
pub type TimestampMs = u64;

pub trait Timestamp {
    // Returns the time elapsed between the timestamp
    // and "now". The result is a Duration.
    fn elapsed(&self) -> Duration;
}

impl Timestamp for TimestampMs {
    fn elapsed(&self) -> Duration {
        let diff = now().saturating_sub(*self);
        Duration::from_millis(diff)
    }
}

// Returns the current time expressed as UNIX
// timestamp in milliseconds
pub fn now() -> TimestampMs {
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(n) => n.as_millis() as TimestampMs,
        Err(_) => panic!("SystemTime before UNIX EPOCH!"),
    }
}

/// Round number of generated randomness.
#[derive(
    Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, MallocSizeOf,
)]
pub struct RandomnessRound(pub u64);

impl fmt::Display for RandomnessRound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Add for RandomnessRound {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl std::ops::Add<u64> for RandomnessRound {
    type Output = Self;
    fn add(self, other: u64) -> Self {
        Self(self.0 + other)
    }
}

impl RandomnessRound {
    pub fn new(round: u64) -> Self {
        Self(round)
    }

    pub fn checked_add(self, rhs: u64) -> Option<Self> {
        self.0.checked_add(rhs).map(Self)
    }

    pub fn signature_message(&self) -> Vec<u8> {
        format!("random_beacon round {}", self.0).into()
    }
}

// Additional metadata information for an entity.
//
// The structure as a whole is not signed. As a result this data
// should not be treated as trustworthy data and should be used
// for NON CRITICAL purposes only. For example should not be used
// for any processes that are part of our protocol that can affect
// safety or liveness.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Arbitrary, MallocSizeOf)]
pub struct Metadata {
    // timestamp of when the entity created. This is generated
    // by the node which creates the entity.
    pub created_at: TimestampMs,
}

impl Default for Metadata {
    fn default() -> Self {
        Metadata { created_at: now() }
    }
}

// This is a versioned `Metadata` type
// Refer to comments above the original `Metadata` struct for more details.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Arbitrary, MallocSizeOf)]
#[enum_dispatch(MetadataAPI)]
pub enum VersionedMetadata {
    V1(MetadataV1),
}

impl VersionedMetadata {
    pub fn new(_protocol_config: &ProtocolConfig) -> Self {
        Self::V1(MetadataV1 {
            created_at: now(),
            received_at: None,
        })
    }
}

#[enum_dispatch]
pub trait MetadataAPI {
    fn created_at(&self) -> &TimestampMs;
    fn set_created_at(&mut self, ts: TimestampMs);
    fn received_at(&self) -> Option<TimestampMs>;
    fn set_received_at(&mut self, ts: TimestampMs);
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Arbitrary, MallocSizeOf)]
pub struct MetadataV1 {
    // timestamp of when the entity created. This is generated
    // by the node which creates the entity.
    pub created_at: TimestampMs,
    // timestamp of when the entity was received by an other node. This will help
    // us calculate latencies that are not affected by clock drift or network
    // delays. This field is not set for own batches.
    pub received_at: Option<TimestampMs>,
}

impl MetadataAPI for MetadataV1 {
    fn created_at(&self) -> &TimestampMs {
        &self.created_at
    }

    fn set_created_at(&mut self, ts: TimestampMs) {
        self.created_at = ts;
    }

    fn received_at(&self) -> Option<TimestampMs> {
        self.received_at
    }

    fn set_received_at(&mut self, ts: TimestampMs) {
        self.received_at = Some(ts);
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Arbitrary)]
#[enum_dispatch(BatchAPI)]
pub enum Batch {
    V1(BatchV1),
    V2(BatchV2),
}

impl Batch {
    pub fn new(transactions: Vec<Transaction>, protocol_config: &ProtocolConfig) -> Self {
        Self::V2(BatchV2::new(transactions, protocol_config))
    }

    pub fn size(&self) -> usize {
        match self {
            Batch::V1(data) => data.size(),
            Batch::V2(data) => data.size(),
        }
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for Batch {
    type TypedDigest = BatchDigest;

    fn digest(&self) -> BatchDigest {
        match self {
            Batch::V1(data) => data.digest(),
            Batch::V2(data) => data.digest(),
        }
    }
}

#[enum_dispatch]
pub trait BatchAPI {
    fn transactions(&self) -> &Vec<Transaction>;
    fn transactions_mut(&mut self) -> &mut Vec<Transaction>;
    fn into_transactions(self) -> Vec<Transaction>;
    fn metadata(&self) -> &Metadata;
    fn metadata_mut(&mut self) -> &mut Metadata;

    // BatchV2 APIs
    fn versioned_metadata(&self) -> &VersionedMetadata;
    fn versioned_metadata_mut(&mut self) -> &mut VersionedMetadata;
}

pub type Transaction = Vec<u8>;
#[derive(Clone, Serialize, Deserialize, Default, Debug, PartialEq, Eq, Arbitrary)]
pub struct BatchV1 {
    pub transactions: Vec<Transaction>,
    pub metadata: Metadata,
}

impl BatchAPI for BatchV1 {
    fn transactions(&self) -> &Vec<Transaction> {
        &self.transactions
    }

    fn transactions_mut(&mut self) -> &mut Vec<Transaction> {
        &mut self.transactions
    }

    fn into_transactions(self) -> Vec<Transaction> {
        self.transactions
    }

    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }

    fn versioned_metadata(&self) -> &VersionedMetadata {
        unimplemented!("BatchV1 does not have a VersionedMetadata field");
    }

    fn versioned_metadata_mut(&mut self) -> &mut VersionedMetadata {
        unimplemented!("BatchV1 does not have a VersionedMetadata field");
    }
}

impl BatchV1 {
    pub fn new(transactions: Vec<Transaction>) -> Self {
        Self {
            transactions,
            metadata: Metadata::default(),
        }
    }

    pub fn size(&self) -> usize {
        self.transactions.iter().map(|t| t.len()).sum()
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Arbitrary)]
pub struct BatchV2 {
    pub transactions: Vec<Transaction>,
    // This field is not included as part of the batch digest
    pub versioned_metadata: VersionedMetadata,
}

impl BatchAPI for BatchV2 {
    fn transactions(&self) -> &Vec<Transaction> {
        &self.transactions
    }

    fn transactions_mut(&mut self) -> &mut Vec<Transaction> {
        &mut self.transactions
    }

    fn into_transactions(self) -> Vec<Transaction> {
        self.transactions
    }

    fn metadata(&self) -> &Metadata {
        unimplemented!("BatchV2 does not have a Metadata field");
    }

    fn metadata_mut(&mut self) -> &mut Metadata {
        unimplemented!("BatchV2 does not have a Metadata field");
    }

    fn versioned_metadata(&self) -> &VersionedMetadata {
        &self.versioned_metadata
    }

    fn versioned_metadata_mut(&mut self) -> &mut VersionedMetadata {
        &mut self.versioned_metadata
    }
}

impl BatchV2 {
    pub fn new(transactions: Vec<Transaction>, protocol_config: &ProtocolConfig) -> Self {
        Self {
            transactions,
            versioned_metadata: VersionedMetadata::new(protocol_config),
        }
    }

    pub fn size(&self) -> usize {
        self.transactions.iter().map(|t| t.len()).sum()
    }
}

// TODO: Remove once we have removed BatchV1 from the codebase.
pub fn validate_batch_version(
    batch: &Batch,
    protocol_config: &ProtocolConfig,
) -> anyhow::Result<()> {
    // We will only accept BatchV2 from the network.
    match batch {
        Batch::V1(_) => {
            Err(anyhow::anyhow!(format!(
                    "Received {batch:?} but network is at {:?} and this batch version is no longer supported",
                    protocol_config.version
                )))
        }
        Batch::V2(_) => {
            Ok(())
        }
    }
}

#[derive(
    Clone,
    Copy,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    MallocSizeOf,
    Arbitrary,
)]
pub struct BatchDigest(pub [u8; crypto::DIGEST_LENGTH]);

impl fmt::Debug for BatchDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
        )
    }
}

impl fmt::Display for BatchDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
                .get(0..16)
                .ok_or(fmt::Error)?
        )
    }
}

impl From<BatchDigest> for Digest<{ crypto::DIGEST_LENGTH }> {
    fn from(digest: BatchDigest) -> Self {
        Digest::new(digest.0)
    }
}

impl AsRef<[u8]> for BatchDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl BatchDigest {
    pub fn new(val: [u8; crypto::DIGEST_LENGTH]) -> BatchDigest {
        BatchDigest(val)
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for BatchV1 {
    type TypedDigest = BatchDigest;

    fn digest(&self) -> Self::TypedDigest {
        BatchDigest::new(
            crypto::DefaultHashFunction::digest_iterator(self.transactions.iter()).into(),
        )
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for BatchV2 {
    type TypedDigest = BatchDigest;

    fn digest(&self) -> Self::TypedDigest {
        BatchDigest::new(
            crypto::DefaultHashFunction::digest_iterator(self.transactions.iter()).into(),
        )
    }
}

// Messages generated internally by Narwhal that are included in headers for sequencing.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Deserialize, MallocSizeOf, Serialize)]
pub enum SystemMessage {
    // DKG is used to generate keys for use in the random beacon protocol.
    // `DkgMessage` is sent out at start-of-epoch to initiate the process.
    // Contents are a serialized `fastcrypto_tbls::dkg::Message`.
    DkgMessage(Vec<u8>),
    // `DkgConfirmation` is the second DKG message, sent as soon as a threshold amount of
    // `DkgMessages` have been received locally, to complete the key generation process.
    // Contents are a serialized `fastcrypto_tbls::dkg::Confirmation`.
    DkgConfirmation(Vec<u8>),
    // `RandomnessSignature` contains random bytes generated by the random beacon protocol
    // for the given round. Can be verified using the VSS public key derived from DKG.
    // Second item is a serialized `crypto::RandomnessSignature`.
    RandomnessSignature(RandomnessRound, Vec<u8>),
}

#[derive(Clone, Deserialize, MallocSizeOf, Serialize)]
#[enum_dispatch(HeaderAPI)]
pub enum Header {
    V1(HeaderV1),
    V2(HeaderV2),
}

// TODO: Revisit if we should not impl Default for Header and just use
// versioned header in Certificate
impl Default for Header {
    fn default() -> Self {
        Self::V1(HeaderV1::default())
    }
}

impl Header {
    pub fn digest(&self) -> HeaderDigest {
        match self {
            Header::V1(data) => data.digest(),
            Header::V2(data) => data.digest(),
        }
    }

    pub fn validate(&self, committee: &Committee, worker_cache: &WorkerCache) -> DagResult<()> {
        match self {
            Header::V1(data) => data.validate(committee, worker_cache),
            Header::V2(data) => data.validate(committee, worker_cache),
        }
    }

    pub fn unwrap_v2(self) -> HeaderV2 {
        match self {
            Header::V1(_) => panic!("called unwrap_v2 on Header::V1"),
            Header::V2(data) => data,
        }
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for Header {
    type TypedDigest = HeaderDigest;

    fn digest(&self) -> HeaderDigest {
        match self {
            Header::V1(data) => data.digest(),
            Header::V2(data) => data.digest(),
        }
    }
}

impl fmt::Debug for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}: B{}({}, E{}, {}B)",
            self.digest(),
            self.round(),
            self.author(),
            self.epoch(),
            self.payload()
                .keys()
                .map(|x| Digest::from(*x).size())
                .sum::<usize>(),
        )
    }
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::V1(data) => {
                write!(f, "B{}({})", data.round, data.author)
            }
            Self::V2(data) => {
                write!(f, "B{}({})", data.round, data.author)
            }
        }
    }
}

impl PartialEq for Header {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::V1(data) => data.digest() == other.digest(),
            Self::V2(data) => data.digest() == other.digest(),
        }
    }
}

#[enum_dispatch]
pub trait HeaderAPI {
    fn author(&self) -> AuthorityIdentifier;
    fn round(&self) -> Round;
    fn epoch(&self) -> Epoch;
    fn created_at(&self) -> &TimestampMs;
    fn payload(&self) -> &IndexMap<BatchDigest, (WorkerId, TimestampMs)>;
    fn system_messages(&self) -> &[SystemMessage];
    fn parents(&self) -> &BTreeSet<CertificateDigest>;

    // Used for testing.
    fn update_payload(&mut self, new_payload: IndexMap<BatchDigest, (WorkerId, TimestampMs)>);
    fn update_round(&mut self, new_round: Round);
    fn clear_parents(&mut self);
}

#[derive(Builder, Clone, Default, Deserialize, MallocSizeOf, Serialize)]
#[builder(pattern = "owned", build_fn(skip))]
pub struct HeaderV1 {
    // Primary that created the header. Must be the same primary that broadcasted the header.
    // Validation is at: https://github.com/MystenLabs/sui/blob/f0b80d9eeef44edd9fbe606cee16717622b68651/narwhal/primary/src/primary.rs#L713-L719
    pub author: AuthorityIdentifier,
    pub round: Round,
    pub epoch: Epoch,
    pub created_at: TimestampMs,
    #[serde(with = "indexmap::serde_seq")]
    pub payload: IndexMap<BatchDigest, (WorkerId, TimestampMs)>,
    pub parents: BTreeSet<CertificateDigest>,
    #[serde(skip)]
    digest: OnceCell<HeaderDigest>,
}

impl HeaderAPI for HeaderV1 {
    fn author(&self) -> AuthorityIdentifier {
        self.author
    }
    fn round(&self) -> Round {
        self.round
    }
    fn epoch(&self) -> Epoch {
        self.epoch
    }
    fn created_at(&self) -> &TimestampMs {
        &self.created_at
    }
    fn payload(&self) -> &IndexMap<BatchDigest, (WorkerId, TimestampMs)> {
        &self.payload
    }
    fn system_messages(&self) -> &[SystemMessage] {
        static EMPTY_SYSTEM_MESSAGES: Lazy<Vec<SystemMessage>> =
            Lazy::new(|| Vec::with_capacity(0));
        &EMPTY_SYSTEM_MESSAGES
    }
    fn parents(&self) -> &BTreeSet<CertificateDigest> {
        &self.parents
    }

    // Used for testing.
    fn update_payload(&mut self, new_payload: IndexMap<BatchDigest, (WorkerId, TimestampMs)>) {
        self.payload = new_payload;
    }
    fn update_round(&mut self, new_round: Round) {
        self.round = new_round;
    }
    fn clear_parents(&mut self) {
        self.parents.clear();
    }
}

impl HeaderV1Builder {
    pub fn build(self) -> Result<HeaderV1, fastcrypto::error::FastCryptoError> {
        let h = HeaderV1 {
            author: self.author.unwrap(),
            round: self.round.unwrap(),
            epoch: self.epoch.unwrap(),
            created_at: self.created_at.unwrap_or(0),
            payload: self.payload.unwrap(),
            parents: self.parents.unwrap(),
            digest: OnceCell::default(),
        };
        h.digest.set(Hash::digest(&h)).unwrap();

        Ok(h)
    }

    // helper method to set directly values to the payload
    pub fn with_payload_batch(
        mut self,
        batch: Batch,
        worker_id: WorkerId,
        created_at: TimestampMs,
    ) -> Self {
        if self.payload.is_none() {
            self.payload = Some(Default::default());
        }
        let payload = self.payload.as_mut().unwrap();

        payload.insert(batch.digest(), (worker_id, created_at));

        self
    }
}

impl HeaderV1 {
    pub async fn new(
        author: AuthorityIdentifier,
        round: Round,
        epoch: Epoch,
        payload: IndexMap<BatchDigest, (WorkerId, TimestampMs)>,
        parents: BTreeSet<CertificateDigest>,
    ) -> Self {
        let header = Self {
            author,
            round,
            epoch,
            created_at: now(),
            payload,
            parents,
            digest: OnceCell::default(),
        };
        let digest = Hash::digest(&header);
        header.digest.set(digest).unwrap();
        header
    }

    pub fn digest(&self) -> HeaderDigest {
        *self.digest.get_or_init(|| Hash::digest(self))
    }

    pub fn validate(&self, committee: &Committee, worker_cache: &WorkerCache) -> DagResult<()> {
        // Ensure the header is from the correct epoch.
        ensure!(
            self.epoch == committee.epoch(),
            DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: self.epoch
            }
        );

        // Ensure the header digest is well formed.
        ensure!(
            Hash::digest(self) == self.digest(),
            DagError::InvalidHeaderDigest
        );

        // Ensure the authority has voting rights.
        let voting_rights = committee.stake_by_id(self.author);
        ensure!(
            voting_rights > 0,
            DagError::UnknownAuthority(self.author.to_string())
        );

        // Ensure all worker ids are correct.
        for (worker_id, _) in self.payload.values() {
            worker_cache
                .worker(
                    committee.authority(&self.author).unwrap().protocol_key(),
                    worker_id,
                )
                .map_err(|_| DagError::HeaderHasBadWorkerIds(self.digest()))?;
        }

        Ok(())
    }
}

#[derive(Builder, Clone, Default, Deserialize, MallocSizeOf, Serialize)]
#[builder(pattern = "owned", build_fn(skip))]
pub struct HeaderV2 {
    // Primary that created the header. Must be the same primary that broadcasted the header.
    // Validation is at: https://github.com/MystenLabs/sui/blob/f0b80d9eeef44edd9fbe606cee16717622b68651/narwhal/primary/src/primary.rs#L713-L719
    pub author: AuthorityIdentifier,
    pub round: Round,
    pub epoch: Epoch,
    pub created_at: TimestampMs,
    #[serde(with = "indexmap::serde_seq")]
    pub payload: IndexMap<BatchDigest, (WorkerId, TimestampMs)>,
    pub system_messages: Vec<SystemMessage>,
    pub parents: BTreeSet<CertificateDigest>,
    #[serde(skip)]
    digest: OnceCell<HeaderDigest>,
}

impl HeaderAPI for HeaderV2 {
    fn author(&self) -> AuthorityIdentifier {
        self.author
    }
    fn round(&self) -> Round {
        self.round
    }
    fn epoch(&self) -> Epoch {
        self.epoch
    }
    fn created_at(&self) -> &TimestampMs {
        &self.created_at
    }
    fn payload(&self) -> &IndexMap<BatchDigest, (WorkerId, TimestampMs)> {
        &self.payload
    }
    fn system_messages(&self) -> &[SystemMessage] {
        &self.system_messages
    }
    fn parents(&self) -> &BTreeSet<CertificateDigest> {
        &self.parents
    }

    // Used for testing.
    fn update_payload(&mut self, new_payload: IndexMap<BatchDigest, (WorkerId, TimestampMs)>) {
        self.payload = new_payload;
    }
    fn update_round(&mut self, new_round: Round) {
        self.round = new_round;
    }
    fn clear_parents(&mut self) {
        self.parents.clear();
    }
}

impl HeaderV2Builder {
    pub fn build(self) -> Result<HeaderV2, fastcrypto::error::FastCryptoError> {
        let h = HeaderV2 {
            author: self.author.unwrap(),
            round: self.round.unwrap(),
            epoch: self.epoch.unwrap(),
            created_at: self.created_at.unwrap_or(0),
            payload: self.payload.unwrap(),
            system_messages: self.system_messages.unwrap_or_default(),
            parents: self.parents.unwrap(),
            digest: OnceCell::default(),
        };
        h.digest.set(Hash::digest(&h)).unwrap();

        Ok(h)
    }

    // helper method to set directly values to the payload
    pub fn with_payload_batch(
        mut self,
        batch: Batch,
        worker_id: WorkerId,
        created_at: TimestampMs,
    ) -> Self {
        if self.payload.is_none() {
            self.payload = Some(Default::default());
        }
        let payload = self.payload.as_mut().unwrap();

        payload.insert(batch.digest(), (worker_id, created_at));

        self
    }
}

impl HeaderV2 {
    pub async fn new(
        author: AuthorityIdentifier,
        round: Round,
        epoch: Epoch,
        payload: IndexMap<BatchDigest, (WorkerId, TimestampMs)>,
        system_messages: Vec<SystemMessage>,
        parents: BTreeSet<CertificateDigest>,
    ) -> Self {
        let header = Self {
            author,
            round,
            epoch,
            created_at: now(),
            payload,
            system_messages,
            parents,
            digest: OnceCell::default(),
        };
        let digest = Hash::digest(&header);
        header.digest.set(digest).unwrap();
        header
    }

    pub fn digest(&self) -> HeaderDigest {
        *self.digest.get_or_init(|| Hash::digest(self))
    }

    pub fn validate(&self, committee: &Committee, worker_cache: &WorkerCache) -> DagResult<()> {
        // Ensure the header is from the correct epoch.
        ensure!(
            self.epoch == committee.epoch(),
            DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: self.epoch
            }
        );

        // Ensure the header digest is well formed.
        ensure!(
            Hash::digest(self) == self.digest(),
            DagError::InvalidHeaderDigest
        );

        // Ensure the authority has voting rights.
        let voting_rights = committee.stake_by_id(self.author);
        ensure!(
            voting_rights > 0,
            DagError::UnknownAuthority(self.author.to_string())
        );

        // Ensure all worker ids are correct.
        for (worker_id, _) in self.payload.values() {
            worker_cache
                .worker(
                    committee.authority(&self.author).unwrap().protocol_key(),
                    worker_id,
                )
                .map_err(|_| DagError::HeaderHasBadWorkerIds(self.digest()))?;
        }

        Ok(())
    }
}

#[derive(
    Clone,
    Copy,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    MallocSizeOf,
    Arbitrary,
)]
pub struct HeaderDigest([u8; crypto::DIGEST_LENGTH]);

impl From<HeaderDigest> for Digest<{ crypto::DIGEST_LENGTH }> {
    fn from(hd: HeaderDigest) -> Self {
        Digest::new(hd.0)
    }
}

impl AsRef<[u8]> for HeaderDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for HeaderDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
        )
    }
}

impl fmt::Display for HeaderDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
                .get(0..16)
                .ok_or(fmt::Error)?
        )
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for HeaderV1 {
    type TypedDigest = HeaderDigest;

    fn digest(&self) -> HeaderDigest {
        let mut hasher = crypto::DefaultHashFunction::new();
        hasher.update(bcs::to_bytes(&self).expect("Serialization should not fail"));
        HeaderDigest(hasher.finalize().into())
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for HeaderV2 {
    type TypedDigest = HeaderDigest;

    fn digest(&self) -> HeaderDigest {
        let mut hasher = crypto::DefaultHashFunction::new();
        hasher.update(bcs::to_bytes(&self).expect("Serialization should not fail"));
        HeaderDigest(hasher.finalize().into())
    }
}

/// A Vote on a Header is a claim by the voting authority that all payloads and the full history
/// of Certificates included in the Header are available.
#[derive(Clone, Serialize, Deserialize)]
#[enum_dispatch(VoteAPI)]
pub enum Vote {
    V1(VoteV1),
}

impl Vote {
    // TODO: Add version number and match on that
    pub async fn new(
        header: &Header,
        author: &AuthorityIdentifier,
        signature_service: &SignatureService<Signature, { crypto::INTENT_MESSAGE_LENGTH }>,
    ) -> Self {
        Vote::V1(VoteV1::new(header, author, signature_service).await)
    }

    pub fn new_with_signer<S>(header: &Header, author: &AuthorityIdentifier, signer: &S) -> Self
    where
        S: Signer<Signature>,
    {
        Vote::V1(VoteV1::new_with_signer(header, author, signer))
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for Vote {
    type TypedDigest = VoteDigest;

    fn digest(&self) -> VoteDigest {
        match self {
            Vote::V1(data) => data.digest(),
        }
    }
}

#[enum_dispatch]
pub trait VoteAPI {
    fn header_digest(&self) -> HeaderDigest;
    fn round(&self) -> Round;
    fn epoch(&self) -> Epoch;
    fn origin(&self) -> AuthorityIdentifier;
    fn author(&self) -> AuthorityIdentifier;
    fn signature(&self) -> &<PublicKey as VerifyingKey>::Sig;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VoteV1 {
    // HeaderDigest, round, epoch and origin for the header being voted on.
    pub header_digest: HeaderDigest,
    pub round: Round,
    pub epoch: Epoch,
    pub origin: AuthorityIdentifier,
    // Author of this vote.
    pub author: AuthorityIdentifier,
    // Signature of the HeaderDigest.
    pub signature: <PublicKey as VerifyingKey>::Sig,
}

impl VoteAPI for VoteV1 {
    fn header_digest(&self) -> HeaderDigest {
        self.header_digest
    }
    fn round(&self) -> Round {
        self.round
    }
    fn epoch(&self) -> Epoch {
        self.epoch
    }
    fn origin(&self) -> AuthorityIdentifier {
        self.origin
    }
    fn author(&self) -> AuthorityIdentifier {
        self.author
    }
    fn signature(&self) -> &<PublicKey as VerifyingKey>::Sig {
        &self.signature
    }
}

impl VoteV1 {
    pub async fn new(
        header: &Header,
        author: &AuthorityIdentifier,
        signature_service: &SignatureService<Signature, { crypto::INTENT_MESSAGE_LENGTH }>,
    ) -> Self {
        let vote = Self {
            header_digest: header.digest(),
            round: header.round(),
            epoch: header.epoch(),
            origin: header.author(),
            author: *author,
            signature: Signature::default(),
        };
        let signature = signature_service
            .request_signature(vote.digest().into())
            .await;
        Self { signature, ..vote }
    }

    pub fn new_with_signer<S>(header: &Header, author: &AuthorityIdentifier, signer: &S) -> Self
    where
        S: Signer<Signature>,
    {
        let vote = Self {
            header_digest: header.digest(),
            round: header.round(),
            epoch: header.epoch(),
            origin: header.author(),
            author: *author,
            signature: Signature::default(),
        };

        let vote_digest: Digest<{ crypto::DIGEST_LENGTH }> = vote.digest().into();
        let signature = Signature::new_secure(&to_intent_message(vote_digest), signer);

        Self { signature, ..vote }
    }
}
#[derive(
    Clone, Serialize, Deserialize, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Copy, Arbitrary,
)]
pub struct VoteDigest([u8; crypto::DIGEST_LENGTH]);

impl From<VoteDigest> for Digest<{ crypto::DIGEST_LENGTH }> {
    fn from(hd: VoteDigest) -> Self {
        Digest::new(hd.0)
    }
}

impl From<VoteDigest> for Digest<{ crypto::INTENT_MESSAGE_LENGTH }> {
    fn from(digest: VoteDigest) -> Self {
        let intent_message = to_intent_message(HeaderDigest(digest.0));
        Digest {
            digest: bcs::to_bytes(&intent_message)
                .expect("Serialization message should not fail")
                .try_into()
                .expect("INTENT_MESSAGE_LENGTH is correct"),
        }
    }
}

impl fmt::Debug for VoteDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
        )
    }
}

impl fmt::Display for VoteDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
                .get(0..16)
                .ok_or(fmt::Error)?
        )
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for VoteV1 {
    type TypedDigest = VoteDigest;

    fn digest(&self) -> VoteDigest {
        VoteDigest(self.header_digest().0)
    }
}

impl fmt::Debug for Vote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}: V{}({}, {}, E{})",
            self.digest(),
            self.round(),
            self.author(),
            self.origin(),
            self.epoch()
        )
    }
}

impl PartialEq for Vote {
    fn eq(&self, other: &Self) -> bool {
        self.digest() == other.digest()
    }
}

#[derive(Clone, Serialize, Deserialize, MallocSizeOf)]
#[enum_dispatch(CertificateAPI)]
pub enum Certificate {
    V1(CertificateV1),
    V2(CertificateV2),
}

impl Certificate {
    pub fn genesis(protocol_config: &ProtocolConfig, committee: &Committee) -> Vec<Self> {
        if protocol_config.narwhal_certificate_v2() {
            CertificateV2::genesis(committee, protocol_config.narwhal_header_v2())
                .into_iter()
                .map(Self::V2)
                .collect()
        } else {
            CertificateV1::genesis(committee, protocol_config.narwhal_header_v2())
                .into_iter()
                .map(Self::V1)
                .collect()
        }
    }

    pub fn new_unverified(
        protocol_config: &ProtocolConfig,
        committee: &Committee,
        header: Header,
        votes: Vec<(AuthorityIdentifier, Signature)>,
    ) -> DagResult<Certificate> {
        if protocol_config.narwhal_certificate_v2() {
            CertificateV2::new_unverified(committee, header, votes)
        } else {
            CertificateV1::new_unverified(committee, header, votes)
        }
    }

    pub fn new_unsigned(
        protocol_config: &ProtocolConfig,
        committee: &Committee,
        header: Header,
        votes: Vec<(AuthorityIdentifier, Signature)>,
    ) -> DagResult<Certificate> {
        if protocol_config.narwhal_certificate_v2() {
            CertificateV2::new_unsigned(committee, header, votes)
        } else {
            CertificateV1::new_unsigned(committee, header, votes)
        }
    }

    /// This function requires that certificate was verified against given committee
    pub fn signed_authorities(&self, committee: &Committee) -> Vec<PublicKey> {
        match self {
            Certificate::V1(certificate) => certificate.signed_authorities(committee),
            Certificate::V2(certificate) => certificate.signed_authorities(committee),
        }
    }

    pub fn signed_by(&self, committee: &Committee) -> (Stake, Vec<PublicKey>) {
        match self {
            Certificate::V1(certificate) => certificate.signed_by(committee),
            Certificate::V2(certificate) => certificate.signed_by(committee),
        }
    }

    pub fn verify(
        self,
        committee: &Committee,
        worker_cache: &WorkerCache,
    ) -> DagResult<Certificate> {
        match self {
            Certificate::V1(certificate) => certificate.verify(committee, worker_cache),
            Certificate::V2(certificate) => certificate.verify(committee, worker_cache),
        }
    }

    pub fn round(&self) -> Round {
        match self {
            Certificate::V1(certificate) => certificate.round(),
            Certificate::V2(certificate) => certificate.round(),
        }
    }

    pub fn epoch(&self) -> Epoch {
        match self {
            Certificate::V1(certificate) => certificate.epoch(),
            Certificate::V2(certificate) => certificate.epoch(),
        }
    }

    pub fn origin(&self) -> AuthorityIdentifier {
        match self {
            Certificate::V1(certificate) => certificate.origin(),
            Certificate::V2(certificate) => certificate.origin(),
        }
    }

    // Used for testing
    pub fn default(protocol_config: &ProtocolConfig) -> Certificate {
        if protocol_config.narwhal_certificate_v2() {
            Certificate::V2(CertificateV2::default())
        } else {
            Certificate::V1(CertificateV1::default())
        }
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for Certificate {
    type TypedDigest = CertificateDigest;

    fn digest(&self) -> CertificateDigest {
        match self {
            Certificate::V1(data) => data.digest(),
            Certificate::V2(data) => data.digest(),
        }
    }
}

#[enum_dispatch]
pub trait CertificateAPI {
    fn header(&self) -> &Header;
    fn aggregated_signature(&self) -> Option<&AggregateSignatureBytes>;
    fn signed_authorities(&self) -> &roaring::RoaringBitmap;
    fn metadata(&self) -> &Metadata;

    // Used for testing.
    fn update_header(&mut self, header: Header);
    fn header_mut(&mut self) -> &mut Header;

    // CertificateV2
    fn signature_verification_state(&self) -> &SignatureVerificationState;
    fn set_signature_verification_state(&mut self, state: SignatureVerificationState);
}

#[serde_as]
#[derive(Clone, Serialize, Deserialize, Default, MallocSizeOf)]
pub struct CertificateV1 {
    pub header: Header,
    pub aggregated_signature: AggregateSignatureBytes,
    #[serde_as(as = "NarwhalBitmap")]
    signed_authorities: roaring::RoaringBitmap,
    pub metadata: Metadata,
}

impl CertificateAPI for CertificateV1 {
    fn header(&self) -> &Header {
        &self.header
    }

    fn aggregated_signature(&self) -> Option<&AggregateSignatureBytes> {
        Some(&self.aggregated_signature)
    }

    fn signature_verification_state(&self) -> &SignatureVerificationState {
        unimplemented!("CertificateV2 field! Use aggregated_signature.");
    }

    fn set_signature_verification_state(&mut self, _state: SignatureVerificationState) {
        unimplemented!("CertificateV2 field! Use aggregated_signature.");
    }

    fn signed_authorities(&self) -> &roaring::RoaringBitmap {
        &self.signed_authorities
    }

    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    // Used for testing.
    fn update_header(&mut self, header: Header) {
        self.header = header;
    }

    fn header_mut(&mut self) -> &mut Header {
        &mut self.header
    }
}

impl CertificateV1 {
    pub fn genesis(committee: &Committee, use_header_v2: bool) -> Vec<Self> {
        committee
            .authorities()
            .map(|authority| Self {
                header: if use_header_v2 {
                    Header::V2(HeaderV2 {
                        author: authority.id(),
                        epoch: committee.epoch(),
                        ..Default::default()
                    })
                } else {
                    Header::V1(HeaderV1 {
                        author: authority.id(),
                        epoch: committee.epoch(),
                        ..Default::default()
                    })
                },
                ..Self::default()
            })
            .collect()
    }

    pub fn new_unverified(
        committee: &Committee,
        header: Header,
        votes: Vec<(AuthorityIdentifier, Signature)>,
    ) -> DagResult<Certificate> {
        Self::new_unsafe(committee, header, votes, true)
    }

    pub fn new_unsigned(
        committee: &Committee,
        header: Header,
        votes: Vec<(AuthorityIdentifier, Signature)>,
    ) -> DagResult<Certificate> {
        Self::new_unsafe(committee, header, votes, false)
    }

    fn new_unsafe(
        committee: &Committee,
        header: Header,
        votes: Vec<(AuthorityIdentifier, Signature)>,
        check_stake: bool,
    ) -> DagResult<Certificate> {
        let mut votes = votes;
        votes.sort_by_key(|(pk, _)| *pk);
        let mut votes: VecDeque<_> = votes.into_iter().collect();

        let mut weight = 0;
        let mut sigs = Vec::new();

        let filtered_votes = committee
            .authorities()
            .enumerate()
            .filter(|(_, authority)| {
                if !votes.is_empty() && authority.id() == votes.front().unwrap().0 {
                    sigs.push(votes.pop_front().unwrap());
                    weight += authority.stake();
                    // If there are repeats, also remove them
                    while !votes.is_empty() && votes.front().unwrap() == sigs.last().unwrap() {
                        votes.pop_front().unwrap();
                    }
                    return true;
                }
                false
            })
            .map(|(index, _)| index as u32);

        let signed_authorities= roaring::RoaringBitmap::from_sorted_iter(filtered_votes)
            .map_err(|_| DagError::InvalidBitmap("Failed to convert votes into a bitmap of authority keys. Something is likely very wrong...".to_string()))?;

        // Ensure that all authorities in the set of votes are known
        ensure!(
            votes.is_empty(),
            DagError::UnknownAuthority(votes.front().unwrap().0.to_string())
        );

        // Ensure that the authorities have enough weight
        ensure!(
            !check_stake || weight >= committee.quorum_threshold(),
            DagError::CertificateRequiresQuorum
        );

        let aggregated_signature = if sigs.is_empty() {
            AggregateSignature::default()
        } else {
            AggregateSignature::aggregate::<Signature, Vec<&Signature>>(
                sigs.iter().map(|(_, sig)| sig).collect(),
            )
            .map_err(|_| DagError::InvalidSignature)?
        };

        Ok(Certificate::V1(CertificateV1 {
            header,
            aggregated_signature: AggregateSignatureBytes::from(&aggregated_signature),
            signed_authorities,
            metadata: Metadata::default(),
        }))
    }

    /// This function requires that certificate was verified against given committee
    pub fn signed_authorities(&self, committee: &Committee) -> Vec<PublicKey> {
        assert_eq!(committee.epoch(), self.epoch());
        let (_stake, pks) = self.signed_by(committee);
        pks
    }

    pub fn signed_by(&self, committee: &Committee) -> (Stake, Vec<PublicKey>) {
        // Ensure the certificate has a quorum.
        let mut weight = 0;

        let auth_indexes = self.signed_authorities.iter().collect::<Vec<_>>();
        let mut auth_iter = 0;
        let pks = committee
            .authorities()
            .enumerate()
            .filter(|(i, authority)| match auth_indexes.get(auth_iter) {
                Some(index) if *index == *i as u32 => {
                    weight += authority.stake();
                    auth_iter += 1;
                    true
                }
                _ => false,
            })
            .map(|(_, authority)| authority.protocol_key().clone())
            .collect();
        (weight, pks)
    }

    /// Verifies the validity of the certificate.
    pub fn verify(
        self,
        committee: &Committee,
        worker_cache: &WorkerCache,
    ) -> DagResult<Certificate> {
        // Ensure the header is from the correct epoch.
        ensure!(
            self.epoch() == committee.epoch(),
            DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: self.epoch()
            }
        );

        // Genesis certificates are always valid.
        let use_header_v2 = matches!(self.header, Header::V2(_));
        if self.round() == 0 && Self::genesis(committee, use_header_v2).contains(&self) {
            return Ok(Certificate::V1(self));
        }

        // Save signature verifications when the header is invalid.
        self.header.validate(committee, worker_cache)?;

        let (weight, pks) = self.signed_by(committee);

        ensure!(
            weight >= committee.quorum_threshold(),
            DagError::CertificateRequiresQuorum
        );

        // Verify the signatures
        let certificate_digest: Digest<{ crypto::DIGEST_LENGTH }> = Digest::from(self.digest());
        AggregateSignature::try_from(&self.aggregated_signature)
            .map_err(|_| DagError::InvalidSignature)?
            .verify_secure(&to_intent_message(certificate_digest), &pks[..])
            .map_err(|_| DagError::InvalidSignature)?;

        Ok(Certificate::V1(self))
    }

    pub fn round(&self) -> Round {
        self.header.round()
    }

    pub fn epoch(&self) -> Epoch {
        self.header.epoch()
    }

    pub fn origin(&self) -> AuthorityIdentifier {
        self.header.author()
    }
}

// Holds AggregateSignatureBytes but with the added layer to specify the
// signatures verification state. This will be used to take advantage of the
// certificate chain that is formed via the DAG by only verifying the
// leaves of the certificate chain when they are fetched from validators
// during catchup.
#[derive(Clone, Serialize, Deserialize, MallocSizeOf, Debug)]
pub enum SignatureVerificationState {
    // This state occurs when the certificate has not yet received a quorum of
    // signatures.
    Unsigned(AggregateSignatureBytes),
    // This state occurs when a certificate has just been received from the network
    // and has not been verified yet.
    Unverified(AggregateSignatureBytes),
    // This state occurs when a certificate was either created locally, received
    // via brodacast, or fetched but was not the parent of another certificate.
    // Therefore this certificate had to be verified directly.
    VerifiedDirectly(AggregateSignatureBytes),
    // This state occurs when the cert was a parent of another fetched certificate
    // that was verified directly, then this certificate is verified indirectly.
    VerifiedIndirectly(AggregateSignatureBytes),
    // This state occurs only for genesis certificates which always has valid
    // signatures bytes but the bytes are garbage so we don't mark them as verified.
    Genesis,
}

impl Default for SignatureVerificationState {
    fn default() -> Self {
        SignatureVerificationState::Unsigned(AggregateSignatureBytes::default())
    }
}

#[serde_as]
#[derive(Clone, Serialize, Deserialize, Default, MallocSizeOf)]
pub struct CertificateV2 {
    pub header: Header,
    pub signature_verification_state: SignatureVerificationState,
    #[serde_as(as = "NarwhalBitmap")]
    signed_authorities: roaring::RoaringBitmap,
    pub metadata: Metadata,
}

impl CertificateAPI for CertificateV2 {
    fn header(&self) -> &Header {
        &self.header
    }

    fn aggregated_signature(&self) -> Option<&AggregateSignatureBytes> {
        match &self.signature_verification_state {
            SignatureVerificationState::VerifiedDirectly(bytes)
            | SignatureVerificationState::Unverified(bytes)
            | SignatureVerificationState::VerifiedIndirectly(bytes)
            | SignatureVerificationState::Unsigned(bytes) => Some(bytes),
            SignatureVerificationState::Genesis => None,
        }
    }

    fn signature_verification_state(&self) -> &SignatureVerificationState {
        &self.signature_verification_state
    }

    fn set_signature_verification_state(&mut self, state: SignatureVerificationState) {
        self.signature_verification_state = state;
    }

    fn signed_authorities(&self) -> &roaring::RoaringBitmap {
        &self.signed_authorities
    }

    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    // Used for testing.
    fn update_header(&mut self, header: Header) {
        self.header = header;
    }

    fn header_mut(&mut self) -> &mut Header {
        &mut self.header
    }
}

impl CertificateV2 {
    pub fn genesis(committee: &Committee, use_header_v2: bool) -> Vec<Self> {
        committee
            .authorities()
            .map(|authority| Self {
                header: if use_header_v2 {
                    Header::V2(HeaderV2 {
                        author: authority.id(),
                        epoch: committee.epoch(),
                        ..Default::default()
                    })
                } else {
                    Header::V1(HeaderV1 {
                        author: authority.id(),
                        epoch: committee.epoch(),
                        ..Default::default()
                    })
                },
                signature_verification_state: SignatureVerificationState::Genesis,
                ..Self::default()
            })
            .collect()
    }

    pub fn new_unverified(
        committee: &Committee,
        header: Header,
        votes: Vec<(AuthorityIdentifier, Signature)>,
    ) -> DagResult<Certificate> {
        Self::new_unsafe(committee, header, votes, true)
    }

    pub fn new_unsigned(
        committee: &Committee,
        header: Header,
        votes: Vec<(AuthorityIdentifier, Signature)>,
    ) -> DagResult<Certificate> {
        Self::new_unsafe(committee, header, votes, false)
    }

    fn new_unsafe(
        committee: &Committee,
        header: Header,
        votes: Vec<(AuthorityIdentifier, Signature)>,
        check_stake: bool,
    ) -> DagResult<Certificate> {
        let mut votes = votes;
        votes.sort_by_key(|(pk, _)| *pk);
        let mut votes: VecDeque<_> = votes.into_iter().collect();

        let mut weight = 0;
        let mut sigs = Vec::new();

        let filtered_votes = committee
            .authorities()
            .enumerate()
            .filter(|(_, authority)| {
                if !votes.is_empty() && authority.id() == votes.front().unwrap().0 {
                    sigs.push(votes.pop_front().unwrap());
                    weight += authority.stake();
                    // If there are repeats, also remove them
                    while !votes.is_empty() && votes.front().unwrap() == sigs.last().unwrap() {
                        votes.pop_front().unwrap();
                    }
                    return true;
                }
                false
            })
            .map(|(index, _)| index as u32);

        let signed_authorities= roaring::RoaringBitmap::from_sorted_iter(filtered_votes)
            .map_err(|_| DagError::InvalidBitmap("Failed to convert votes into a bitmap of authority keys. Something is likely very wrong...".to_string()))?;

        // Ensure that all authorities in the set of votes are known
        ensure!(
            votes.is_empty(),
            DagError::UnknownAuthority(votes.front().unwrap().0.to_string())
        );

        // Ensure that the authorities have enough weight
        ensure!(
            !check_stake || weight >= committee.quorum_threshold(),
            DagError::CertificateRequiresQuorum
        );

        let aggregated_signature = if sigs.is_empty() {
            AggregateSignature::default()
        } else {
            AggregateSignature::aggregate::<Signature, Vec<&Signature>>(
                sigs.iter().map(|(_, sig)| sig).collect(),
            )
            .map_err(|_| DagError::InvalidSignature)?
        };

        let aggregate_signature_bytes = AggregateSignatureBytes::from(&aggregated_signature);

        let signature_verification_state = if !check_stake {
            SignatureVerificationState::Unsigned(aggregate_signature_bytes)
        } else {
            SignatureVerificationState::Unverified(aggregate_signature_bytes)
        };

        Ok(Certificate::V2(CertificateV2 {
            header,
            signature_verification_state,
            signed_authorities,
            metadata: Metadata::default(),
        }))
    }

    /// This function requires that certificate was verified against given committee
    pub fn signed_authorities(&self, committee: &Committee) -> Vec<PublicKey> {
        assert_eq!(committee.epoch(), self.epoch());
        let (_stake, pks) = self.signed_by(committee);
        pks
    }

    pub fn signed_by(&self, committee: &Committee) -> (Stake, Vec<PublicKey>) {
        // Ensure the certificate has a quorum.
        let mut weight = 0;

        let auth_indexes = self.signed_authorities.iter().collect::<Vec<_>>();
        let mut auth_iter = 0;
        let pks = committee
            .authorities()
            .enumerate()
            .filter(|(i, authority)| match auth_indexes.get(auth_iter) {
                Some(index) if *index == *i as u32 => {
                    weight += authority.stake();
                    auth_iter += 1;
                    true
                }
                _ => false,
            })
            .map(|(_, authority)| authority.protocol_key().clone())
            .collect();
        (weight, pks)
    }

    /// Verifies the validity of the certificate.
    pub fn verify(
        self,
        committee: &Committee,
        worker_cache: &WorkerCache,
    ) -> DagResult<Certificate> {
        // Ensure the header is from the correct epoch.
        ensure!(
            self.epoch() == committee.epoch(),
            DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: self.epoch()
            }
        );

        // Genesis certificates are always valid.
        let use_header_v2 = matches!(self.header, Header::V2(_));
        if self.round() == 0 && Self::genesis(committee, use_header_v2).contains(&self) {
            return Ok(Certificate::V2(self));
        }

        // Save signature verifications when the header is invalid.
        self.header.validate(committee, worker_cache)?;

        let (weight, pks) = self.signed_by(committee);

        ensure!(
            weight >= committee.quorum_threshold(),
            DagError::CertificateRequiresQuorum
        );

        let verified_cert = self.verify_signature(pks)?;

        Ok(verified_cert)
    }

    fn verify_signature(mut self, pks: Vec<PublicKey>) -> DagResult<Certificate> {
        let aggregrate_signature_bytes = match self.signature_verification_state {
            SignatureVerificationState::VerifiedIndirectly(_)
            | SignatureVerificationState::VerifiedDirectly(_)
            | SignatureVerificationState::Genesis => return Ok(Certificate::V2(self)),
            SignatureVerificationState::Unverified(ref bytes) => bytes,
            SignatureVerificationState::Unsigned(_) => {
                bail!(DagError::CertificateRequiresQuorum);
            }
        };

        // Verify the signatures
        let certificate_digest: Digest<{ crypto::DIGEST_LENGTH }> = Digest::from(self.digest());
        AggregateSignature::try_from(aggregrate_signature_bytes)
            .map_err(|_| DagError::InvalidSignature)?
            .verify_secure(&to_intent_message(certificate_digest), &pks[..])
            .map_err(|_| DagError::InvalidSignature)?;

        self.signature_verification_state =
            SignatureVerificationState::VerifiedDirectly(aggregrate_signature_bytes.clone());

        Ok(Certificate::V2(self))
    }

    pub fn round(&self) -> Round {
        self.header.round()
    }

    pub fn epoch(&self) -> Epoch {
        self.header.epoch()
    }

    pub fn origin(&self) -> AuthorityIdentifier {
        self.header.author()
    }
}

// Certificate version is validated against network protocol version. If CertificateV2
// is being used then the cert will also be marked as Unverifed as this certificate
// is assumed to be received from the network. This SignatureVerificationState is
// why the modified certificate is being returned.
pub fn validate_received_certificate_version(
    mut certificate: Certificate,
    protocol_config: &ProtocolConfig,
) -> anyhow::Result<Certificate> {
    // If network has advanced to using version 28, which sets narwhal_certificate_v2
    // to true, we will start using CertificateV2 locally and so we will only accept
    // CertificateV2 from the network. Otherwise CertificateV1 is used.
    match certificate {
        Certificate::V1(_) => {
            // CertificateV1 does not have a concept of aggregated signature state
            // so there is nothing to reset.
            if protocol_config.narwhal_certificate_v2() {
                return Err(anyhow::anyhow!(format!(
                    "Received CertificateV1 {certificate:?} but network is at {:?} and this certificate version is no longer supported",
                    protocol_config.version
                )));
            }
        }
        Certificate::V2(_) => {
            if !protocol_config.narwhal_certificate_v2() {
                return Err(anyhow::anyhow!(format!(
                    "Received CertificateV2 {certificate:?} but network is at {:?} and this certificate version is not supported yet",
                    protocol_config.version
                )));
            } else {
                // CertificateV2 was received from the network so we need to mark
                // certificate aggregated signature state as unverified.
                certificate.set_signature_verification_state(
                    SignatureVerificationState::Unverified(
                        certificate
                            .aggregated_signature()
                            .ok_or(anyhow::anyhow!("Invalid signature"))?
                            .clone(),
                    ),
                );
            }
        }
    };
    Ok(certificate)
}

#[derive(
    Clone,
    Copy,
    Serialize,
    Deserialize,
    Default,
    MallocSizeOf,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Arbitrary,
)]

pub struct CertificateDigest([u8; crypto::DIGEST_LENGTH]);

impl CertificateDigest {
    pub fn new(digest: [u8; crypto::DIGEST_LENGTH]) -> CertificateDigest {
        CertificateDigest(digest)
    }
}

impl AsRef<[u8]> for CertificateDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<CertificateDigest> for Digest<{ crypto::DIGEST_LENGTH }> {
    fn from(hd: CertificateDigest) -> Self {
        Digest::new(hd.0)
    }
}

impl fmt::Debug for CertificateDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
        )
    }
}

impl fmt::Display for CertificateDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
                .get(0..16)
                .ok_or(fmt::Error)?
        )
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for CertificateV1 {
    type TypedDigest = CertificateDigest;

    fn digest(&self) -> CertificateDigest {
        CertificateDigest(self.header.digest().0)
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for CertificateV2 {
    type TypedDigest = CertificateDigest;

    fn digest(&self) -> CertificateDigest {
        CertificateDigest(self.header.digest().0)
    }
}

impl fmt::Debug for Certificate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Certificate::V1(data) => write!(
                f,
                "{}: C{}({}, {}, E{})",
                data.digest(),
                data.round(),
                data.origin(),
                data.header.digest(),
                data.epoch()
            ),
            Certificate::V2(data) => write!(
                f,
                "{}: C{}({}, {}, E{})",
                data.digest(),
                data.round(),
                data.origin(),
                data.header.digest(),
                data.epoch()
            ),
        }
    }
}

impl PartialEq for Certificate {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Certificate::V1(data), Certificate::V1(other_data)) => data.eq(other_data),
            (Certificate::V2(data), Certificate::V2(other_data)) => data.eq(other_data),
            (Certificate::V1(_), Certificate::V2(_)) => {
                unimplemented!("Invalid comparison between CertificateV1 & CertificateV2");
            }
            (Certificate::V2(_), Certificate::V1(_)) => {
                unimplemented!("Invalid comparison between CertificateV2 & CertificateV1");
            }
        }
    }
}

impl PartialEq for CertificateV1 {
    fn eq(&self, other: &Self) -> bool {
        let mut ret = self.header().digest() == other.header().digest();
        ret &= self.round() == other.round();
        ret &= self.epoch() == other.epoch();
        ret &= self.origin() == other.origin();
        ret
    }
}

impl PartialEq for CertificateV2 {
    fn eq(&self, other: &Self) -> bool {
        let mut ret = self.header().digest() == other.header().digest();
        ret &= self.round() == other.round();
        ret &= self.epoch() == other.epoch();
        ret &= self.origin() == other.origin();
        ret
    }
}

/// Request for broadcasting certificates to peers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendCertificateRequest {
    pub certificate: Certificate,
}

/// Response from peers after receiving a certificate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendCertificateResponse {
    pub accepted: bool,
}

/// Used by the primary to request a vote from other primaries on newly produced headers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestVoteRequest {
    pub header: Header,

    // Optional parent certificates provided by the requester, in case this primary doesn't yet
    // have them and requires them in order to offer a vote.
    pub parents: Vec<Certificate>,
}

/// Used by the primary to reply to RequestVoteRequest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestVoteResponse {
    pub vote: Option<Vote>,

    // Indicates digests of missing certificates without which a vote cannot be provided.
    pub missing: Vec<CertificateDigest>,
}

/// Request for sending random beacon signatures to a peer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendRandomnessPartialSignaturesRequest {
    pub round: RandomnessRound,
    pub sigs: Vec<RandomnessPartialSignature>,
}

/// Used by the primary to fetch certificates from other primaries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchCertificatesRequest {
    /// The exclusive lower bound is a round number where each primary should return certificates above that.
    /// This corresponds to the GC round at the requestor.
    pub exclusive_lower_bound: Round,
    /// This contains per authority serialized RoaringBitmap for the round diffs between
    /// - rounds of certificates to be skipped from the response and
    /// - the GC round.
    /// These rounds are skipped because the requestor already has them.
    pub skip_rounds: Vec<(AuthorityIdentifier, Vec<u8>)>,
    /// Maximum number of certificates that should be returned.
    pub max_items: usize,
}

impl FetchCertificatesRequest {
    #[allow(clippy::mutable_key_type)]
    pub fn get_bounds(&self) -> (Round, BTreeMap<AuthorityIdentifier, BTreeSet<Round>>) {
        let skip_rounds: BTreeMap<AuthorityIdentifier, BTreeSet<Round>> = self
            .skip_rounds
            .iter()
            .filter_map(|(k, serialized)| {
                match RoaringBitmap::deserialize_from(&mut &serialized[..]) {
                    Ok(bitmap) => {
                        let rounds: BTreeSet<Round> = bitmap
                            .into_iter()
                            .map(|r| self.exclusive_lower_bound + r as Round)
                            .collect();
                        Some((*k, rounds))
                    }
                    Err(e) => {
                        warn!("Failed to deserialize RoaringBitmap {e}");
                        None
                    }
                }
            })
            .collect();
        (self.exclusive_lower_bound, skip_rounds)
    }

    #[allow(clippy::mutable_key_type)]
    pub fn set_bounds(
        mut self,
        gc_round: Round,
        skip_rounds: BTreeMap<AuthorityIdentifier, BTreeSet<Round>>,
    ) -> Self {
        self.exclusive_lower_bound = gc_round;
        self.skip_rounds = skip_rounds
            .into_iter()
            .map(|(k, rounds)| {
                let mut serialized = Vec::new();
                rounds
                    .into_iter()
                    .map(|v| u32::try_from(v - gc_round).unwrap())
                    .collect::<RoaringBitmap>()
                    .serialize_into(&mut serialized)
                    .unwrap();
                (k, serialized)
            })
            .collect();
        self
    }

    pub fn set_max_items(mut self, max_items: usize) -> Self {
        self.max_items = max_items;
        self
    }
}

/// Used by the primary to reply to FetchCertificatesRequest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FetchCertificatesResponse {
    /// Certificates sorted from lower to higher rounds.
    pub certificates: Vec<Certificate>,
}

/// Used by the primary to request that the worker sync the target missing batches.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerSynchronizeMessage {
    pub digests: Vec<BatchDigest>,
    pub target: AuthorityIdentifier,
    // Used to indicate to the worker that it does not need to fully validate
    // the batch it receives because it is part of a certificate. Only digest
    // verification is required.
    pub is_certified: bool,
}

/// Used by the primary to request that the worker fetch the missing batches and reply
/// with all of the content.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchBatchesRequest {
    pub digests: HashSet<BatchDigest>,
    pub known_workers: HashSet<NetworkPublicKey>,
}

/// All batches requested by the primary.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchBatchesResponse {
    pub batches: HashMap<BatchDigest, Batch>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchMessage {
    // TODO: revisit including the digest here [see #188]
    pub digest: BatchDigest,
    pub batch: Batch,
}

/// Used by worker to inform primary it sealed a new batch.
#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct WorkerOwnBatchMessage {
    pub digest: BatchDigest,
    pub worker_id: WorkerId,
    pub metadata: VersionedMetadata,
}

/// Used by worker to inform primary it received a batch from another authority.
#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct WorkerOthersBatchMessage {
    pub digest: BatchDigest,
    pub worker_id: WorkerId,
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
#[enum_dispatch(VoteInfoAPI)]
pub enum VoteInfo {
    V1(VoteInfoV1),
}

#[enum_dispatch]
pub trait VoteInfoAPI {
    fn epoch(&self) -> Epoch;
    fn round(&self) -> Round;
    fn vote_digest(&self) -> VoteDigest;
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct VoteInfoV1 {
    /// The latest Epoch for which a vote was sent to given authority
    pub epoch: Epoch,
    /// The latest round for which a vote was sent to given authority
    pub round: Round,
    /// The hash of the vote used to ensure equality
    pub vote_digest: VoteDigest,
}

impl VoteInfoAPI for VoteInfoV1 {
    fn epoch(&self) -> Epoch {
        self.epoch
    }

    fn round(&self) -> Round {
        self.round
    }

    fn vote_digest(&self) -> VoteDigest {
        self.vote_digest
    }
}

impl From<&VoteV1> for VoteInfoV1 {
    fn from(vote: &VoteV1) -> Self {
        VoteInfoV1 {
            epoch: vote.epoch(),
            round: vote.round(),
            vote_digest: vote.digest(),
        }
    }
}

impl From<&Vote> for VoteInfo {
    fn from(vote: &Vote) -> Self {
        match vote {
            Vote::V1(vote) => VoteInfo::V1(VoteInfoV1::from(vote)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Batch, BatchAPI, BatchV2, MetadataAPI, MetadataV1, Timestamp, VersionedMetadata};
    use std::time::Duration;
    use test_utils::latest_protocol_version;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_elapsed() {
        let batch = Batch::new(vec![], &latest_protocol_version());

        assert!(*batch.versioned_metadata().created_at() > 0);

        assert!(batch.versioned_metadata().received_at().is_none());

        sleep(Duration::from_secs(2)).await;

        assert!(
            batch
                .versioned_metadata()
                .created_at()
                .elapsed()
                .as_secs_f64()
                >= 2.0
        );
    }

    #[test]
    fn test_elapsed_when_newer_than_now() {
        let batch = Batch::V2(BatchV2 {
            transactions: vec![],
            versioned_metadata: VersionedMetadata::V1(MetadataV1 {
                created_at: 2999309726980, // something in the future - Fri Jan 16 2065 05:35:26
                received_at: None,
            }),
        });

        assert_eq!(
            batch
                .versioned_metadata()
                .created_at()
                .elapsed()
                .as_secs_f64(),
            0.0
        );
    }
}
