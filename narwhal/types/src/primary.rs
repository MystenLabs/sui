// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    error::{DagError, DagResult},
    serde::NarwhalBitmap,
    CertificateDigestProto,
};
use blake2::{digest::Update, VarBlake2b};
use bytes::Bytes;
use config::{Committee, Epoch, SharedWorkerCache, Stake, WorkerId, WorkerInfo};
use crypto::{AggregateSignature, PublicKey, Signature};
use dag::node_dag::Affiliated;
use derive_builder::Builder;
use fastcrypto::{
    traits::{AggregateAuthenticator, EncodeDecodeBase64, Signer, VerifyingKey},
    Digest, Hash, SignatureService, Verifier, DIGEST_LEN,
};
use indexmap::IndexMap;
use mysten_util_mem::MallocSizeOf;
use proptest_derive::Arbitrary;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
};

/// The round number.
pub type Round = u64;

pub type Transaction = Vec<u8>;
#[derive(Clone, Serialize, Deserialize, Default, Debug, PartialEq, Eq, Arbitrary)]
pub struct Batch(pub Vec<Transaction>);

#[derive(
    Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, Hash, PartialOrd, Ord, MallocSizeOf,
)]
pub struct BatchDigest(pub [u8; DIGEST_LEN]);

impl fmt::Debug for BatchDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(self.0))
    }
}

impl fmt::Display for BatchDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(self.0).get(0..16).unwrap())
    }
}

impl From<BatchDigest> for Digest {
    fn from(digest: BatchDigest) -> Self {
        Digest::new(digest.0)
    }
}

impl BatchDigest {
    pub fn new(val: [u8; DIGEST_LEN]) -> BatchDigest {
        BatchDigest(val)
    }
}

impl Hash for Batch {
    type TypedDigest = BatchDigest;

    fn digest(&self) -> Self::TypedDigest {
        BatchDigest::new(fastcrypto::blake2b_256(|hasher| {
            self.0.iter().for_each(|tx| hasher.update(tx))
        }))
    }
}

#[derive(Builder, Clone, Default, Deserialize, MallocSizeOf, Serialize)]
#[builder(pattern = "owned", build_fn(skip))]
pub struct Header {
    pub author: PublicKey,
    pub round: Round,
    pub epoch: Epoch,
    #[serde(with = "indexmap::serde_seq")]
    pub payload: IndexMap<BatchDigest, WorkerId>,
    pub parents: BTreeSet<CertificateDigest>,
    pub id: HeaderDigest,
    pub signature: Signature,
}

impl HeaderBuilder {
    pub fn build<F>(self, signer: &F) -> Result<Header, fastcrypto::traits::Error>
    where
        F: Signer<Signature>,
    {
        let h = Header {
            author: self.author.unwrap(),
            round: self.round.unwrap(),
            epoch: self.epoch.unwrap(),
            payload: self.payload.unwrap(),
            parents: self.parents.unwrap(),
            id: HeaderDigest::default(),
            signature: Signature::default(),
        };

        Ok(Header {
            id: h.digest(),
            signature: signer.try_sign(Digest::from(h.digest()).as_ref())?,
            ..h
        })
    }

    // helper method to set directly values to the payload
    pub fn with_payload_batch(mut self, batch: Batch, worker_id: WorkerId) -> Self {
        if self.payload.is_none() {
            self.payload = Some(Default::default());
        }
        let payload = self.payload.as_mut().unwrap();

        payload.insert(batch.digest(), worker_id);

        self
    }
}

impl Header {
    pub async fn new(
        author: PublicKey,
        round: Round,
        epoch: Epoch,
        payload: IndexMap<BatchDigest, WorkerId>,
        parents: BTreeSet<CertificateDigest>,
        signature_service: &mut SignatureService<Signature>,
    ) -> Self {
        let header = Self {
            author,
            round,
            epoch,
            payload,
            parents,
            id: HeaderDigest::default(),
            signature: Signature::default(),
        };
        let id = header.digest();
        let signature = signature_service.request_signature(id.into()).await;
        Self {
            id,
            signature,
            ..header
        }
    }

    pub fn verify(&self, committee: &Committee, worker_cache: SharedWorkerCache) -> DagResult<()> {
        // Ensure the header is from the correct epoch.
        ensure!(
            self.epoch == committee.epoch(),
            DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: self.epoch
            }
        );

        // Ensure the header id is well formed.
        ensure!(self.digest() == self.id, DagError::InvalidHeaderId);

        // Ensure the authority has voting rights.
        let voting_rights = committee.stake(&self.author);
        ensure!(
            voting_rights > 0,
            DagError::UnknownAuthority(self.author.encode_base64())
        );

        // Ensure all worker ids are correct.
        for worker_id in self.payload.values() {
            worker_cache
                .load()
                .worker(&self.author, worker_id)
                .map_err(|_| DagError::MalformedHeader(self.id))?;
        }

        // Check the signature.
        let id_digest: Digest = Digest::from(self.id);
        self.author
            .verify(id_digest.as_ref(), &self.signature)
            .map_err(DagError::from)
    }
}

#[derive(
    Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, Hash, PartialOrd, Ord, MallocSizeOf,
)]
pub struct HeaderDigest([u8; DIGEST_LEN]);

impl From<HeaderDigest> for Digest {
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
        write!(f, "{}", base64::encode(self.0).get(0..16).unwrap())
    }
}

impl Hash for Header {
    type TypedDigest = HeaderDigest;

    fn digest(&self) -> HeaderDigest {
        let hasher_update = |hasher: &mut VarBlake2b| {
            hasher.update(&self.author);
            hasher.update(self.round.to_le_bytes());
            hasher.update(self.epoch.to_le_bytes());
            for (x, y) in self.payload.iter() {
                hasher.update(Digest::from(*x));
                hasher.update(y.to_le_bytes());
            }
            for x in self.parents.iter() {
                hasher.update(Digest::from(*x))
            }
        };
        HeaderDigest(fastcrypto::blake2b_256(hasher_update))
    }
}

impl fmt::Debug for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}: B{}({}, E{}, {}B)",
            self.id,
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

#[derive(Clone, Serialize, Deserialize)]
pub struct Vote {
    pub id: HeaderDigest,
    pub round: Round,
    pub epoch: Epoch,
    pub origin: PublicKey,
    pub author: PublicKey,
    pub signature: <PublicKey as VerifyingKey>::Sig,
}

impl Vote {
    pub async fn new(
        header: &Header,
        author: &PublicKey,
        signature_service: &mut SignatureService<Signature>,
    ) -> Self {
        let vote = Self {
            id: header.id,
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
            id: header.id,
            round: header.round,
            epoch: header.epoch,
            origin: header.author.clone(),
            author: author.clone(),
            signature: Signature::default(),
        };

        let vote_digest: Digest = vote.digest().into();
        let signature = signer.sign(vote_digest.as_ref());

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
        let vote_digest: Digest = self.digest().into();
        self.author
            .verify(vote_digest.as_ref(), &self.signature)
            .map_err(DagError::from)
    }
}
#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Copy)]
pub struct VoteDigest([u8; DIGEST_LEN]);

impl From<VoteDigest> for Digest {
    fn from(hd: VoteDigest) -> Self {
        Digest::new(hd.0)
    }
}

impl fmt::Debug for VoteDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(self.0))
    }
}

impl fmt::Display for VoteDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(self.0).get(0..16).unwrap())
    }
}

impl Hash for Vote {
    type TypedDigest = VoteDigest;

    fn digest(&self) -> VoteDigest {
        let hasher_update = |hasher: &mut VarBlake2b| {
            hasher.update(Digest::from(self.id));
            hasher.update(self.round.to_le_bytes());
            hasher.update(self.epoch.to_le_bytes());
            hasher.update(&self.origin);
        };

        VoteDigest(fastcrypto::blake2b_256(hasher_update))
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
            self.id,
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
#[derive(Clone, MallocSizeOf, Serialize, Deserialize, Default)]
pub struct Certificate {
    pub header: Header,
    aggregated_signature: AggregateSignature,
    #[serde_as(as = "NarwhalBitmap")]
    signed_authorities: roaring::RoaringBitmap,
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
            AggregateSignature::aggregate(sigs.into_iter().map(|(_, sig)| sig).collect())
                .map_err(DagError::InvalidSignature)?
        };

        Ok(Certificate {
            header,
            aggregated_signature,
            signed_authorities,
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

    pub fn verify(&self, committee: &Committee, worker_cache: SharedWorkerCache) -> DagResult<()> {
        // Ensure the header is from the correct epoch.
        ensure!(
            self.epoch() == committee.epoch(),
            DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: self.epoch()
            }
        );

        // Genesis certificates are always valid.
        if Self::genesis(committee).contains(self) {
            return Ok(());
        }

        // Check the embedded header.
        self.header.verify(committee, worker_cache)?;

        let (weight, pks) = self.signed_by(committee);

        ensure!(
            weight >= committee.quorum_threshold(),
            DagError::CertificateRequiresQuorum
        );

        // Verify the signatures
        let certificate_digest: Digest = Digest::from(self.digest());
        self.aggregated_signature
            .verify(&pks[..], certificate_digest.as_ref())
            .map_err(|_| signature::Error::new())
            .map_err(DagError::from)?;

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
    Clone, Copy, Serialize, Deserialize, Default, MallocSizeOf, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
pub struct CertificateDigest([u8; DIGEST_LEN]);

impl CertificateDigest {
    pub fn new(digest: [u8; DIGEST_LEN]) -> CertificateDigest {
        CertificateDigest(digest)
    }
}

impl AsRef<[u8]> for CertificateDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<CertificateDigest> for Digest {
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
        write!(f, "{}", base64::encode(self.0).get(0..16).unwrap())
    }
}

impl Hash for Certificate {
    type TypedDigest = CertificateDigest;

    fn digest(&self) -> CertificateDigest {
        let hasher_update = |hasher: &mut VarBlake2b| {
            hasher.update(Digest::from(self.header.id));
            hasher.update(self.round().to_le_bytes());
            hasher.update(self.epoch().to_le_bytes());
            hasher.update(&self.origin());
        };

        CertificateDigest(fastcrypto::blake2b_256(hasher_update))
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
            self.header.id,
            self.epoch()
        )
    }
}

impl PartialEq for Certificate {
    fn eq(&self, other: &Self) -> bool {
        let mut ret = self.header.id == other.header.id;
        ret &= self.round() == other.round();
        ret &= self.epoch() == other.epoch();
        ret &= self.origin() == other.origin();
        ret
    }
}

impl Affiliated for Certificate {
    fn parents(&self) -> Vec<<Self as Hash>::TypedDigest> {
        self.header.parents.iter().cloned().collect()
    }

    // This makes the genesis certificate and empty blocks compressible,
    // so that they will never be reported by a DAG walk
    // (`read_causal`, `node_read_causal`).
    fn compressible(&self) -> bool {
        self.header.payload.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PrimaryMessage {
    Header(Header),
    Vote(Vote),
    Certificate(Certificate),
    CertificatesRequest(Vec<CertificateDigest>, /* requestor */ PublicKey),

    CertificatesBatchRequest {
        certificate_ids: Vec<CertificateDigest>,
        requestor: PublicKey,
    },
    CertificatesBatchResponse {
        certificates: Vec<(CertificateDigest, Option<Certificate>)>,
        from: PublicKey,
    },

    CertificatesRangeRequest {
        // Requests certificate digests with rounds >= `range_start` to be sent back.
        // No upper range limit is specified, because the requestor does not know the
        // current upper limit. The response size should still be acceptable if all
        // certificate digests from an authority are returned: e.g. a response can be
        // 32B / digest * 200 authorities * 50 rounds ~ 320KB
        range_start: Round,
        // Maximum number of rounds that should be contained in each reply.
        max_rounds: u64,
        requestor: PublicKey,
    },
    CertificatesRangeResponse {
        // Certificate digests, grouped by round numbers.
        certificate_ids: BTreeMap<Round, Vec<CertificateDigest>>,
        from: PublicKey,
    },

    PayloadAvailabilityRequest {
        certificate_ids: Vec<CertificateDigest>,
        requestor: PublicKey,
    },

    PayloadAvailabilityResponse {
        payload_availability: Vec<(CertificateDigest, bool)>,
        from: PublicKey,
    },
}

/// Message to reconfigure worker tasks. This message must be sent by a trusted source.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum ReconfigureNotification {
    /// Indicate the committee has changed. This happens at epoch change.
    NewEpoch(Committee),
    /// Update some network information of the committee.
    UpdateCommittee(Committee),
    /// Indicate a shutdown.
    Shutdown,
}

/// The messages sent by the primary to its workers.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PrimaryWorkerMessage {
    /// Reconfigure the worker.
    Reconfigure(ReconfigureNotification),
    /// The primary requests a batch from the worker
    RequestBatch(BatchDigest),
    /// Delete the batches, dictated from the provided vector of digest, from the worker node
    DeleteBatches(Vec<BatchDigest>),
}

/// Used by the primary to request that the worker sync the target missing batches.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerSynchronizeMessage {
    pub digests: Vec<BatchDigest>,
    pub target: PublicKey,
}

#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct BatchMessage {
    // TODO: revisit including the id here [see #188]
    pub id: BatchDigest,
    pub transactions: Batch,
}

pub type BlockRemoverResult<T> = Result<T, BlockRemoverError>;

#[derive(Clone, Debug)]
pub struct BlockRemoverError {
    pub ids: Vec<CertificateDigest>,
    pub error: BlockRemoverErrorKind,
}

// TODO: refactor BlockError & BlockRemoverError to be one type shared by get/remove collections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlockRemoverErrorKind {
    Timeout,
    Failed,
    StorageFailure,
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
    pub id: CertificateDigest,
    pub error: BlockErrorKind,
}

impl<T> From<BlockError> for BlockResult<T> {
    fn from(error: BlockError) -> Self {
        Err(error)
    }
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "block id: {}, error type: {}", self.id, self.error)
    }
}

impl fmt::Display for BlockErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// The messages sent by the workers to their primary.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum WorkerPrimaryMessage {
    /// The worker indicates it sealed a new batch.
    OurBatch(BatchDigest, WorkerId),
    /// The worker indicates it received a batch's digest from another authority.
    OthersBatch(BatchDigest, WorkerId),
    /// The worker sends a requested batch
    RequestedBatch(BatchDigest, Batch),
    /// When batches are successfully deleted, this message is sent dictating the
    /// batches that have been deleted from the worker.
    DeletedBatches(Vec<BatchDigest>),
    /// An error has been returned by worker
    Error(WorkerPrimaryError),
    /// Reconfiguration message sent by the executor (usually upon epoch change).
    Reconfigure(ReconfigureNotification),
}

#[derive(Debug, Serialize, Deserialize, thiserror::Error, Clone, Eq, PartialEq)]
pub enum WorkerPrimaryError {
    #[error("Batch with id {0} has not been found")]
    RequestedBatchNotFound(BatchDigest),

    #[error("An error occurred while deleting batches. None deleted")]
    ErrorWhileDeletingBatches(Vec<BatchDigest>),
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct WorkerInfoResponse {
    /// Map of workers' id and their network addresses.
    pub workers: BTreeMap<WorkerId, WorkerInfo>,
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct RoundVoteDigestPair {
    /// The latest round for which a vote was sent to given authority
    pub round: Round,
    /// The hash of the vote used to ensure equality
    pub vote_digest: VoteDigest,
}
