// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;
use sui_types::committee::Committee;
use sui_types::crypto::AuthoritySignInfo;
use sui_types::error::SuiResult;
use sui_types::message_envelope::{Envelope, Message, VerifiedEnvelope};

pub trait SignatureVerifier: Sync + Send + Clone + 'static {
    fn verify_one<T: Message + Serialize>(
        &self,
        envelope: Envelope<T, AuthoritySignInfo>,
        committee: &Committee,
    ) -> SuiResult<VerifiedEnvelope<T, AuthoritySignInfo>>;
}

#[derive(Default, Clone)]
pub struct DefaultSignatureVerifier;

impl SignatureVerifier for DefaultSignatureVerifier {
    fn verify_one<T: Message + Serialize>(
        &self,
        envelope: Envelope<T, AuthoritySignInfo>,
        committee: &Committee,
    ) -> SuiResult<VerifiedEnvelope<T, AuthoritySignInfo>> {
        envelope.verify(committee)
    }
}

#[derive(Default, Clone)]
pub struct IgnoreSignatureVerifier;

impl SignatureVerifier for IgnoreSignatureVerifier {
    fn verify_one<T: Message + Serialize>(
        &self,
        envelope: Envelope<T, AuthoritySignInfo>,
        _committee: &Committee,
    ) -> SuiResult<VerifiedEnvelope<T, AuthoritySignInfo>> {
        Ok(VerifiedEnvelope::new_unchecked(envelope))
    }
}
