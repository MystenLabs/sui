// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::AuthorityName;
use crate::committee::{Committee, EpochId};
use crate::crypto::{
    AuthorityKeyPair, AuthorityQuorumSignInfo, AuthoritySignInfo, AuthoritySignInfoTrait,
    AuthoritySignature, AuthorityStrongQuorumSignInfo, EmptySignInfo, Signer,
};
use crate::error::SuiResult;
use crate::executable_transaction::CertificateProof;
use crate::messages_checkpoint::CheckpointSequenceNumber;
use crate::transaction::SenderSignedData;
use fastcrypto::traits::KeyPair;
use once_cell::sync::OnceCell;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_name::{DeserializeNameAdapter, SerializeNameAdapter};
use shared_crypto::intent::{Intent, IntentScope};
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

pub trait Message {
    type DigestType: Clone + Debug;
    const SCOPE: IntentScope;

    fn scope(&self) -> IntentScope {
        Self::SCOPE
    }

    fn digest(&self) -> Self::DigestType;
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
#[serde(remote = "Envelope")]
pub struct Envelope<T: Message, S> {
    #[serde(skip)]
    digest: OnceCell<T::DigestType>,

    data: T,
    auth_signature: S,
}

impl<'de, T, S> Deserialize<'de> for Envelope<T, S>
where
    T: Message + Deserialize<'de>,
    S: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Envelope::deserialize(DeserializeNameAdapter::new(
            deserializer,
            std::any::type_name::<Self>(),
        ))
    }
}

impl<T, Sig> Serialize for Envelope<T, Sig>
where
    T: Message + Serialize,
    Sig: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        Envelope::serialize(
            self,
            SerializeNameAdapter::new(serializer, std::any::type_name::<Self>()),
        )
    }
}

impl<T: Message, S> Envelope<T, S> {
    pub fn new_from_data_and_sig(data: T, sig: S) -> Self {
        Self {
            digest: Default::default(),
            data,
            auth_signature: sig,
        }
    }

    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn into_data(self) -> T {
        self.data
    }

    pub fn into_sig(self) -> S {
        self.auth_signature
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

    pub fn auth_sig_mut_for_testing(&mut self) -> &mut S {
        &mut self.auth_signature
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
}

impl<T> Envelope<T, AuthoritySignInfo>
where
    T: Message + Serialize,
{
    pub fn new(
        epoch: EpochId,
        data: T,
        secret: &dyn Signer<AuthoritySignature>,
        authority: AuthorityName,
    ) -> Self {
        let auth_signature = Self::sign(epoch, &data, secret, authority);
        Self {
            digest: OnceCell::new(),
            data,
            auth_signature,
        }
    }

    pub fn sign(
        epoch: EpochId,
        data: &T,
        secret: &dyn Signer<AuthoritySignature>,
        authority: AuthorityName,
    ) -> AuthoritySignInfo {
        AuthoritySignInfo::new(epoch, &data, Intent::sui_app(T::SCOPE), authority, secret)
    }

    pub fn epoch(&self) -> EpochId {
        self.auth_signature.epoch
    }
}

impl Envelope<SenderSignedData, AuthoritySignInfo> {
    pub fn verify_committee_sigs_only(&self, committee: &Committee) -> SuiResult {
        self.auth_signature.verify_secure(
            self.data(),
            Intent::sui_app(IntentScope::SenderSignedTransaction),
            committee,
        )
    }
}

impl<T, const S: bool> Envelope<T, AuthorityQuorumSignInfo<S>>
where
    T: Message + Serialize,
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

    pub fn new_from_keypairs_for_testing(
        data: T,
        keypairs: &[AuthorityKeyPair],
        committee: &Committee,
    ) -> Self {
        let signatures = keypairs
            .iter()
            .map(|keypair| {
                AuthoritySignInfo::new(
                    committee.epoch(),
                    &data,
                    Intent::sui_app(T::SCOPE),
                    keypair.public().into(),
                    keypair,
                )
            })
            .collect();
        Self::new(data, signatures, committee).unwrap()
    }

    pub fn epoch(&self) -> EpochId {
        self.auth_signature.epoch
    }
}

/// TrustedEnvelope is a serializable wrapper around Envelope which is
/// `Into<VerifiedEnvelope>` - in other words it models a verified message which has been
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

    pub fn inner(&self) -> &Envelope<T, S> {
        &self.0
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

    pub fn inner(&self) -> &Envelope<T, S> {
        &self.0 .0
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

impl<T: Message, S> DerefMut for Envelope<T, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
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

/// The following implementation provides two ways to construct a VerifiedEnvelope with CertificateProof.
/// It is implemented in this file such that we could reuse the digest without having to
/// recompute it.
/// We allow converting a VerifiedCertificate into a VerifiedEnvelope with CertificateProof::Certificate;
/// and converting a VerifiedTransaction along with checkpoint information into a VerifiedEnvelope
/// with CertificateProof::Checkpoint.
impl<T: Message> VerifiedEnvelope<T, CertificateProof> {
    pub fn new_from_certificate(
        certificate: VerifiedEnvelope<T, AuthorityStrongQuorumSignInfo>,
    ) -> Self {
        let inner = certificate.into_inner();
        let Envelope {
            digest,
            data,
            auth_signature,
        } = inner;
        VerifiedEnvelope::new_unchecked(Envelope {
            digest,
            data,
            auth_signature: CertificateProof::new_from_cert_sig(auth_signature),
        })
    }

    pub fn new_from_checkpoint(
        transaction: VerifiedEnvelope<T, EmptySignInfo>,
        epoch: EpochId,
        checkpoint: CheckpointSequenceNumber,
    ) -> Self {
        let inner = transaction.into_inner();
        let Envelope {
            digest,
            data,
            auth_signature: _,
        } = inner;
        VerifiedEnvelope::new_unchecked(Envelope {
            digest,
            data,
            auth_signature: CertificateProof::new_from_checkpoint(epoch, checkpoint),
        })
    }

    pub fn new_system(transaction: VerifiedEnvelope<T, EmptySignInfo>, epoch: EpochId) -> Self {
        let inner = transaction.into_inner();
        let Envelope {
            digest,
            data,
            auth_signature: _,
        } = inner;
        VerifiedEnvelope::new_unchecked(Envelope {
            digest,
            data,
            auth_signature: CertificateProof::new_system(epoch),
        })
    }

    pub fn new_from_quorum_execution(
        transaction: VerifiedEnvelope<T, EmptySignInfo>,
        epoch: EpochId,
    ) -> Self {
        let inner = transaction.into_inner();
        let Envelope {
            digest,
            data,
            auth_signature: _,
        } = inner;
        VerifiedEnvelope::new_unchecked(Envelope {
            digest,
            data,
            auth_signature: CertificateProof::QuorumExecuted(epoch),
        })
    }

    pub fn new_from_consensus(
        transaction: VerifiedEnvelope<T, EmptySignInfo>,
        epoch: EpochId,
    ) -> Self {
        let inner = transaction.into_inner();
        let Envelope {
            digest,
            data,
            auth_signature: _,
        } = inner;
        VerifiedEnvelope::new_unchecked(Envelope {
            digest,
            data,
            auth_signature: CertificateProof::new_from_consensus(epoch),
        })
    }

    pub fn epoch(&self) -> EpochId {
        self.auth_signature.epoch()
    }
}
