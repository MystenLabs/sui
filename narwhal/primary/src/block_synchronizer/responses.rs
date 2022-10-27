// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{Committee, SharedWorkerCache};
use crypto::PublicKey;
use fastcrypto::hash::{Digest, Hash, HashFunction};
use std::fmt::{Debug, Display, Formatter};
use thiserror::Error;
use tracing::{error, warn};
use types::{Certificate, CertificateDigest};

// RequestID helps us identify an incoming request and
// all the consequent network requests associated with it.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestID(pub [u8; crypto::DIGEST_LENGTH]);

impl RequestID {
    // Create a request key (deterministically) from arbitrary data.
    pub fn new(data: &[u8]) -> Self {
        RequestID(crypto::DefaultHashFunction::digest(data).into())
    }
}

impl Debug for RequestID {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", base64::encode(self.0))
    }
}

impl FromIterator<CertificateDigest> for RequestID {
    fn from_iter<T: IntoIterator<Item = CertificateDigest>>(ids: T) -> Self {
        let mut ids_sorted: Vec<CertificateDigest> = ids.into_iter().collect();
        ids_sorted.sort();

        let result: Vec<u8> = ids_sorted
            .into_iter()
            .flat_map(|d| Digest::from(d).to_vec())
            .collect();

        RequestID::new(&result)
    }
}

impl<'a> FromIterator<&'a Certificate> for RequestID {
    fn from_iter<T: IntoIterator<Item = &'a Certificate>>(certificates: T) -> Self {
        certificates.into_iter().map(|c| c.digest()).collect()
    }
}

impl Display for RequestID {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", base64::encode(self.0))
    }
}

#[derive(Debug, Clone)]
pub struct CertificatesResponse {
    pub certificates: Vec<(CertificateDigest, Option<Certificate>)>,
    pub from: PublicKey,
}

impl CertificatesResponse {
    pub fn request_id(&self) -> RequestID {
        self.certificates.iter().map(|entry| entry.0).collect()
    }

    /// This method does two things:
    /// 1) filters only the found certificates
    /// 2) validates the certificates
    /// Even if one found certificate is not valid, an error is returned. Otherwise
    /// and Ok result is returned with (any) found certificates.
    pub fn validate_certificates(
        &self,
        committee: &Committee,
        worker_cache: SharedWorkerCache,
    ) -> Result<Vec<Certificate>, CertificatesResponseError> {
        let peer_found_certs: Vec<Certificate> = self
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

        let invalid_certificates: Vec<Certificate> = peer_found_certs
            .clone()
            .into_iter()
            .filter(|c| {
                if let Err(err) = c.verify(committee, worker_cache.clone()) {
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

#[derive(Debug, Clone)]
pub enum AvailabilityResponse {
    Certificate(CertificatesResponse),
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum CertificatesResponseError {
    #[error("Found invalid certificates form peer {name} - potentially Byzantine.")]
    ValidationError {
        name: PublicKey,
        invalid_certificates: Vec<Certificate>,
    },
}
