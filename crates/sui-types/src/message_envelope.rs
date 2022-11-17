// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::AuthorityName;
use crate::committee::{Committee, EpochId};
use crate::crypto::{
    AuthorityQuorumSignInfo, AuthoritySignInfo, AuthoritySignInfoTrait, AuthoritySignature,
    AuthorityStrongQuorumSignInfo, EmptySignInfo, Signable,
};
use crate::error::SuiResult;
use crate::indirect_validity::IndirectValidity;
use once_cell::sync::OnceCell;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fmt::{Debug, Display, Formatter};
use std::ops::Deref;

pub trait Message {
    type DigestType: Clone + Debug;

    fn digest(&self) -> Self::DigestType;

    /// Verify the internal data consistency of this message.
    /// In some cases, such as user signed transaction, we also need
    /// to verify the user signature here.
    fn verify(&self) -> SuiResult;
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct Envelope<T: Message, S> {
    #[serde(skip)]
    digest: OnceCell<T::DigestType>,

    data: T,
    auth_signature: S,
}

impl<T: Message, S> Envelope<T, S> {
    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn into_data(self) -> T {
        self.data
    }

    pub fn into_data_and_sig(self) -> (T, S) {
        let Self {
            data,
            auth_signature,
            ..
        } = self;
        (data, auth_signature)
    }

    /// Remove the authority signatures `S` from this envelope.
    pub fn into_unsigned(self) -> Envelope<T, EmptySignInfo> {
        Envelope::<T, EmptySignInfo>::new(self.into_data())
    }

    pub fn auth_sig(&self) -> &S {
        &self.auth_signature
    }

    pub fn digest(&self) -> &T::DigestType {
        self.digest.get_or_init(|| self.data.digest())
    }

    pub fn data_mut_for_testing(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<T: Message + PartialEq, S: PartialEq> PartialEq for Envelope<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data && self.auth_signature == other.auth_signature
    }
}

impl<T: Message> Envelope<T, EmptySignInfo> {
    pub fn new(data: T) -> Self {
        Self {
            digest: OnceCell::new(),
            data,
            auth_signature: EmptySignInfo {},
        }
    }

    pub fn verify_signature(&self) -> SuiResult {
        self.data.verify()
    }

    pub fn verify(self) -> SuiResult<VerifiedEnvelope<T, EmptySignInfo>> {
        self.verify_signature()?;
        Ok(VerifiedEnvelope::<T, EmptySignInfo>::new_from_verified(
            self,
        ))
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
        let auth_signature = AuthoritySignInfo::new(epoch, &data, authority, secret);
        Self {
            digest: OnceCell::new(),
            data,
            auth_signature,
        }
    }

    pub fn epoch(&self) -> EpochId {
        self.auth_signature.epoch
    }

    pub fn verify_signature(&self, committee: &Committee) -> SuiResult {
        self.data.verify()?;
        self.auth_signature.verify(self.data(), committee)
    }

    pub fn verify(
        self,
        committee: &Committee,
    ) -> SuiResult<VerifiedEnvelope<T, AuthoritySignInfo>> {
        self.verify_signature(committee)?;
        Ok(VerifiedEnvelope::<T, AuthoritySignInfo>::new_from_verified(
            self,
        ))
    }
}

impl<T, const S: bool> Envelope<T, AuthorityQuorumSignInfo<S>>
where
    T: Message + Signable<Vec<u8>>,
{
    pub fn new(
        data: T,
        signatures: Vec<AuthoritySignInfo>,
        committee: &Committee,
    ) -> SuiResult<Self> {
        let cert = Self {
            digest: OnceCell::new(),
            data,
            auth_signature: AuthorityQuorumSignInfo::<S>::new_from_auth_sign_infos(
                signatures, committee,
            )?,
        };

        Ok(cert)
    }

    pub fn epoch(&self) -> EpochId {
        self.auth_signature.epoch
    }

    // TODO: Eventually we should remove all calls to verify_signature
    // and make sure they all call verify to avoid repeated verifications.
    pub fn verify_signature(&self, committee: &Committee) -> SuiResult {
        self.data.verify()?;
        self.auth_signature.verify(self.data(), committee)
    }

    pub fn verify(
        self,
        committee: &Committee,
    ) -> SuiResult<VerifiedEnvelope<T, AuthorityQuorumSignInfo<S>>> {
        self.verify_signature(committee)?;
        Ok(VerifiedEnvelope::<T, AuthorityQuorumSignInfo<S>>::new_from_verified(self))
    }
}

/// TrustedEnvelope is a serializable wrapper around Envelope which is
/// Into<VerifiedEnvelope> - in other words it models a verified message which has been
/// written to the db (or some other trusted store), and may be read back from the db without
/// further signature verification.
///
/// TrustedEnvelope should *only* appear in database interfaces.
///
/// DO NOT USE in networked APIs.
///
/// Because it is used very sparingly, it can be audited easily: Use rust-analyzer,
/// or run: git grep -E 'TrustedEnvelope'
///
/// And verify that none of the uses appear in any network APIs.
#[derive(Clone, Serialize, Deserialize)]
pub struct TrustedEnvelope<T: Message, S>(Envelope<T, S>);

impl<T, S: Debug> Debug for TrustedEnvelope<T, S>
where
    T: Message + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl<T: Message, S> TrustedEnvelope<T, S> {
    pub fn into_inner(self) -> Envelope<T, S> {
        self.0
    }
}

// An empty marker struct that can't be serialized.
#[derive(Clone)]
struct NoSer;
// Never remove this assert!
static_assertions::assert_not_impl_any!(NoSer: Serialize, DeserializeOwned);

#[derive(Clone)]
pub struct VerifiedEnvelope<T: Message, S>(TrustedEnvelope<T, S>, NoSer);

impl<T, S: Debug> Debug for VerifiedEnvelope<T, S>
where
    T: Message + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0 .0)
    }
}

impl<T: Message, S> VerifiedEnvelope<T, S> {
    /// This API should only be called when the input is already verified.
    pub fn new_from_verified(inner: Envelope<T, S>) -> Self {
        Self(TrustedEnvelope(inner), NoSer)
    }

    /// There are some situations (e.g. fragment verification) where its very awkward and/or
    /// inefficient to obtain verified certificates from calling CertifiedTransaction::verify()
    /// Use this carefully.
    pub fn new_unchecked(inner: Envelope<T, S>) -> Self {
        Self(TrustedEnvelope(inner), NoSer)
    }

    pub fn into_inner(self) -> Envelope<T, S> {
        self.0 .0
    }

    pub fn into_message(self) -> T {
        self.into_inner().into_data()
    }

    /// Use this when you need to serialize a verified envelope.
    /// This should generally only be used for database writes.
    /// ***never use over the network!***
    pub fn serializable_ref(&self) -> &TrustedEnvelope<T, S> {
        &self.0
    }

    /// Use this when you need to serialize a verified envelope.
    /// This should generally only be used for database writes.
    /// ***never use over the network!***
    pub fn serializable(self) -> TrustedEnvelope<T, S> {
        self.0
    }

    /// Remove the authority signatures `S` from this envelope.
    pub fn into_unsigned(self) -> VerifiedEnvelope<T, EmptySignInfo> {
        VerifiedEnvelope::<T, EmptySignInfo>::new_from_verified(self.into_inner().into_unsigned())
    }
}

/// After deserialization, a TrustedTransactionEnvelope can be turned back into a
/// VerifiedTransactionEnvelope.
impl<T: Message, S> From<TrustedEnvelope<T, S>> for VerifiedEnvelope<T, S> {
    fn from(e: TrustedEnvelope<T, S>) -> Self {
        Self::new_unchecked(e.0)
    }
}

impl<T: Message, S> Deref for VerifiedEnvelope<T, S> {
    type Target = Envelope<T, S>;
    fn deref(&self) -> &Self::Target {
        &self.0 .0
    }
}

impl<T: Message, S> Deref for Envelope<T, S> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: Message, S> From<VerifiedEnvelope<T, S>> for Envelope<T, S> {
    fn from(v: VerifiedEnvelope<T, S>) -> Self {
        v.0 .0
    }
}

impl<T: Message, S> PartialEq for VerifiedEnvelope<T, S>
where
    Envelope<T, S>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.0 .0 == other.0 .0
    }
}

impl<T: Message, S> Eq for VerifiedEnvelope<T, S> where Envelope<T, S>: Eq {}

impl<T, S> Display for VerifiedEnvelope<T, S>
where
    T: Message,
    Envelope<T, S>: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0 .0)
    }
}

impl<T: Message> Envelope<T, IndirectValidity> {
    pub fn new(data: T, validity: IndirectValidity) -> Self {
        Self {
            digest: OnceCell::new(),
            data,
            auth_signature: validity,
        }
    }
}

// Note: There are many cases where its okay to construct an Envelope with IndirectValidity
// from AuthorityWeakQuorumSignInfo, including effects, checkpoint summaries, etc, which in
// general only require that one honest validator has attested to it. But, we only offer a blanket
// implementation for AuthorityStrongQuorumSignInfo to avoid accidentally promoting a case where
// AuthorityWeakQuorumSignInfo is insufficient, such as a CertifiedTransaction.
//
// Cases where AuthorityWeakQuorumSignInfo is sufficient should all be special cased.
impl<T: Message> From<Envelope<T, AuthorityStrongQuorumSignInfo>>
    for Envelope<T, IndirectValidity>
{
    fn from(env: Envelope<T, AuthorityStrongQuorumSignInfo>) -> Envelope<T, IndirectValidity> {
        Envelope::<T, IndirectValidity>::new(
            env.data,
            IndirectValidity::from_certified(env.auth_signature.epoch),
        )
    }
}
