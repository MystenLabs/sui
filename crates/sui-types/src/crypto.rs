// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::base_types::{AuthorityName, SuiAddress};
use crate::committee::{Committee, EpochId};
use crate::error::{SuiError, SuiResult};
use crate::sui_serde::Base64;
use crate::sui_serde::Readable;
use anyhow::Error;
use base64ct::Encoding;
use digest::Digest;
use ed25519_dalek as dalek;
use ed25519_dalek::Verifier;
use narwhal_crypto::ed25519::{Ed25519KeyPair, Ed25519PublicKey, Ed25519Signature};
use narwhal_crypto::traits::{
    AggregateAuthenticator, Authenticator, KeyPair as NarwhalKeypair, SigningKey, ToFromBytes,
    VerifyingKey,
};
use rand::rngs::OsRng;
use schemars::JsonSchema;
use serde::{de, Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::Bytes;
use sha3::Sha3_256;
use signature::Signature as NativeSignature;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

// Question: Should we change this to newtype?

pub type KeyPair = Ed25519KeyPair;

pub type PublicKey = <KeyPair as NarwhalKeypair>::PubKey;
pub type PrivateKey = <KeyPair as NarwhalKeypair>::PrivKey;

// Signatures for Authorities
pub type AuthoritySignature = <<KeyPair as NarwhalKeypair>::PubKey as VerifyingKey>::Sig;
pub type AggregateAuthoritySignature =
    <<<KeyPair as NarwhalKeypair>::PubKey as VerifyingKey>::Sig as Authenticator>::AggregateSig;

// Signatures for Users
pub type AccountSignature = <<KeyPair as NarwhalKeypair>::PubKey as VerifyingKey>::Sig;
pub type AggregateAccountSignature =
    <<<KeyPair as NarwhalKeypair>::PubKey as VerifyingKey>::Sig as Authenticator>::AggregateSig;

pub trait SuiKeypair {
    const PUBLIC_KEY_LENGTH: usize;
    const SECRET_KEY_LENGTH: usize;
    const SIGNATURE_LENGTH: usize;

    fn public_key_bytes(&self) -> PublicKeyBytes;
    fn make_narwhal_keypair(&self) -> KeyPair;
    fn copy(&self) -> KeyPair;
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error>
    where
        Self: Sized;
}

impl SuiKeypair for KeyPair {
    const PUBLIC_KEY_LENGTH: usize = ed25519_dalek::PUBLIC_KEY_LENGTH;
    const SECRET_KEY_LENGTH: usize = ed25519_dalek::SECRET_KEY_LENGTH;
    const SIGNATURE_LENGTH: usize = ed25519_dalek::SIGNATURE_LENGTH;

    fn public_key_bytes(&self) -> PublicKeyBytes {
        // self.public_key_cell.get_or_init(|| {
        let pk_arr: [u8; ed25519_dalek::PUBLIC_KEY_LENGTH] = self.name.0.to_bytes();
        PublicKeyBytes(pk_arr)
        // })
    }

    // TODO: eradicate unneeded uses (i.e. most)
    /// Avoid implementing `clone` on secret keys to prevent mistakes.
    #[must_use]
    fn copy(&self) -> KeyPair {
        KeyPair {
            name: self.public().clone(),
            secret: <KeyPair as NarwhalKeypair>::PrivKey::from_bytes(self.private().as_ref())
                .unwrap(),
        }
    }

    /// Make a Narwhal-compatible key pair from a Sui keypair.
    fn make_narwhal_keypair(&self) -> Ed25519KeyPair {
        let key = (*self).copy();
        key
    }

    fn from_bytes(bytes: &[u8]) -> Result<KeyPair, signature::Error> {
        let public_key_bytes = &bytes[..Self::PUBLIC_KEY_LENGTH];
        let secret_key_bytes = &bytes[Self::PUBLIC_KEY_LENGTH..];
        Ok(KeyPair {
            name: <KeyPair as NarwhalKeypair>::PubKey::from_bytes(public_key_bytes)?,
            secret: <KeyPair as NarwhalKeypair>::PrivKey::from_bytes(secret_key_bytes)?,
        })
    }
}

pub trait SuiSignature {
    const SIGNATURE_LENGTH: usize;
}

impl SuiSignature for AccountSignature {
    const SIGNATURE_LENGTH: usize = ed25519_dalek::SIGNATURE_LENGTH;
}

pub trait SuiAuthoritySignature {
    fn new<T>(value: &T, secret: &dyn signature::Signer<Ed25519Signature>) -> Self
    where
        T: Signable<Vec<u8>>;
    fn verify<T>(&self, value: &T, author: PublicKeyBytes) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>;
}

impl SuiAuthoritySignature for AuthoritySignature {
    fn new<T>(value: &T, secret: &dyn signature::Signer<Ed25519Signature>) -> Self
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        secret.sign(&message)
    }

    fn verify<T>(&self, value: &T, author: PublicKeyBytes) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // is this a cryptographically valid public Key?
        let public_key: PublicKey = author.try_into()?;

        // serialize the message (see BCS serialization for determinism)
        let mut message = Vec::new();
        value.write(&mut message);

        // perform cryptographic signature check
        public_key
            .verify(&message, &self)
            .map_err(|error| SuiError::InvalidSignature {
                error: error.to_string(),
            })
    }
}

impl signature::Signer<Signature> for KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        let signature_bytes: <<KeyPair as NarwhalKeypair>::PrivKey as SigningKey>::Sig =
            self.try_sign(msg)?;
        let pk_bytes = self.public_key_bytes();
        let public_key_bytes = pk_bytes.as_ref();
        let mut result_bytes = [0u8; SUI_SIGNATURE_LENGTH];
        result_bytes[..KeyPair::SIGNATURE_LENGTH].copy_from_slice(signature_bytes.as_ref());
        result_bytes[KeyPair::SIGNATURE_LENGTH..].copy_from_slice(public_key_bytes);
        Ok(Signature(result_bytes))
    }
}

#[serde_as]
#[derive(
    Eq, Default, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct PublicKeyBytes(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; dalek::PUBLIC_KEY_LENGTH],
);

impl PublicKeyBytes {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
    /// Make a Narwhal-compatible public key from a Sui pub.
    pub fn make_narwhal_public_key(&self) -> Result<Ed25519PublicKey, signature::Error> {
        let pub_key = dalek::PublicKey::from_bytes(&self.0)?;
        Ok(Ed25519PublicKey(pub_key))
    }
}

impl AsRef<[u8]> for PublicKeyBytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl TryInto<Ed25519PublicKey> for PublicKeyBytes {
    type Error = SuiError;

    fn try_into(self) -> Result<Ed25519PublicKey, Self::Error> {
        // TODO(https://github.com/MystenLabs/sui/issues/101): Do better key validation
        // to ensure the bytes represent a poin on the curve.
        Ed25519PublicKey::from_bytes(self.as_ref()).map_err(|_| SuiError::InvalidAuthenticator)
    }
}

// TODO(https://github.com/MystenLabs/sui/issues/101): more robust key validation
impl TryFrom<&[u8]> for PublicKeyBytes {
    type Error = SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, SuiError> {
        let arr: [u8; dalek::PUBLIC_KEY_LENGTH] = bytes
            .try_into()
            .map_err(|_| SuiError::InvalidAuthenticator)?;
        Ok(Self(arr))
    }
}

impl std::fmt::Debug for PublicKeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = hex::encode(&self.0);
        write!(f, "k#{}", s)?;
        Ok(())
    }
}

pub fn random_key_pairs(num: usize) -> Vec<KeyPair> {
    let mut items = num;
    let mut rng = OsRng;

    std::iter::from_fn(|| {
        if items == 0 {
            None
        } else {
            items -= 1;
            Some(get_key_pair_from_rng(&mut rng).1)
        }
    })
    .collect::<Vec<_>>()
}

// TODO: get_key_pair() and get_key_pair_from_bytes() should return KeyPair only.
// TODO: rename to random_key_pair
pub fn get_key_pair() -> (SuiAddress, KeyPair) {
    get_key_pair_from_rng(&mut OsRng)
}

/// Generate a keypair from the specified RNG (useful for testing with seedable rngs).
pub fn get_key_pair_from_rng<R>(csprng: &mut R) -> (SuiAddress, KeyPair)
where
    R: rand::CryptoRng + rand::RngCore,
{
    let kp = KeyPair::generate(csprng);
    (SuiAddress::from(kp.public_key_bytes()), kp)
}

// TODO: C-GETTER
pub fn get_key_pair_from_bytes(bytes: &[u8]) -> SuiResult<(SuiAddress, KeyPair)> {
    let kp = KeyPair::from_bytes(bytes).map_err(|e| SuiError::InvalidKeypair {
        error: e.to_string(),
    })?;
    Ok((SuiAddress::from(kp.public_key_bytes()), kp))
}

// TODO: replace this with a byte interpretation based on multicodec
pub const SUI_SIGNATURE_LENGTH: usize = KeyPair::SIGNATURE_LENGTH + KeyPair::PUBLIC_KEY_LENGTH;

#[serde_as]
#[derive(Eq, PartialEq, Copy, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Signature(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; SUI_SIGNATURE_LENGTH],
);

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl signature::Signature for Signature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let val: [u8; SUI_SIGNATURE_LENGTH] =
            bytes.try_into().map_err(|_| signature::Error::new())?;
        Ok(Self(val))
    }
}

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = base64ct::Base64::encode_string(self.signature_bytes());
        let p = base64ct::Base64::encode_string(self.public_key_bytes());
        write!(f, "{s}@{p}")?;
        Ok(())
    }
}

impl Signature {
    pub fn new<T>(value: &T, secret: &dyn signature::Signer<Signature>) -> Self
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        secret.sign(&message)
    }

    pub fn new_empty() -> Self {
        Self([0u8; SUI_SIGNATURE_LENGTH])
    }

    pub fn signature_bytes(&self) -> &[u8] {
        &self.0[..AccountSignature::SIGNATURE_LENGTH]
    }

    pub fn public_key_bytes(&self) -> &[u8] {
        &self.0[AccountSignature::SIGNATURE_LENGTH..]
    }

    /// This performs signature verification on the passed-in signature, additionally checking
    /// that the signature was performed with a PublicKey belonging to an expected author, indicated by its Sui Address
    pub fn verify<T>(&self, value: &T, author: SuiAddress) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        let (message, signature, public_key_bytes) = self.get_verification_inputs(value, author)?;

        // is this a cryptographically correct public key?
        // TODO: perform stricter key validation, sp. small order points, see https://github.com/MystenLabs/sui/issues/101
        let public_key = <KeyPair as NarwhalKeypair>::PubKey::from_bytes(public_key_bytes.as_ref())
            .map_err(|err| SuiError::InvalidSignature {
                error: err.to_string(),
            })?;

        // perform cryptographic signature check
        public_key
            .verify(&message, &signature)
            .map_err(|error| SuiError::InvalidSignature {
                error: error.to_string(),
            })
    }

    pub fn get_verification_inputs<T>(
        &self,
        value: &T,
        author: SuiAddress,
    ) -> Result<(Vec<u8>, AccountSignature, PublicKeyBytes), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // Is this signature emitted by the expected author?
        let public_key_bytes: [u8; KeyPair::PUBLIC_KEY_LENGTH] = self
            .public_key_bytes()
            .try_into()
            .expect("byte lengths match");
        let received_addr = SuiAddress::from(&PublicKeyBytes(public_key_bytes));
        if received_addr != author {
            return Err(SuiError::IncorrectSigner {
                error: format!("Signature get_verification_inputs() failure. Author is {author}, received address is {received_addr}")
            });
        }

        // deserialize the signature
        let signature = AccountSignature::from_bytes(self.signature_bytes()).map_err(|err| {
            SuiError::InvalidSignature {
                error: err.to_string(),
            }
        })?;

        // serialize the message (see BCS serialization for determinism)
        let mut message = Vec::new();
        value.write(&mut message);

        Ok((message, signature, PublicKeyBytes(public_key_bytes)))
    }
}

/// AuthoritySignInfoTrait is a trait used specifically for a few structs in messages.rs
/// to template on whether the struct is signed by an authority. We want to limit how
/// those structs can be instanted on, hence the sealed trait.
/// TODO: We could also add the aggregated signature as another impl of the trait.
///       This will make CertifiedTransaction also an instance of the same struct.
pub trait AuthoritySignInfoTrait: private::SealedAuthoritySignInfoTrait {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EmptySignInfo {}
impl AuthoritySignInfoTrait for EmptySignInfo {}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct AuthoritySignInfo {
    pub epoch: EpochId,
    pub authority: AuthorityName,
    pub signature: AuthoritySignature,
}
impl AuthoritySignInfoTrait for AuthoritySignInfo {}

impl Hash for AuthoritySignInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch.hash(state);
        self.authority.hash(state);
    }
}

impl PartialEq for AuthoritySignInfo {
    fn eq(&self, other: &Self) -> bool {
        // We do not compare the signature, because there can be multiple
        // valid signatures for the same epoch and authority.
        self.epoch == other.epoch && self.authority == other.authority
    }
}

impl AuthoritySignInfo {
    pub fn add_to_verification_obligation(
        &self,
        committee: &Committee,
        obligation: &mut VerificationObligation<AggregateAuthoritySignature>,
        message_index: usize,
    ) -> SuiResult<()> {
        obligation
            .public_keys
            .get(message_index)
            .ok_or(SuiError::InvalidAddress)?
            .push(committee.public_key(&self.authority)?);
        obligation
            .signatures
            .get(message_index)
            .ok_or(SuiError::InvalidAddress)?
            .add_signature(self.signature.clone());
        Ok(())
    }
}

/// Represents at least a quorum (could be more) of authority signatures.
/// STRONG_THRESHOLD indicates whether to use the quorum threshold for quorum check.
/// When STRONG_THRESHOLD is true, the quorum is valid when the total stake is
/// at least the quorum threshold (2f+1) of the committee; when STRONG_THRESHOLD is false,
/// the quorum is valid when the total stake is at least the validity threshold (f+1) of
/// the committee.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuthorityQuorumSignInfo<const STRONG_THRESHOLD: bool> {
    pub epoch: EpochId,
    #[schemars(with = "Base64")]
    pub authorities: Vec<AuthorityName>,
    #[schemars(with = "Base64")]
    pub signature: AggregateAuthoritySignature
}

pub type AuthorityStrongQuorumSignInfo = AuthorityQuorumSignInfo<true>;
pub type AuthorityWeakQuorumSignInfo = AuthorityQuorumSignInfo<false>;

// Note: if you meet an error due to this line it may be because you need an Eq implementation for `CertifiedTransaction`,
// or one of the structs that include it, i.e. `ConfirmationTransaction`, `TransactionInfoResponse` or `ObjectInfoResponse`.
//
// Please note that any such implementation must be agnostic to the exact set of signatures in the certificate, as
// clients are allowed to equivocate on the exact nature of valid certificates they send to the system. This assertion
// is a simple tool to make sure certificates are accounted for correctly - should you remove it, you're on your own to
// maintain the invariant that valid certificates with distinct signatures are equivalent, but yet-unchecked
// certificates that differ on signers aren't.
//
// see also https://github.com/MystenLabs/sui/issues/266
// static_assertions::assert_not_impl_any!(AuthorityStrongQuorumSignInfo<S>: Hash, Eq, PartialEq);
// static_assertions::assert_not_impl_any!(AuthorityWeakQuorumSignInfo<S>: Hash, Eq, PartialEq);

impl<const S: bool> AuthoritySignInfoTrait for AuthorityQuorumSignInfo<S> {}

impl<const STRONG_THRESHOLD: bool> AuthorityQuorumSignInfo<STRONG_THRESHOLD> {
    pub fn add_to_verification_obligation(
        &self,
        committee: &Committee,
        obligation: &mut VerificationObligation<AggregateAuthoritySignature>,
        message_index: usize,
    ) -> SuiResult<()> {
        // Check epoch
        fp_ensure!(
            self.epoch == committee.epoch(),
            SuiError::WrongEpoch {
                expected_epoch: committee.epoch()
            }
        );

        let mut weight = 0;
        let mut used_authorities = HashSet::new();

        let pk_index = obligation.public_keys.len();
        obligation.public_keys.push(Vec::new());

        // Create obligations for the committee signatures
        for authority in self.authorities.iter() {
            // Check that each authority only appears once.
            fp_ensure!(
                !used_authorities.contains(authority),
                SuiError::CertificateAuthorityReuse
            );
            used_authorities.insert(*authority);
            // Update weight.
            let voting_rights = committee.weight(authority);
            fp_ensure!(voting_rights > 0, SuiError::UnknownSigner);
            weight += voting_rights;

            obligation
                .public_keys[pk_index]
                .push(committee.public_key(authority)?);
            obligation.signatures.push(signature.clone());
            obligation.message_index.push(message_index);
        }

        let threshold = if STRONG_THRESHOLD {
            committee.quorum_threshold()
        } else {
            committee.validity_threshold()
        };
        fp_ensure!(weight >= threshold, SuiError::CertificateRequiresQuorum);

        Ok(())
    }
}

mod private {
    pub trait SealedAuthoritySignInfoTrait {}
    impl SealedAuthoritySignInfoTrait for super::EmptySignInfo {}
    impl SealedAuthoritySignInfoTrait for super::AuthoritySignInfo {}
    impl<const S: bool> SealedAuthoritySignInfoTrait for super::AuthorityQuorumSignInfo<S> {}
}

/// Something that we know how to hash and sign.
pub trait Signable<W> {
    fn write(&self, writer: &mut W);
}
pub trait SignableBytes
where
    Self: Sized,
{
    fn from_signable_bytes(bytes: &[u8]) -> Result<Self, anyhow::Error>;
}
/// Activate the blanket implementation of `Signable` based on serde and BCS.
/// * We use `serde_name` to extract a seed from the name of structs and enums.
/// * We use `BCS` to generate canonical bytes suitable for hashing and signing.
pub trait BcsSignable: Serialize + serde::de::DeserializeOwned {}

impl<T, W> Signable<W> for T
where
    T: BcsSignable,
    W: std::io::Write,
{
    fn write(&self, writer: &mut W) {
        let name = serde_name::trace_name::<Self>().expect("Self must be a struct or an enum");
        // Note: This assumes that names never contain the separator `::`.
        write!(writer, "{}::", name).expect("Hasher should not fail");
        bcs::serialize_into(writer, &self).expect("Message serialization should not fail");
    }
}

impl<T> SignableBytes for T
where
    T: BcsSignable,
{
    fn from_signable_bytes(bytes: &[u8]) -> Result<Self, Error> {
        // Remove name tag before deserialization using BCS
        let name = serde_name::trace_name::<Self>().expect("Self must be a struct or an enum");
        let name_byte_len = format!("{}::", name).bytes().len();
        Ok(bcs::from_bytes(&bytes[name_byte_len..])?)
    }
}

pub type PubKeyLookup<P> = HashMap<PublicKeyBytes, P>;

pub fn sha3_hash<S: Signable<Sha3_256>>(signable: &S) -> [u8; 32] {
    let mut digest = Sha3_256::default();
    signable.write(&mut digest);
    let hash = digest.finalize();
    hash.into()
}

#[derive(Default)]
pub struct VerificationObligation<S>
where
    S: AggregateAuthenticator,
{
    lookup: PubKeyLookup<S::PubKey>,
    messages: Vec<Vec<u8>>,
    pub signatures: Vec<S>, // Change to AggregatedAuthenticator. Then make Ed25519Signature implement AggregatedAuthenticator.
    pub public_keys: Vec<Vec<S::PubKey>>,
}

impl<S: AggregateAuthenticator> VerificationObligation<S> 
    where PublicKeyBytes: TryInto<S::PubKey>
{
    pub fn new(lookup: PubKeyLookup<S::PubKey>) -> VerificationObligation<S> {
        VerificationObligation {
            lookup,
            ..Default::default()
        }
    }

    pub fn lookup_public_key(&mut self, key_bytes: &PublicKeyBytes) -> Result<S::PubKey, SuiError> {
        match self.lookup.get(key_bytes) {
            Some(v) => Ok(v.clone()),
            None => {
                let public_key: S::PubKey = (*key_bytes).try_into()
                    .map_err(|e| SuiError::InvalidAddress)?;
                self.lookup.insert(*key_bytes, public_key.clone());
                Ok(public_key)
            }
        }
    }

    /// Add a new message to the list of messages to be verified.
    /// Returns the index of the message.
    pub fn add_message(&mut self, message: Vec<u8>) {
        self.signatures.push(S::default());
        self.messages.push(message);
    }

    pub fn verify_all(self) -> SuiResult<PubKeyLookup<S::PubKey>> {
        S::batch_verify(
            &self.signatures.iter().map(|x| x).collect::<Vec<_>>()[..],
            &self
                .public_keys
                .iter()
                .map(|x| &x.iter().collect::<Vec<_>>()[..])
                .collect::<Vec<_>>()[..],
            &self
                .messages
                .iter()
                .map(|x| &x.iter().map(|&y| y).collect::<Vec<_>>()[..]).
                collect::<Vec<_>>()[..]
        )
        .map_err(|error| SuiError::InvalidSignature {
            error: format!("{error}"),
        })?;

        Ok(self.lookup)
    }
}
