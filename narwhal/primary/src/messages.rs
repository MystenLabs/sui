// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    error::{DagError, DagResult},
    primary::Round,
};
use config::{Committee, WorkerId};
use crypto::{
    traits::{EncodeDecodeBase64, VerifyingKey},
    Digest, Hash, SignatureService, DIGEST_LEN,
};
use ed25519_dalek::{Digest as _, Sha512};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    convert::TryInto,
    fmt,
};

pub type Transaction = Vec<u8>;
#[derive(Clone, Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
pub struct Batch(pub Vec<Transaction>);

#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BatchDigest(pub [u8; crypto::DIGEST_LEN]);

impl fmt::Debug for BatchDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(&self.0))
    }
}

impl fmt::Display for BatchDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(&self.0).get(0..16).unwrap())
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
        let mut hasher = Sha512::new();
        for x in &self.0 {
            hasher.update(x);
        }
        BatchDigest(
            hasher.finalize().as_slice()[..DIGEST_LEN]
                .try_into()
                .unwrap(),
        )
    }
}

#[derive(Clone, Serialize, Deserialize, Default, Builder)]
#[builder(pattern = "owned", build_fn(skip))]
#[serde(bound(deserialize = "PublicKey: VerifyingKey"))] // bump the bound to VerifyingKey as soon as you include a sig
pub struct Header<PublicKey: VerifyingKey> {
    pub author: PublicKey,
    pub round: Round,
    pub payload: BTreeMap<BatchDigest, WorkerId>,
    pub parents: BTreeSet<CertificateDigest>,
    pub id: HeaderDigest,
    pub signature: <PublicKey as VerifyingKey>::Sig,
}

impl<PublicKey: VerifyingKey> HeaderBuilder<PublicKey> {
    #[allow(dead_code)]
    pub fn build<F>(self, signer: F) -> Header<PublicKey>
    where
        F: FnOnce(&[u8]) -> PublicKey::Sig,
    {
        let h = Header {
            author: self.author.unwrap(),
            round: self.round.unwrap(),
            payload: self.payload.unwrap(),
            parents: self.parents.unwrap(),
            id: HeaderDigest::default(),
            signature: PublicKey::Sig::default(),
        };

        Header {
            id: h.digest(),
            signature: signer(Digest::from(h.digest()).as_ref()),
            ..h
        }
    }

    // helper method to set directly values to the payload
    #[allow(dead_code)]
    pub fn with_payload_batch(mut self, batch: Batch, worker_id: WorkerId) -> Self {
        if self.payload.is_none() {
            self.payload = Some(BTreeMap::new());
        }
        let payload = self.payload.as_mut().unwrap();

        payload.insert(batch.digest(), worker_id);

        self
    }
}

impl<PublicKey: VerifyingKey> Header<PublicKey> {
    pub async fn new(
        author: PublicKey,
        round: Round,
        payload: BTreeMap<BatchDigest, WorkerId>,
        parents: BTreeSet<CertificateDigest>,
        signature_service: &mut SignatureService<PublicKey::Sig>,
    ) -> Self {
        let header = Self {
            author,
            round,
            payload,
            parents,
            id: HeaderDigest::default(),
            signature: PublicKey::Sig::default(),
        };
        let id = header.digest();
        let signature = signature_service.request_signature(id.into()).await;
        Self {
            id,
            signature,
            ..header
        }
    }

    pub fn verify(&self, committee: &Committee<PublicKey>) -> DagResult<()> {
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
            committee
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

#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HeaderDigest([u8; DIGEST_LEN]);

impl From<HeaderDigest> for Digest {
    fn from(hd: HeaderDigest) -> Self {
        Digest::new(hd.0)
    }
}

impl fmt::Debug for HeaderDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(&self.0))
    }
}

impl fmt::Display for HeaderDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(&self.0).get(0..16).unwrap())
    }
}

impl<PublicKey: VerifyingKey> Hash for Header<PublicKey> {
    type TypedDigest = HeaderDigest;

    fn digest(&self) -> HeaderDigest {
        let mut hasher = Sha512::new();
        hasher.update(&self.author);
        hasher.update(self.round.to_le_bytes());
        for (x, y) in &self.payload {
            hasher.update(Digest::from(*x).as_ref());
            hasher.update(y.to_le_bytes());
        }
        for x in &self.parents {
            hasher.update(Digest::from(*x).as_ref());
        }
        HeaderDigest(
            hasher.finalize().as_slice()[..DIGEST_LEN]
                .try_into()
                .unwrap(),
        )
    }
}

impl<PublicKey: VerifyingKey> fmt::Debug for Header<PublicKey> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}: B{}({}, {})",
            self.id,
            self.round,
            self.author.encode_base64(),
            self.payload
                .keys()
                .map(|x| Digest::from(*x).size())
                .sum::<usize>(),
        )
    }
}

impl<PublicKey: VerifyingKey> fmt::Display for Header<PublicKey> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "B{}({})", self.round, self.author.encode_base64())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "PublicKey: VerifyingKey"))] // bump the bound to VerifyingKey as soon as you include a sig

pub struct Vote<PublicKey: VerifyingKey> {
    pub id: HeaderDigest,
    pub round: Round,
    pub origin: PublicKey,
    pub author: PublicKey,
    pub signature: <PublicKey as VerifyingKey>::Sig,
}

impl<PublicKey: VerifyingKey> Vote<PublicKey> {
    pub async fn new(
        header: &Header<PublicKey>,
        author: &PublicKey,
        signature_service: &mut SignatureService<PublicKey::Sig>,
    ) -> Self {
        let vote = Self {
            id: header.id,
            round: header.round,
            origin: header.author.clone(),
            author: author.clone(),
            signature: PublicKey::Sig::default(),
        };
        let signature = signature_service
            .request_signature(vote.digest().into())
            .await;
        Self { signature, ..vote }
    }

    pub fn verify(&self, committee: &Committee<PublicKey>) -> DagResult<()> {
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
#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoteDigest([u8; DIGEST_LEN]);

impl From<VoteDigest> for Digest {
    fn from(hd: VoteDigest) -> Self {
        Digest::new(hd.0)
    }
}

impl fmt::Debug for VoteDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(&self.0))
    }
}

impl fmt::Display for VoteDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(&self.0).get(0..16).unwrap())
    }
}

impl<PublicKey: VerifyingKey> Hash for Vote<PublicKey> {
    type TypedDigest = VoteDigest;

    fn digest(&self) -> VoteDigest {
        let mut hasher = Sha512::new();
        let header_id: Digest = self.id.into();
        hasher.update(header_id);
        hasher.update(self.round.to_le_bytes());
        hasher.update(&self.origin);
        VoteDigest(
            hasher.finalize().as_slice()[..DIGEST_LEN]
                .try_into()
                .unwrap(),
        )
    }
}

impl<PublicKey: VerifyingKey> fmt::Debug for Vote<PublicKey> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}: V{}({}, {})",
            self.digest(),
            self.round,
            self.author.encode_base64(),
            self.id
        )
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(bound(deserialize = "PublicKey: VerifyingKey"))] // bump the bound to VerifyingKey as soon as you include a sig
pub struct Certificate<PublicKey: VerifyingKey> {
    pub header: Header<PublicKey>,
    pub votes: Vec<(PublicKey, <PublicKey as VerifyingKey>::Sig)>,
}

impl<PublicKey: VerifyingKey> Certificate<PublicKey> {
    pub fn genesis(committee: &Committee<PublicKey>) -> Vec<Self> {
        committee
            .authorities
            .keys()
            .map(|name| Self {
                header: Header {
                    author: name.clone(),
                    ..Header::default()
                },
                ..Self::default()
            })
            .collect()
    }

    pub fn verify(&self, committee: &Committee<PublicKey>) -> DagResult<()> {
        // Genesis certificates are always valid.
        if Self::genesis(committee).contains(self) {
            return Ok(());
        }

        // Check the embedded header.
        self.header.verify(committee)?;

        // Ensure the certificate has a quorum.
        let mut weight = 0;
        let mut used = HashSet::new();
        for (name, _) in self.votes.iter() {
            ensure!(
                !used.contains(name),
                DagError::AuthorityReuse(name.encode_base64())
            );
            let voting_rights = committee.stake(name);
            ensure!(
                voting_rights > 0,
                DagError::UnknownAuthority(name.encode_base64())
            );
            used.insert(name.clone());
            weight += voting_rights;
        }
        ensure!(
            weight >= committee.quorum_threshold(),
            DagError::CertificateRequiresQuorum
        );
        let (pks, sigs): (Vec<PublicKey>, Vec<PublicKey::Sig>) = self.votes.iter().cloned().unzip();
        // Verify the signatures
        let certificate_digest: Digest = Digest::from(self.digest());
        PublicKey::verify_batch(certificate_digest.as_ref(), &pks, &sigs).map_err(DagError::from)
    }

    pub fn round(&self) -> Round {
        self.header.round
    }

    pub fn origin(&self) -> PublicKey {
        self.header.author.clone()
    }
}
#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CertificateDigest([u8; DIGEST_LEN]);

impl From<CertificateDigest> for Digest {
    fn from(hd: CertificateDigest) -> Self {
        Digest::new(hd.0)
    }
}

impl fmt::Debug for CertificateDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(&self.0))
    }
}

impl fmt::Display for CertificateDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(&self.0).get(0..16).unwrap())
    }
}

impl<PublicKey: VerifyingKey> Hash for Certificate<PublicKey> {
    type TypedDigest = CertificateDigest;

    fn digest(&self) -> CertificateDigest {
        let mut hasher = Sha512::new();
        let header_id: Digest = self.header.id.into();
        hasher.update(header_id);
        hasher.update(self.round().to_le_bytes());
        hasher.update(self.origin());
        CertificateDigest(
            hasher.finalize().as_slice()[..DIGEST_LEN]
                .try_into()
                .unwrap(),
        )
    }
}

impl<PublicKey: VerifyingKey> fmt::Debug for Certificate<PublicKey> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}: C{}({}, {})",
            self.digest(),
            self.round(),
            self.origin().encode_base64(),
            self.header.id
        )
    }
}

impl<PublicKey: VerifyingKey> PartialEq for Certificate<PublicKey> {
    fn eq(&self, other: &Self) -> bool {
        let mut ret = self.header.id == other.header.id;
        ret &= self.round() == other.round();
        ret &= self.origin() == other.origin();
        ret
    }
}
