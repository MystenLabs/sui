// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    error::{DagError, DagResult},
    serde::NarwhalBitmap,
    CertificateDigestProto,
};
use bytes::Bytes;
use config::{Committee, Epoch, Stake, WorkerCache, WorkerId, WorkerInfo};
use crypto::{
    to_intent_message, AggregateSignature, NarwhalAuthorityAggregateSignature,
    NarwhalAuthoritySignature, PublicKey, PublicKeyBytes, Signature,
};
use dag::node_dag::Affiliated;
use derive_builder::Builder;
use fastcrypto::{
    hash::{Digest, Hash, HashFunction},
    signature_service::SignatureService,
    traits::{AggregateAuthenticator, EncodeDecodeBase64, InsecureDefault, Signer, VerifyingKey},
};
use indexmap::IndexMap;
use mysten_util_mem::MallocSizeOf;
use once_cell::sync::OnceCell;
use proptest_derive::Arbitrary;
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::time::{Duration, SystemTime};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
};
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

pub type Transaction = Vec<u8>;
#[derive(Clone, Serialize, Deserialize, Default, Debug, PartialEq, Eq, Arbitrary)]
pub struct Batch {
    pub transactions: Vec<Transaction>,
    pub metadata: Metadata,
}

impl Batch {
    pub fn new(transactions: Vec<Transaction>) -> Self {
        Batch {
            transactions,
            metadata: Metadata::default(),
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
        write!(f, "{}", base64::encode(self.0))
    }
}

impl fmt::Display for BatchDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::encode(self.0).get(0..16).ok_or(fmt::Error)?
        )
    }
}

impl From<BatchDigest> for Digest<{ crypto::DIGEST_LENGTH }> {
    fn from(digest: BatchDigest) -> Self {
        Digest::new(digest.0)
    }
}

impl BatchDigest {
    pub fn new(val: [u8; crypto::DIGEST_LENGTH]) -> BatchDigest {
        BatchDigest(val)
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for Batch {
    type TypedDigest = BatchDigest;

    fn digest(&self) -> Self::TypedDigest {
        BatchDigest::new(
            crypto::DefaultHashFunction::digest_iterator(self.transactions.iter()).into(),
        )
    }
}

#[derive(Builder, Clone, Deserialize, MallocSizeOf, Serialize)]
#[builder(pattern = "owned", build_fn(skip))]
pub struct Header {
    // Primary that created the header. Must be the same primary that broadcasted the header.
    // Validation is at: https://github.com/MystenLabs/sui/blob/f0b80d9eeef44edd9fbe606cee16717622b68651/narwhal/primary/src/primary.rs#L713-L719
    pub author: PublicKey,
    pub round: Round,
    pub epoch: Epoch,
    pub created_at: TimestampMs,
    #[serde(with = "indexmap::serde_seq")]
    pub payload: IndexMap<BatchDigest, (WorkerId, TimestampMs)>,
    pub parents: BTreeSet<CertificateDigest>,
    pub ancestors: Vec<Option<(Round, CertificateDigest)>>,
    #[serde(skip)]
    digest: OnceCell<HeaderDigest>,
}

impl HeaderBuilder {
    pub fn build(self) -> Result<Header, fastcrypto::error::FastCryptoError> {
        let h = Header {
            author: self.author.unwrap(),
            round: self.round.unwrap(),
            epoch: self.epoch.unwrap(),
            created_at: self.created_at.unwrap_or(0),
            payload: self.payload.unwrap(),
            parents: self.parents.unwrap(),
            ancestors: self.ancestors.unwrap(),
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

impl Header {
    pub async fn new(
        author: PublicKey,
        round: Round,
        epoch: Epoch,
        payload: IndexMap<BatchDigest, (WorkerId, TimestampMs)>,
        parents: BTreeSet<CertificateDigest>,
        ancestors: Vec<Option<(Round, CertificateDigest)>>,
    ) -> Self {
        let header = Self {
            author,
            round,
            epoch,
            created_at: now(),
            payload,
            parents,
            ancestors,
            digest: OnceCell::default(),
        };
        let digest = Hash::digest(&header);
        header.digest.set(digest).unwrap();
        header
    }

    pub fn digest(&self) -> HeaderDigest {
        *self.digest.get_or_init(|| Hash::digest(self))
    }

    pub fn parents(&self) -> Vec<CertificateDigest> {
        self.ancestors
            .iter()
            .filter_map(|a| match a {
                Some((round, digest)) => {
                    if round + 1 == self.round {
                        Some(*digest)
                    } else {
                        None
                    }
                }
                None => None,
            })
            .collect()
    }

    pub fn ancestor_digests(&self) -> Vec<CertificateDigest> {
        self.ancestors
            .iter()
            .filter_map(|a| a.map(|inner| inner.1))
            .collect()
    }

    pub fn ancestors(&self) -> Vec<(Round, CertificateDigest)> {
        self.ancestors.iter().filter_map(|a| *a).collect()
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
        let voting_rights = committee.stake(&self.author);
        ensure!(
            voting_rights > 0,
            DagError::UnknownAuthority(self.author.encode_base64())
        );

        // Ensure all worker ids are correct.
        for (worker_id, _) in self.payload.values() {
            worker_cache
                .worker(&self.author, worker_id)
                .map_err(|_| DagError::HeaderHasBadWorkerIds(self.digest()))?;
        }

        Ok(())
    }
}

impl Default for Header {
    fn default() -> Self {
        Self {
            author: PublicKey::insecure_default(),
            round: Round::default(),
            epoch: Epoch::default(),
            created_at: TimestampMs::default(),
            payload: IndexMap::default(),
            parents: BTreeSet::default(),
            ancestors: Vec::default(),
            digest: OnceCell::default(),
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
pub struct HeaderDigest([u8; crypto::DIGEST_LENGTH]);

impl From<HeaderDigest> for Digest<{ crypto::DIGEST_LENGTH }> {
    fn from(hd: HeaderDigest) -> Self {
        Digest::new(hd.0)
    }
}

impl fmt::Debug for HeaderDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(self.0))
    }
}

impl fmt::Display for HeaderDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::encode(self.0).get(0..16).ok_or(fmt::Error)?
        )
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for Header {
    type TypedDigest = HeaderDigest;

    fn digest(&self) -> HeaderDigest {
        let mut hasher = crypto::DefaultHashFunction::new();
        hasher.update(bcs::to_bytes(&self).expect("Serialization should not fail"));
        HeaderDigest(hasher.finalize().into())
    }
}

impl fmt::Debug for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}: B{}({}, E{}, {}B)",
            self.digest.get().cloned().unwrap_or_default(),
            self.round,
            self.author.encode_base64(),
            self.epoch,
            self.payload
                .keys()
                .map(|x| Digest::from(*x).size())
                .sum::<usize>(),
        )
    }
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "B{}({})", self.round, self.author.encode_base64())
    }
}

impl PartialEq for Header {
    fn eq(&self, other: &Self) -> bool {
        self.digest() == other.digest()
    }
}

/// A Vote on a Header is a claim by the voting authority that all payloads and the full history
/// of Certificates included in the Header are available.
#[derive(Clone, Serialize, Deserialize)]
pub struct Vote {
    // HeaderDigest, round, epoch and origin for the header being voted on.
    pub header_digest: HeaderDigest,
    pub round: Round,
    pub epoch: Epoch,
    pub origin: PublicKey,
    // Author of this vote.
    pub author: PublicKey,
    // Signature of the HeaderDigest.
    pub signature: <PublicKey as VerifyingKey>::Sig,
}

impl Vote {
    pub async fn new(
        header: &Header,
        author: &PublicKey,
        signature_service: &SignatureService<Signature, { crypto::INTENT_MESSAGE_LENGTH }>,
    ) -> Self {
        let vote = Self {
            header_digest: header.digest(),
            round: header.round,
            epoch: header.epoch,
            origin: header.author.clone(),
            author: author.clone(),
            signature: Signature::default(),
        };
        let signature = signature_service
            .request_signature(vote.digest().into())
            .await;
        Self { signature, ..vote }
    }

    pub fn new_with_signer<S>(header: &Header, author: &PublicKey, signer: &S) -> Self
    where
        S: Signer<Signature>,
    {
        let vote = Self {
            header_digest: header.digest(),
            round: header.round,
            epoch: header.epoch,
            origin: header.author.clone(),
            author: author.clone(),
            signature: Signature::default(),
        };

        let vote_digest: Digest<{ crypto::DIGEST_LENGTH }> = vote.digest().into();
        let signature = Signature::new_secure(&to_intent_message(vote_digest), signer);

        Self { signature, ..vote }
    }

    pub fn verify(&self, committee: &Committee) -> DagResult<()> {
        // Ensure the header is from the correct epoch.
        ensure!(
            self.epoch == committee.epoch(),
            DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: self.epoch
            }
        );

        // Ensure the authority has voting rights.
        ensure!(
            committee.stake(&self.author) > 0,
            DagError::UnknownAuthority(self.author.encode_base64())
        );

        // Check the signature.
        let vote_digest: Digest<{ crypto::DIGEST_LENGTH }> = self.digest().into();
        self.signature
            .verify_secure(&to_intent_message(vote_digest), &self.author)
            .map_err(|_| DagError::InvalidSignature)
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
        write!(f, "{}", base64::encode(self.0))
    }
}

impl fmt::Display for VoteDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::encode(self.0).get(0..16).ok_or(fmt::Error)?
        )
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for Vote {
    type TypedDigest = VoteDigest;

    fn digest(&self) -> VoteDigest {
        VoteDigest(self.header_digest.0)
    }
}

impl fmt::Debug for Vote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}: V{}({}, {}, E{})",
            self.digest(),
            self.round,
            self.author.encode_base64(),
            self.origin.encode_base64(),
            self.epoch
        )
    }
}

impl PartialEq for Vote {
    fn eq(&self, other: &Self) -> bool {
        self.digest() == other.digest()
    }
}

#[serde_as]
#[derive(Clone, Serialize, Deserialize, Default, MallocSizeOf)]
pub struct Certificate {
    pub header: Header,
    aggregated_signature: AggregateSignature,
    #[serde_as(as = "NarwhalBitmap")]
    signed_authorities: roaring::RoaringBitmap,
    pub metadata: Metadata,
}

impl Certificate {
    pub fn genesis(committee: &Committee) -> Vec<Self> {
        committee
            .authorities
            .keys()
            .map(|name| Self {
                header: Header {
                    author: name.clone(),
                    epoch: committee.epoch(),
                    ..Header::default()
                },
                ..Self::default()
            })
            .collect()
    }

    pub fn new(
        committee: &Committee,
        header: Header,
        votes: Vec<(PublicKey, Signature)>,
    ) -> DagResult<Certificate> {
        Self::new_unsafe(committee, header, votes, true)
    }

    pub fn new_unsigned(
        committee: &Committee,
        header: Header,
        votes: Vec<(PublicKey, Signature)>,
    ) -> DagResult<Certificate> {
        Self::new_unsafe(committee, header, votes, false)
    }

    pub fn new_test_empty(author: PublicKey) -> Self {
        let header = Header {
            author,
            ..Default::default()
        };
        Self {
            header,
            ..Default::default()
        }
    }

    fn new_unsafe(
        committee: &Committee,
        header: Header,
        votes: Vec<(PublicKey, Signature)>,
        check_stake: bool,
    ) -> DagResult<Certificate> {
        let mut votes = votes;
        votes.sort_by_key(|(pk, _)| pk.clone());
        let mut votes: VecDeque<_> = votes.into_iter().collect();

        let mut weight = 0;
        let keys = committee.keys();
        let mut sigs = Vec::new();

        let filtered_votes = keys
            .iter()
            .enumerate()
            .filter(|(_, &pk)| {
                if !votes.is_empty() && pk == &votes.front().unwrap().0 {
                    sigs.push(votes.pop_front().unwrap());
                    weight += &committee.stake(pk);
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
            DagError::UnknownAuthority(votes.front().unwrap().0.encode_base64())
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

        Ok(Certificate {
            header,
            aggregated_signature,
            signed_authorities,
            metadata: Metadata::default(),
        })
    }

    /// This function requires that certificate was verified against given committee
    pub fn signed_authorities(&self, committee: &Committee) -> Vec<PublicKey> {
        assert_eq!(committee.epoch, self.epoch());
        let (_stake, pks) = self.signed_by(committee);
        pks
    }

    fn signed_by(&self, committee: &Committee) -> (Stake, Vec<PublicKey>) {
        // Ensure the certificate has a quorum.
        let mut weight = 0;

        let auth_indexes = self.signed_authorities.iter().collect::<Vec<_>>();
        let mut auth_iter = 0;
        let pks = committee
            .authorities()
            .enumerate()
            .filter(|(i, (_, auth))| match auth_indexes.get(auth_iter) {
                Some(index) if *index == *i as u32 => {
                    weight += auth.stake;
                    auth_iter += 1;
                    true
                }
                _ => false,
            })
            .map(|(_, (pk, _))| pk.clone())
            .collect();
        (weight, pks)
    }

    /// Verifies the validity of the certificate.
    /// TODO: Output a different type, similar to Sui VerifiedCertificate.
    pub fn verify(&self, committee: &Committee, worker_cache: &WorkerCache) -> DagResult<()> {
        // Ensure the header is from the correct epoch.
        ensure!(
            self.epoch() == committee.epoch(),
            DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: self.epoch()
            }
        );

        // Genesis certificates are always valid.
        if self.round() == 0 && Self::genesis(committee).contains(self) {
            return Ok(());
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
        self.aggregated_signature
            .verify_secure(&to_intent_message(certificate_digest), &pks[..])
            .map_err(|_| DagError::InvalidSignature)?;

        Ok(())
    }

    pub fn round(&self) -> Round {
        self.header.round
    }

    pub fn epoch(&self) -> Epoch {
        self.header.epoch
    }

    pub fn origin(&self) -> PublicKey {
        self.header.author.clone()
    }
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
impl From<CertificateDigest> for CertificateDigestProto {
    fn from(hd: CertificateDigest) -> Self {
        CertificateDigestProto {
            digest: Bytes::from(hd.0.to_vec()),
        }
    }
}

impl fmt::Debug for CertificateDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(self.0))
    }
}

impl fmt::Display for CertificateDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::encode(self.0).get(0..16).ok_or(fmt::Error)?
        )
    }
}

impl Hash<{ crypto::DIGEST_LENGTH }> for Certificate {
    type TypedDigest = CertificateDigest;

    fn digest(&self) -> CertificateDigest {
        CertificateDigest(self.header.digest().0)
    }
}

impl fmt::Debug for Certificate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}: C{}({}, {}, E{})",
            self.digest(),
            self.round(),
            self.origin().encode_base64(),
            self.header.digest(),
            self.epoch()
        )
    }
}

impl PartialEq for Certificate {
    fn eq(&self, other: &Self) -> bool {
        let mut ret = self.header.digest() == other.header.digest();
        ret &= self.round() == other.round();
        ret &= self.epoch() == other.epoch();
        ret &= self.origin() == other.origin();
        ret
    }
}

impl Affiliated for Certificate {
    fn parents(&self) -> Vec<<Self as Hash<{ crypto::DIGEST_LENGTH }>>::TypedDigest> {
        self.header.parents.iter().cloned().collect()
    }

    // This makes the genesis certificate and empty blocks compressible,
    // so that they will never be reported by a DAG walk
    // (`read_causal`, `node_read_causal`).
    fn compressible(&self) -> bool {
        self.header.payload.is_empty()
    }
}

/// Request for broadcasting certificates to peers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendCertificateRequest {
    pub certificate: Certificate,
}

/// Response from peers after receiving a certificate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendCertificateResponse {}

/// Used by the primary to request a vote from other primaries on newly produced headers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestVoteRequest {
    pub header: Header,

    // Optional parent certificates provided by the requester, in case this primary doesn't yet
    // have them and requires them in order to offer a vote.
    pub parents: Vec<Certificate>,

    pub ancestors: Vec<Certificate>,
}

/// Used by the primary to reply to RequestVoteRequest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestVoteResponse {
    pub vote: Option<Vote>,

    // Indicates digests of missing certificates without which a vote cannot be provided.
    pub missing: Vec<CertificateDigest>,
}

/// Used by the primary to get specific certificates from other primaries.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetCertificatesRequest {
    pub digests: Vec<CertificateDigest>,
}

/// Used by the primary to reply to GetCertificatesRequest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetCertificatesResponse {
    pub certificates: Vec<Certificate>,
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
    pub skip_rounds: Vec<(PublicKeyBytes, Vec<u8>)>,
    /// Maximum number of certificates that should be returned.
    pub max_items: usize,
}

impl FetchCertificatesRequest {
    #[allow(clippy::mutable_key_type)]
    pub fn get_bounds(&self) -> (Round, BTreeMap<PublicKeyBytes, BTreeSet<Round>>) {
        let skip_rounds: BTreeMap<PublicKeyBytes, BTreeSet<Round>> = self
            .skip_rounds
            .iter()
            .filter_map(|(k, serialized)| {
                match RoaringBitmap::deserialize_from(&mut &serialized[..]) {
                    Ok(bitmap) => {
                        let rounds: BTreeSet<Round> = bitmap
                            .into_iter()
                            .map(|r| self.exclusive_lower_bound + r as Round)
                            .collect();
                        Some((k.clone(), rounds))
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
        skip_rounds: BTreeMap<PublicKeyBytes, BTreeSet<Round>>,
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

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct PayloadAvailabilityRequest {
    pub certificate_digests: Vec<CertificateDigest>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct PayloadAvailabilityResponse {
    pub payload_availability: Vec<(CertificateDigest, bool)>,
}

impl PayloadAvailabilityResponse {
    pub fn available_certificates(&self) -> Vec<CertificateDigest> {
        self.payload_availability
            .iter()
            .filter_map(|(digest, available)| available.then_some(*digest))
            .collect()
    }
}

/// Used by the primary to request that the worker sync the target missing batches.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerSynchronizeMessage {
    pub digests: Vec<BatchDigest>,
    pub target: PublicKey,
    // Used to indicate to the worker that it does not need to fully validate
    // the batch it receives because it is part of a certificate. Only digest
    // verification is required.
    pub is_certified: bool,
}

/// Used by the primary to request that the worker delete the specified batches.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkerDeleteBatchesMessage {
    pub digests: Vec<BatchDigest>,
}

#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct BatchMessage {
    // TODO: revisit including the digest here [see #188]
    pub digest: BatchDigest,
    pub batch: Batch,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BlockErrorKind {
    BlockNotFound,
    BatchTimeout,
    BatchError,
}

pub type BlockResult<T> = Result<T, BlockError>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BlockError {
    pub digest: CertificateDigest,
    pub error: BlockErrorKind,
}

impl<T> From<BlockError> for BlockResult<T> {
    fn from(error: BlockError) -> Self {
        Err(error)
    }
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "block digest: {}, error type: {}",
            self.digest, self.error
        )
    }
}

impl fmt::Display for BlockErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Used by worker to inform primary it sealed a new batch.
#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct WorkerOurBatchMessage {
    pub digest: BatchDigest,
    pub worker_id: WorkerId,
    pub metadata: Metadata,
}

/// Used by worker to inform primary it received a batch from another authority.
#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct WorkerOthersBatchMessage {
    pub digest: BatchDigest,
    pub worker_id: WorkerId,
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct WorkerInfoResponse {
    /// Map of workers' id and their network addresses.
    pub workers: BTreeMap<WorkerId, WorkerInfo>,
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct VoteInfo {
    /// The latest Epoch for which a vote was sent to given authority
    pub epoch: Epoch,
    /// The latest round for which a vote was sent to given authority
    pub round: Round,
    /// The hash of the vote used to ensure equality
    pub vote_digest: VoteDigest,
}

#[cfg(test)]
mod tests {
    use crate::{Batch, Metadata, Timestamp};
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_elapsed() {
        let batch = Batch::new(vec![]);
        assert!(batch.metadata.created_at > 0);

        sleep(Duration::from_secs(2)).await;

        assert!(batch.metadata.created_at.elapsed().as_secs_f64() >= 2.0);
    }

    #[test]
    fn test_elapsed_when_newer_than_now() {
        let batch = Batch {
            transactions: vec![],
            metadata: Metadata {
                created_at: 2999309726980, // something in the future - Fri Jan 16 2065 05:35:26
            },
        };

        assert_eq!(batch.metadata.created_at.elapsed().as_secs_f64(), 0.0);
    }
}
