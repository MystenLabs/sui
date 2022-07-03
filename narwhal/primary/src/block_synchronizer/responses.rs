// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use blake2::digest::Update;
use config::Committee;
use crypto::{traits::VerifyingKey, Digest, Hash};
use std::fmt::{Display, Formatter};
use thiserror::Error;
use tracing::{error, warn};
use types::{Certificate, CertificateDigest};

// RequestID helps us identify an incoming request and
// all the consequent network requests associated with it.
#[derive(Clone, Debug, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestID(pub [u8; crypto::DIGEST_LEN]);

impl RequestID {
    // Create a request key (deterministically) from arbitrary data.
    pub fn new(data: &[u8]) -> Self {
        RequestID(crypto::blake2b_256(|hasher| hasher.update(data)))
    }
}

impl<'a> FromIterator<&'a CertificateDigest> for RequestID {
    fn from_iter<T: IntoIterator<Item = &'a CertificateDigest>>(ids: T) -> Self {
        let mut ids_sorted: Vec<&CertificateDigest> = ids.into_iter().collect();
        ids_sorted.sort();

        let result: Vec<u8> = ids_sorted
            .into_iter()
            .flat_map(|d| Digest::from(*d).to_vec())
            .collect();

        RequestID::new(&result)
    }
}

impl<'a, PublicKey: VerifyingKey> FromIterator<&'a Certificate<PublicKey>> for RequestID {
    fn from_iter<T: IntoIterator<Item = &'a Certificate<PublicKey>>>(certificates: T) -> Self {
        let ids: Vec<CertificateDigest> = certificates.into_iter().map(|c| c.digest()).collect();

        ids.iter().collect()
    }
}

impl Display for RequestID {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", base64::encode(&self.0))
    }
}

#[derive(Debug, Clone)]
pub struct PayloadAvailabilityResponse<PublicKey: VerifyingKey> {
    pub block_ids: Vec<(CertificateDigest, bool)>,
    pub from: PublicKey,
}

impl<PublicKey: VerifyingKey> PayloadAvailabilityResponse<PublicKey> {
    pub fn request_id(&self) -> RequestID {
        let ids: Vec<CertificateDigest> = self.block_ids.iter().map(|entry| entry.0).collect();

        ids.iter().collect()
    }

    pub fn available_block_ids(&self) -> Vec<CertificateDigest> {
        self.block_ids
            .iter()
            .filter_map(|(id, available)| available.then_some(*id))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct CertificatesResponse<PublicKey: VerifyingKey> {
    pub certificates: Vec<(CertificateDigest, Option<Certificate<PublicKey>>)>,
    pub from: PublicKey,
}

impl<PublicKey: VerifyingKey> CertificatesResponse<PublicKey> {
    pub fn request_id(&self) -> RequestID {
        let ids: Vec<CertificateDigest> = self.certificates.iter().map(|entry| entry.0).collect();

        ids.iter().collect()
    }

    /// This method does two things:
    /// 1) filters only the found certificates
    /// 2) validates the certificates
    /// Even if one found certificate is not valid, an error is returned. Otherwise
    /// and Ok result is returned with (any) found certificates.
    pub fn validate_certificates(
        &self,
        committee: &Committee<PublicKey>,
    ) -> Result<Vec<Certificate<PublicKey>>, CertificatesResponseError<PublicKey>> {
        let peer_found_certs: Vec<Certificate<PublicKey>> = self
            .certificates
            .iter()
            .filter_map(|e| e.1.clone())
            .collect();

        if peer_found_certs.as_slice().is_empty() {
            // no certificates found, skip
            warn!(
                "No certificates are able to be served from {:?}",
                &self.from
            );
            return Ok(vec![]);
        }

        let invalid_certificates: Vec<Certificate<PublicKey>> = peer_found_certs
            .clone()
            .into_iter()
            .filter(|c| {
                if let Err(err) = c.verify(committee) {
                    error!(
                        "Certificate verification failed for id {} with error {:?}",
                        c.digest(),
                        err
                    );
                    return true;
                }
                false
            })
            .collect();

        if !invalid_certificates.is_empty() {
            error!("Found at least one invalid certificate from peer {:?}. Will ignore all certificates", self.from);

            return Err(CertificatesResponseError::ValidationError {
                name: self.from.clone(),
                invalid_certificates,
            });
        }

        Ok(peer_found_certs)
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum CertificatesResponseError<PublicKey: VerifyingKey> {
    #[error("Found invalid certificates form peer {name} - potentially Byzantine.")]
    ValidationError {
        name: PublicKey,
        invalid_certificates: Vec<Certificate<PublicKey>>,
    },
}
