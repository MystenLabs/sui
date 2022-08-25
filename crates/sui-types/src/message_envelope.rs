// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::AuthorityName;
use crate::committee::{Committee, EpochId};
use crate::crypto::{
    AuthorityQuorumSignInfo, AuthoritySignInfo, AuthoritySignInfoTrait, AuthoritySignature,
    EmptySignInfo, Signable, SuiAuthoritySignature, VerificationObligation,
};
use crate::error::SuiResult;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

pub trait Message {
    type DigestType;

    fn digest(&self) -> Self::DigestType;

    /// Verify the internal data consistency of this message.
    /// In some cases, such as user signed transaction, we also need
    /// to verify the user signature here.
    fn verify(&self) -> SuiResult;

    /// This is only needed if this message contains signature that needs
    /// to be verified. In most messages this function can be a noop.
    fn add_to_verification_obligation(
        &self,
        obligation: &mut VerificationObligation,
    ) -> SuiResult<()>;
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct Envelope<T: Message, S> {
    #[serde(skip)]
    digest: OnceCell<T::DigestType>,
    #[serde(skip)]
    verified: bool,

    data: T,
    pub auth_signature: S,
}

impl<T: Message, S: AuthoritySignInfoTrait> Envelope<T, S> {
    pub fn into_data(self) -> T {
        self.data
    }
}

impl<T: Message + PartialEq, S: PartialEq> PartialEq for Envelope<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data && self.auth_signature == other.auth_signature
    }
}

impl<T, S> Envelope<T, S>
where
    T: Message + Signable<Vec<u8>>,
    S: AuthoritySignInfoTrait,
{
    pub fn digest(&self) -> &T::DigestType {
        self.digest.get_or_init(|| self.data.digest())
    }

    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn auth_sig(&self) -> &S {
        &self.auth_signature
    }

    pub fn is_verified(&self) -> bool {
        self.verified
    }

    /// A convenient interface to verify this message only.
    pub fn verify(&self, committee: &Committee) -> SuiResult {
        if !self.verified {
            self.data.verify()?;
            self.auth_signature.verify(&self.data, committee)?;
        }
        Ok(())
    }

    pub fn verify_mut(&mut self, committee: &Committee) -> SuiResult {
        self.verify(committee)?;
        self.verified = true;
        Ok(())
    }

    /// Add this message to `obligation` for verification.
    /// This allows batch verification. This message can be
    /// one of the many messages that need to be verified.
    pub fn add_to_verification_obligation(
        &self,
        committee: &Committee,
        obligation: &mut VerificationObligation,
    ) -> SuiResult<()> {
        self.data.add_to_verification_obligation(obligation)?;

        let idx = obligation.add_message(&self.data);
        self.auth_signature
            .add_to_verification_obligation(committee, obligation, idx)
    }
}

impl<T> Envelope<T, EmptySignInfo>
where
    T: Message + Signable<Vec<u8>>,
{
    pub fn from_signed<S: AuthoritySignInfoTrait>(envelope: Envelope<T, S>) -> Self {
        Self {
            digest: envelope.digest,
            data: envelope.data,
            auth_signature: EmptySignInfo {},
            verified: false,
        }
    }

    pub fn verify_user_sig(&self) -> SuiResult {
        if self.verified {
            return Ok(());
        }
        self.data.verify()
    }

    pub fn mut_verify_user_sig(&mut self) -> SuiResult {
        self.verify_user_sig()?;
        self.verified = true;
        Ok(())
    }

    pub fn new(data: T) -> Self {
        Self {
            digest: OnceCell::new(),
            data,
            auth_signature: EmptySignInfo {},
            verified: false,
        }
    }
}

impl<T> Envelope<T, AuthoritySignInfo>
where
    T: Message + Signable<Vec<u8>>,
{
    pub fn new(
        epoch: EpochId,
        data: T,
        secret: &dyn signature::Signer<AuthoritySignature>,
        authority: AuthorityName,
    ) -> Self {
        let signature = AuthoritySignature::new(&data, secret);
        Self {
            digest: OnceCell::new(),
            data,
            auth_signature: AuthoritySignInfo {
                epoch,
                authority,
                signature,
            },
            verified: false,
        }
    }
}

impl<T, const S: bool> Envelope<T, AuthorityQuorumSignInfo<S>>
where
    T: Message + Signable<Vec<u8>>,
{
    pub fn new<U>(
        unsigned_envelop: Envelope<T, U>,
        signatures: Vec<(AuthorityName, AuthoritySignature)>,
        committee: &Committee,
    ) -> SuiResult<Self> {
        let Envelope { digest, data, .. } = unsigned_envelop;
        let cert = Self {
            digest,
            data,
            auth_signature: AuthorityQuorumSignInfo::<S>::new_with_signatures(
                signatures.into_iter().map(|v| (v.0, v.1)).collect(),
                committee,
            )?,
            verified: false,
        };

        cert.verify(committee)?;
        Ok(cert)
    }

    pub fn new_empty<U>(
        unsigned_envelop: Envelope<T, U>,
        committee: &Committee,
    ) -> SuiResult<Self> {
        let Envelope { digest, data, .. } = unsigned_envelop;
        let cert = Self {
            digest,
            data,
            auth_signature: AuthorityQuorumSignInfo::new(committee.epoch),
            verified: false,
        };

        cert.data().verify()?;
        Ok(cert)
    }
}
