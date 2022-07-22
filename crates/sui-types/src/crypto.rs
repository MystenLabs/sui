// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use anyhow::Error;
use base64ct::Encoding;
use digest::Digest;
use narwhal_crypto::ed25519::{
    Ed25519AggregateSignature, Ed25519KeyPair, Ed25519PrivateKey, Ed25519PublicKey,
    Ed25519Signature,
};
pub use narwhal_crypto::traits::KeyPair as KeypairTraits;
pub use narwhal_crypto::traits::{
    AggregateAuthenticator, Authenticator, SigningKey, ToFromBytes, VerifyingKey,
};
use narwhal_crypto::Verifier;
use rand::rngs::OsRng;
use roaring::RoaringBitmap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};
use sha3::Sha3_256;

use crate::base_types::{AuthorityName, SuiAddress};
use crate::committee::{Committee, EpochId};
use crate::error::{SuiError, SuiResult};
use crate::sui_serde::{Base64, Readable, SuiBitmap};

// Comment the one you want to use
pub type KeyPair = Ed25519KeyPair; // Associated Types don't work here yet for some reason.
pub type PrivateKey = Ed25519PrivateKey;
pub type PublicKey = Ed25519PublicKey;

// Signatures for Authorities
pub type AuthoritySignature = Ed25519Signature;
pub type AggregateAuthoritySignature = Ed25519AggregateSignature;

// Signatures for Users
pub type AccountSignature = Ed25519Signature;
pub type AggregateAccountSignature = Ed25519AggregateSignature;

//
// Define Bytes representation of the Authority's PublicKey
//

#[serde_as]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PublicKeyBytes(#[serde_as(as = "Readable<Base64, Bytes>")] [u8; PublicKey::LENGTH]);

impl TryFrom<PublicKeyBytes> for PublicKey {
    type Error = signature::Error;

    fn try_from(bytes: PublicKeyBytes) -> Result<PublicKey, Self::Error> {
        PublicKey::from_bytes(bytes.as_ref()).map_err(|_| Self::Error::new())
    }
}

impl From<&PublicKey> for PublicKeyBytes {
    fn from(pk: &PublicKey) -> PublicKeyBytes {
        PublicKeyBytes::from_bytes(pk.as_ref()).unwrap()
    }
}

impl AsRef<[u8]> for PublicKeyBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl Display for PublicKeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = hex::encode(&self.0);
        write!(f, "k#{}", s)?;
        Ok(())
    }
}

impl ToFromBytes for PublicKeyBytes {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let bytes: [u8; PublicKey::LENGTH] =
            bytes.try_into().map_err(signature::Error::from_source)?;
        Ok(PublicKeyBytes(bytes))
    }
}

impl PublicKeyBytes {
    /// This ensures it's impossible to construct an instance with other than registered lengths
    pub fn new(bytes: [u8; PublicKey::LENGTH]) -> PublicKeyBytes
where {
        PublicKeyBytes(bytes)
    }

    // this is probably derivable, but we'd rather have it explicitly laid out for instructional purposes,
    // see [#34](https://github.com/MystenLabs/narwhal/issues/34)
    #[allow(dead_code)]
    fn default() -> Self {
        Self([0u8; PublicKey::LENGTH])
    }
}

impl FromStr for PublicKeyBytes {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let value = hex::decode(s)?;
        Self::from_bytes(&value[..]).map_err(|_| anyhow::anyhow!("byte deserialization failed"))
    }
}

//
// Add helper calls for Authority Signature
//

pub trait SuiAuthoritySignature {
    fn new<T>(value: &T, secret: &dyn signature::Signer<Self>) -> Self
    where
        T: Signable<Vec<u8>>;

    fn verify<T>(&self, value: &T, author: PublicKeyBytes) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>;
}

impl SuiAuthoritySignature for AuthoritySignature {
    fn new<T>(value: &T, secret: &dyn signature::Signer<Self>) -> Self
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
        let public_key =
            PublicKey::from_bytes(author.as_ref()).map_err(|_| SuiError::InvalidAddress)?;
        // serialize the message (see BCS serialization for determinism)
        let mut message = Vec::new();
        value.write(&mut message);

        // perform cryptographic signature check
        public_key
            .verify(&message, self)
            .map_err(|error| SuiError::InvalidSignature {
                error: error.to_string(),
            })
    }
}

impl signature::Signer<Signature> for KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        let signature_bytes: AccountSignature =
            <Self as signature::Signer<AccountSignature>>::try_sign(self, msg)?;
        let public_key_bytes: PublicKeyBytes = self.public().into();

        let mut result_bytes = [0u8; SUI_SIGNATURE_LENGTH];
        let sig_length = <KeyPair as narwhal_crypto::traits::KeyPair>::Sig::LENGTH;
        result_bytes[..sig_length].copy_from_slice(signature_bytes.as_ref());
        result_bytes[sig_length..].copy_from_slice(public_key_bytes.as_ref());
        Ok(Signature(result_bytes))
    }
}

impl signature::Verifier<Signature> for PublicKeyBytes {
    fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), signature::Error> {
        // deserialize the signature
        let signature =
            <AccountSignature as signature::Signature>::from_bytes(signature.signature_bytes())
                .map_err(|_| signature::Error::new())?;

        let public_key =
            PublicKey::from_bytes(self.as_ref()).map_err(|_| signature::Error::new())?;

        // perform cryptographic signature check
        public_key
            .verify(message, &signature)
            .map_err(|_| signature::Error::new())
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
    (kp.public().into(), kp)
}

// TODO: C-GETTER
pub fn get_key_pair_from_bytes(bytes: &[u8]) -> SuiResult<(SuiAddress, KeyPair)> {
    let priv_length = <KeyPair as narwhal_crypto::traits::KeyPair>::PrivKey::LENGTH;
    let sk =
        PrivateKey::from_bytes(&bytes[..priv_length]).map_err(|_| SuiError::InvalidPrivateKey)?;
    let kp: KeyPair = sk.into();
    if kp.public().as_ref() != &bytes[priv_length..] {
        return Err(SuiError::InvalidAddress);
    }
    Ok((kp.public().into(), kp))
}

// TODO: replace this with a byte interpretation based on multicodec
pub const SUI_SIGNATURE_LENGTH: usize = PublicKey::LENGTH + AccountSignature::LENGTH;

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
        &self.0[..AccountSignature::LENGTH]
    }

    pub fn public_key_bytes(&self) -> &[u8] {
        &self.0[AccountSignature::LENGTH..]
    }

    /// This performs signature verification on the passed-in signature, additionally checking
    /// that the signature was performed with a PublicKey belonging to an expected author, indicated by its Sui Address
    pub fn verify<T>(&self, value: &T, author: SuiAddress) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        let mut message: Vec<u8> = Vec::new();
        value.write(&mut message);
        let (signature, public_key_bytes) = self.get_verification_inputs(author)?;

        // is this a cryptographically correct public key?
        // TODO: perform stricter key validation, sp. small order points, see https://github.com/MystenLabs/sui/issues/101
        let public_key = PublicKey::from_bytes(public_key_bytes.as_ref()).map_err(|err| {
            SuiError::InvalidSignature {
                error: err.to_string(),
            }
        })?;

        // perform cryptographic signature check
        public_key
            .verify(&message, &signature)
            .map_err(|error| SuiError::InvalidSignature {
                error: error.to_string(),
            })
    }

    pub fn get_verification_inputs(
        &self,
        author: SuiAddress,
    ) -> Result<(AccountSignature, PublicKeyBytes), SuiError> {
        // Is this signature emitted by the expected author?
        let public_key_bytes: PublicKeyBytes =
            PublicKeyBytes::from_bytes(self.public_key_bytes()).expect("byte lengths match");
        let received_addr: SuiAddress = (&public_key_bytes).into();
        if received_addr != author {
            return Err(SuiError::IncorrectSigner {
                error: format!("Signature get_verification_inputs() failure. Author is {author}, received address is {received_addr}")
            });
        }

        // deserialize the signature
        let signature =
            <AccountSignature as signature::Signature>::from_bytes(self.signature_bytes())
                .map_err(|err| SuiError::InvalidSignature {
                    error: err.to_string(),
                })?;

        // serialize the message (see BCS serialization for determinism)
        Ok((signature, public_key_bytes))
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
        let weight = committee.weight(&self.authority);
        fp_ensure!(weight > 0, SuiError::UnknownSigner);

        obligation
            .public_keys
            .get_mut(message_index)
            .ok_or(SuiError::InvalidAddress)?
            .push(committee.public_key(&self.authority)?);
        obligation
            .signatures
            .get_mut(message_index)
            .ok_or(SuiError::InvalidAddress)?
            .add_signature(self.signature.clone())
            .map_err(|_| SuiError::InvalidSignature {
                error: "Invalid Signature".to_string(),
            })?;
        Ok(())
    }

    pub fn verify<T>(&self, data: &T, committee: &Committee) -> SuiResult<()>
    where
        T: Signable<Vec<u8>>,
    {
        let mut obligation = VerificationObligation::default();
        let idx = obligation.add_message(data);
        self.add_to_verification_obligation(committee, &mut obligation, idx)?;
        obligation.verify_all()?;
        Ok(())
    }
}

/// Represents at least a quorum (could be more) of authority signatures.
/// STRONG_THRESHOLD indicates whether to use the quorum threshold for quorum check.
/// When STRONG_THRESHOLD is true, the quorum is valid when the total stake is
/// at least the quorum threshold (2f+1) of the committee; when STRONG_THRESHOLD is false,
/// the quorum is valid when the total stake is at least the validity threshold (f+1) of
/// the committee.
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuthorityQuorumSignInfo<const STRONG_THRESHOLD: bool> {
    pub epoch: EpochId,
    #[schemars(with = "Base64")]
    pub signature: AggregateAuthoritySignature,
    #[schemars(with = "Base64")]
    #[serde_as(as = "SuiBitmap")]
    pub signers_map: RoaringBitmap,
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
static_assertions::assert_not_impl_any!(AuthorityStrongQuorumSignInfo: Hash, Eq, PartialEq);
static_assertions::assert_not_impl_any!(AuthorityWeakQuorumSignInfo: Hash, Eq, PartialEq);

impl<const S: bool> AuthoritySignInfoTrait for AuthorityQuorumSignInfo<S> {}

impl<const STRONG_THRESHOLD: bool> AuthorityQuorumSignInfo<STRONG_THRESHOLD> {
    pub fn new(epoch: EpochId) -> Self {
        AuthorityQuorumSignInfo {
            epoch,
            signature: AggregateAccountSignature::default(),
            signers_map: RoaringBitmap::new(),
        }
    }

    pub fn new_with_signatures(
        epoch: EpochId,
        mut signatures: Vec<(PublicKeyBytes, AuthoritySignature)>,
        committee: &Committee,
    ) -> SuiResult<Self> {
        let mut map = RoaringBitmap::new();
        signatures.sort_by_key(|(public_key, _)| *public_key);

        for (pk, _) in &signatures {
            map.insert(
                committee
                    .authority_index(pk)
                    .ok_or(SuiError::UnknownSigner)? as u32,
            );
        }
        let sigs: Vec<AuthoritySignature> = signatures.into_iter().map(|(_, sig)| sig).collect();

        Ok(AuthorityQuorumSignInfo {
            epoch,
            signature: AggregateAuthoritySignature::aggregate(sigs).map_err(|e| {
                SuiError::InvalidSignature {
                    error: e.to_string(),
                }
            })?,
            signers_map: map,
        })
    }

    pub fn authorities<'a>(
        &'a self,
        committee: &'a Committee,
    ) -> impl Iterator<Item = SuiResult<&AuthorityName>> {
        self.signers_map.iter().map(|i| {
            committee
                .authority_by_index(i)
                .ok_or(SuiError::InvalidAuthenticator)
        })
    }

    pub fn len(&self) -> u64 {
        self.signers_map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.signers_map.is_empty()
    }

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

        // Create obligations for the committee signatures
        obligation
            .signatures
            .get_mut(message_index)
            .ok_or(SuiError::InvalidAuthenticator)?
            .add_aggregate(self.signature.clone())
            .map_err(|_| SuiError::InvalidSignature {
                error: "Signature Aggregation failed".to_string(),
            })?;

        let selected_public_keys = obligation
            .public_keys
            .get_mut(message_index)
            .ok_or(SuiError::InvalidAuthenticator)?;

        for authority_index in self.signers_map.iter() {
            let authority = committee
                .authority_by_index(authority_index)
                .ok_or(SuiError::UnknownSigner)?;

            // Update weight.
            let voting_rights = committee.weight(authority);
            fp_ensure!(voting_rights > 0, SuiError::UnknownSigner);
            weight += voting_rights;

            selected_public_keys.push(committee.public_key(authority)?);
        }

        let threshold = if STRONG_THRESHOLD {
            committee.quorum_threshold()
        } else {
            committee.validity_threshold()
        };
        fp_ensure!(weight >= threshold, SuiError::CertificateRequiresQuorum);

        Ok(())
    }

    pub fn verify<T>(&self, data: &T, committee: &Committee) -> SuiResult<()>
    where
        T: Signable<Vec<u8>>,
    {
        let mut obligation = VerificationObligation::default();
        let message_index = obligation.add_message(data);
        self.add_to_verification_obligation(committee, &mut obligation, message_index)?;
        obligation.verify_all()?;
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
    pub messages: Vec<Vec<u8>>,
    pub signatures: Vec<S>,
    pub public_keys: Vec<Vec<S::PubKey>>,
}

impl<S: AggregateAuthenticator> VerificationObligation<S> {
    pub fn new() -> VerificationObligation<S> {
        VerificationObligation {
            ..Default::default()
        }
    }

    /// Add a new message to the list of messages to be verified.
    /// Returns the index of the message.
    pub fn add_message<T>(&mut self, message_value: &T) -> usize
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        message_value.write(&mut message);

        self.signatures.push(S::default());
        self.public_keys.push(Vec::new());
        self.messages.push(message);
        self.messages.len() - 1
    }

    pub fn verify_all(self) -> SuiResult<()> {
        S::batch_verify(
            &self.signatures[..],
            &self.public_keys.iter().map(|x| &x[..]).collect::<Vec<_>>(),
            &self.messages.iter().map(|x| &x[..]).collect::<Vec<_>>()[..],
        )
        .map_err(|error| SuiError::InvalidSignature {
            error: format!("{error}"),
        })?;

        Ok(())
    }
}
