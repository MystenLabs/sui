// Copyright(C) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    error::{DagError, DagResult},
    primary::Round,
};
use config::{Committee, WorkerId};
use crypto::{
    traits::{EncodeDecodeBase64Ext, VerifyingKey},
    Digest, Hash, SignatureService,
};
use ed25519_dalek::{Digest as _, Sha512};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    convert::TryInto,
    fmt,
};

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(bound(deserialize = "PublicKey: VerifyingKey"))] // bump the bound to VerifyingKey as soon as you include a sig
pub struct Header<PublicKey: VerifyingKey> {
    pub author: PublicKey,
    pub round: Round,
    pub payload: BTreeMap<Digest, WorkerId>,
    pub parents: BTreeSet<Digest>,
    pub id: Digest,
    pub signature: <PublicKey as VerifyingKey>::Sig,
}

impl<PublicKey: VerifyingKey> Header<PublicKey> {
    pub async fn new(
        author: PublicKey,
        round: Round,
        payload: BTreeMap<Digest, WorkerId>,
        parents: BTreeSet<Digest>,
        signature_service: &mut SignatureService<PublicKey::Sig>,
    ) -> Self {
        let header = Self {
            author,
            round,
            payload,
            parents,
            id: Digest::default(),
            signature: PublicKey::Sig::default(),
        };
        let id = header.digest();
        let signature = signature_service.request_signature(id.clone()).await;
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
                .map_err(|_| DagError::MalformedHeader(self.id.clone()))?;
        }

        // Check the signature.
        self.author
            .verify(self.id.as_ref(), &self.signature)
            .map_err(DagError::from)
    }
}

impl<PublicKey: VerifyingKey> Hash for Header<PublicKey> {
    fn digest(&self) -> Digest {
        let mut hasher = Sha512::new();
        hasher.update(&self.author);
        hasher.update(self.round.to_le_bytes());
        for (x, y) in &self.payload {
            hasher.update(x);
            hasher.update(y.to_le_bytes());
        }
        for x in &self.parents {
            hasher.update(x);
        }
        Digest::new(hasher.finalize().as_slice()[..32].try_into().unwrap())
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
            self.payload.keys().map(|x| x.size()).sum::<usize>(),
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
    pub id: Digest,
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
            id: header.id.clone(),
            round: header.round,
            origin: header.author.clone(),
            author: author.clone(),
            signature: PublicKey::Sig::default(),
        };
        let signature = signature_service.request_signature(vote.digest()).await;
        Self { signature, ..vote }
    }

    pub fn verify(&self, committee: &Committee<PublicKey>) -> DagResult<()> {
        // Ensure the authority has voting rights.
        ensure!(
            committee.stake(&self.author) > 0,
            DagError::UnknownAuthority(self.author.encode_base64())
        );

        // Check the signature.
        self.author
            .verify(self.digest().as_ref(), &self.signature)
            .map_err(DagError::from)
    }
}

impl<PublicKey: VerifyingKey> Hash for Vote<PublicKey> {
    fn digest(&self) -> Digest {
        let mut hasher = Sha512::new();
        hasher.update(&self.id);
        hasher.update(self.round.to_le_bytes());
        hasher.update(&self.origin);
        Digest::new(hasher.finalize().as_slice()[..32].try_into().unwrap())
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
        PublicKey::verify_batch(self.digest().as_ref(), &pks, &sigs).map_err(DagError::from)
    }

    pub fn round(&self) -> Round {
        self.header.round
    }

    pub fn origin(&self) -> PublicKey {
        self.header.author.clone()
    }
}

impl<PublicKey: VerifyingKey> Hash for Certificate<PublicKey> {
    fn digest(&self) -> Digest {
        let mut hasher = Sha512::new();
        hasher.update(&self.header.id);
        hasher.update(self.round().to_le_bytes());
        hasher.update(&self.origin());
        Digest::new(hasher.finalize().as_slice()[..32].try_into().unwrap())
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
