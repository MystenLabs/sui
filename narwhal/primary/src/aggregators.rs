// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::PrimaryMetrics;
use config::{AuthorityIdentifier, Committee, Stake};
use crypto::{
    to_intent_message, AggregateSignature, NarwhalAuthorityAggregateSignature,
    NarwhalAuthoritySignature, Signature,
};
use fastcrypto::hash::{Digest, Hash};
use std::collections::HashSet;
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use tracing::warn;
use types::{
    ensure,
    error::{DagError, DagResult},
    Certificate, CertificateAPI, Header, SignatureVerificationState, Vote, VoteAPI,
};

/// Aggregates votes for a particular header into a certificate.
pub struct VotesAggregator {
    protocol_config: ProtocolConfig,
    weight: Stake,
    votes: Vec<(AuthorityIdentifier, Signature)>,
    used: HashSet<AuthorityIdentifier>,
    metrics: Arc<PrimaryMetrics>,
}

impl VotesAggregator {
    pub fn new(protocol_config: &ProtocolConfig, metrics: Arc<PrimaryMetrics>) -> Self {
        metrics.votes_received_last_round.set(0);

        Self {
            protocol_config: protocol_config.clone(),
            weight: 0,
            votes: Vec::new(),
            used: HashSet::new(),
            metrics,
        }
    }

    pub fn append(
        &mut self,
        vote: Vote,
        committee: &Committee,
        header: &Header,
    ) -> DagResult<Option<Certificate>> {
        let author = vote.author();

        // Ensure it is the first time this authority votes.
        ensure!(
            self.used.insert(author),
            DagError::AuthorityReuse(author.to_string())
        );

        self.votes.push((author, vote.signature().clone()));
        self.weight += committee.stake_by_id(author);

        self.metrics
            .votes_received_last_round
            .set(self.votes.len() as i64);
        if self.weight >= committee.quorum_threshold() {
            let mut cert = Certificate::new_unverified(
                &self.protocol_config,
                committee,
                header.clone(),
                self.votes.clone(),
            )?;
            let (_, pks) = cert.signed_by(committee);

            let certificate_digest: Digest<{ crypto::DIGEST_LENGTH }> = Digest::from(cert.digest());
            match AggregateSignature::try_from(
                cert.aggregated_signature()
                    .ok_or(DagError::InvalidSignature)?,
            )
            .map_err(|_| DagError::InvalidSignature)?
            .verify_secure(&to_intent_message(certificate_digest), &pks[..])
            {
                Err(err) => {
                    warn!(
                        "Failed to verify aggregated sig on certificate: {} error: {}",
                        certificate_digest, err
                    );
                    self.votes.retain(|(id, sig)| {
                        let pk = committee.authority_safe(id).protocol_key();
                        if sig
                            .verify_secure(&to_intent_message(certificate_digest), pk)
                            .is_err()
                        {
                            warn!("Invalid signature on header from authority: {}", id);
                            self.weight -= committee.stake(pk);
                            false
                        } else {
                            true
                        }
                    });
                    return Ok(None);
                }
                Ok(_) => {
                    // TODO: Move this block and the AggregateSignature verification into Certificate
                    if self.protocol_config.narwhal_certificate_v2() {
                        cert.set_signature_verification_state(
                            SignatureVerificationState::VerifiedDirectly(
                                cert.aggregated_signature()
                                    .ok_or(DagError::InvalidSignature)?
                                    .clone(),
                            ),
                        );
                    }
                    return Ok(Some(cert));
                }
            }
        }
        Ok(None)
    }
}

/// Aggregate certificates and check if we reach a quorum.
pub struct CertificatesAggregator {
    weight: Stake,
    certificates: Vec<Certificate>,
    used: HashSet<AuthorityIdentifier>,
}

impl CertificatesAggregator {
    pub fn new() -> Self {
        Self {
            weight: 0,
            certificates: Vec::new(),
            used: HashSet::new(),
        }
    }

    pub fn append(
        &mut self,
        certificate: Certificate,
        committee: &Committee,
    ) -> Option<Vec<Certificate>> {
        let origin = certificate.origin();

        // Ensure it is the first time this authority votes.
        if !self.used.insert(origin) {
            return None;
        }

        self.certificates.push(certificate);
        self.weight += committee.stake_by_id(origin);
        if self.weight >= committee.quorum_threshold() {
            // Note that we do not reset the weight here. If this function is called again and
            // the proposer didn't yet advance round, we can add extra certificates as parents.
            // This is required when running Bullshark as consensus.
            return Some(self.certificates.drain(..).collect());
        }
        None
    }
}
