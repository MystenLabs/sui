// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    error::{DagError, DagResult},
    messages::{Certificate, CertificateDigest, Header, Vote},
};
use config::{Committee, Stake};
use crypto::{
    traits::{EncodeDecodeBase64, VerifyingKey},
    Hash as _,
};
use std::collections::HashSet;

/// Aggregates votes for a particular header into a certificate.
pub struct VotesAggregator<PublicKey: VerifyingKey> {
    weight: Stake,
    votes: Vec<(PublicKey, PublicKey::Sig)>,
    used: HashSet<PublicKey>,
}

impl<PublicKey: VerifyingKey> VotesAggregator<PublicKey> {
    pub fn new() -> Self {
        Self {
            weight: 0,
            votes: Vec::new(),
            used: HashSet::new(),
        }
    }

    pub fn append(
        &mut self,
        vote: Vote<PublicKey>,
        committee: &Committee<PublicKey>,
        header: &Header<PublicKey>,
    ) -> DagResult<Option<Certificate<PublicKey>>> {
        let author = vote.author;

        // Ensure it is the first time this authority votes.
        ensure!(
            self.used.insert(author.clone()),
            DagError::AuthorityReuse(author.encode_base64())
        );

        self.votes.push((author.clone(), vote.signature));
        self.weight += committee.stake(&author);
        if self.weight >= committee.quorum_threshold() {
            self.weight = 0; // Ensures quorum is only reached once.
            return Ok(Some(Certificate {
                header: header.clone(),
                votes: self.votes.clone(),
            }));
        }
        Ok(None)
    }
}

/// Aggregate certificates and check if we reach a quorum.
pub struct CertificatesAggregator<PublicKey: VerifyingKey> {
    weight: Stake,
    certificates: Vec<CertificateDigest>,
    used: HashSet<PublicKey>,
}

impl<PublicKey: VerifyingKey> CertificatesAggregator<PublicKey> {
    pub fn new() -> Self {
        Self {
            weight: 0,
            certificates: Vec::new(),
            used: HashSet::new(),
        }
    }

    pub fn append(
        &mut self,
        certificate: Certificate<PublicKey>,
        committee: &Committee<PublicKey>,
    ) -> Option<Vec<CertificateDigest>> {
        let origin = certificate.origin();

        // Ensure it is the first time this authority votes.
        if !self.used.insert(origin.clone()) {
            return None;
        }

        self.certificates.push(certificate.digest());
        self.weight += committee.stake(&origin);
        if self.weight >= committee.quorum_threshold() {
            self.weight = 0; // Ensures quorum is only reached once.
            return Some(self.certificates.drain(..).collect());
        }
        None
    }
}
