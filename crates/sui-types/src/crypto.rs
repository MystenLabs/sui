// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use anyhow::Error;
use base64ct::Encoding;
use digest::Digest;
use narwhal_crypto::ed25519::{
    Ed25519AggregateSignature, Ed25519KeyPair, Ed25519PrivateKey, Ed25519PublicKey,
    Ed25519Signature,
};
use narwhal_crypto::secp256k1::{
    Secp256k1KeyPair, Secp256k1PrivateKey, Secp256k1PublicKey, Secp256k1Signature,
};
pub use narwhal_crypto::traits::KeyPair as KeypairTraits;
pub use narwhal_crypto::traits::{
    AggregateAuthenticator, Authenticator, EncodeDecodeBase64, SigningKey, ToFromBytes,
    VerifyingKey,
};
use narwhal_crypto::Verifier;
use rand::rngs::OsRng;
use roaring::RoaringBitmap;
use schemars::JsonSchema;
use serde::ser::Serializer;
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::{serde_as, Bytes};
use sha3::Sha3_256;
use signature::Signer;

use crate::base_types::{AuthorityName, SuiAddress};
use crate::committee::{Committee, EpochId};
use crate::error::{SuiError, SuiResult};
use crate::sui_serde::{Base64, Readable, SuiBitmap};
pub use enum_dispatch::enum_dispatch;

// Authority Objects
pub type AuthorityKeyPair = Ed25519KeyPair;
pub type AuthorityPrivateKey = Ed25519PrivateKey;
pub type AuthorityPublicKey = Ed25519PublicKey;
pub type AuthoritySignature = Ed25519Signature;
pub type AggregateAuthoritySignature = Ed25519AggregateSignature;

// TODO(joyqvq): prefix these types with Default, DefaultAccountKeyPair etc
pub type AccountKeyPair = Ed25519KeyPair;
pub type AccountPublicKey = Ed25519PublicKey;
pub type AccountPrivateKey = Ed25519PrivateKey;
pub type AccountSignature = Ed25519Signature;

// TODO: fix below.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum SuiKeyPair {
    Ed25519SuiKeyPair(Ed25519KeyPair),
    Secp256k1SuiKeyPair(Secp256k1KeyPair),
}

#[derive(Debug, Clone)]
pub enum PublicKey {
    Ed25519KeyPair(Ed25519PublicKey),
    Secp256k1KeyPair(Secp256k1PublicKey),
}

impl SuiKeyPair {
    pub fn public(&self) -> PublicKey {
        match self {
            SuiKeyPair::Ed25519SuiKeyPair(kp) => PublicKey::Ed25519KeyPair(kp.public().clone()),
            SuiKeyPair::Secp256k1SuiKeyPair(kp) => PublicKey::Secp256k1KeyPair(kp.public().clone()),
        }
    }
}

impl signature::Signer<Signature> for SuiKeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        match self {
            SuiKeyPair::Ed25519SuiKeyPair(kp) => kp.try_sign(msg),
            SuiKeyPair::Secp256k1SuiKeyPair(kp) => kp.try_sign(msg),
        }
    }
}

impl FromStr for SuiKeyPair {
    type Err = eyre::Report;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let kp = Self::decode_base64(s).map_err(|e| eyre::eyre!("{}", e.to_string()))?;
        Ok(kp)
    }
}

impl EncodeDecodeBase64 for SuiKeyPair {
    fn encode_base64(&self) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        match self {
            SuiKeyPair::Ed25519SuiKeyPair(kp) => {
                let kp1 = kp.copy();
                bytes.extend_from_slice(&[self.public().flag()]);
                bytes.extend_from_slice(kp.public().as_ref());
                bytes.extend_from_slice(kp1.private().as_ref());
            }
            SuiKeyPair::Secp256k1SuiKeyPair(kp) => {
                let kp1 = kp.copy();
                bytes.extend_from_slice(&[self.public().flag()]);
                bytes.extend_from_slice(kp.public().as_ref());
                bytes.extend_from_slice(kp1.private().as_ref());
            }
        }
        base64ct::Base64::encode_string(&bytes[..])
    }

    fn decode_base64(value: &str) -> Result<Self, eyre::Report> {
        let bytes =
            base64ct::Base64::decode_vec(value).map_err(|e| eyre::eyre!("{}", e.to_string()))?;
        match bytes.first() {
            Some(x) => {
                if x == &Ed25519SuiSignature::SCHEME.flag() {
                    let sk = Ed25519PrivateKey::from_bytes(&bytes[1 + Ed25519PublicKey::LENGTH..])?;
                    Ok(SuiKeyPair::Ed25519SuiKeyPair(<Ed25519KeyPair as From<
                        Ed25519PrivateKey,
                    >>::from(
                        sk
                    )))
                } else if x == &Secp256k1SuiSignature::SCHEME.flag() {
                    let sk =
                        Secp256k1PrivateKey::from_bytes(&bytes[1 + Secp256k1PublicKey::LENGTH..])?;
                    Ok(SuiKeyPair::Secp256k1SuiKeyPair(
                        <Secp256k1KeyPair as From<Secp256k1PrivateKey>>::from(sk),
                    ))
                } else {
                    Err(eyre::eyre!("Invalid flag byte"))
                }
            }
            _ => Err(eyre::eyre!("Invalid bytes")),
        }
    }
}

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        match self {
            PublicKey::Ed25519KeyPair(pk) => pk.as_ref(),
            PublicKey::Secp256k1KeyPair(pk) => pk.as_ref(),
        }
    }
}

impl PublicKey {
    pub fn flag(&self) -> u8 {
        match self {
            PublicKey::Ed25519KeyPair(_) => Ed25519SuiSignature::SCHEME.flag(),
            PublicKey::Secp256k1KeyPair(_) => Secp256k1SuiSignature::SCHEME.flag(),
        }
    }
}
//
// Define Bytes representation of the Authority's PublicKey
//

#[serde_as]
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AuthorityPublicKeyBytes(
    #[serde_as(as = "Readable<Base64, Bytes>")] [u8; AuthorityPublicKey::LENGTH],
);

impl AuthorityPublicKeyBytes {
    fn fmt_impl(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::result::Result<(), std::fmt::Error> {
        let s = hex::encode(&self.0);
        write!(f, "k#{}", s)?;
        Ok(())
    }
}

impl TryFrom<AuthorityPublicKeyBytes> for AuthorityPublicKey {
    type Error = signature::Error;

    fn try_from(bytes: AuthorityPublicKeyBytes) -> Result<AuthorityPublicKey, Self::Error> {
        AuthorityPublicKey::from_bytes(bytes.as_ref()).map_err(|_| Self::Error::new())
    }
}

impl From<&AuthorityPublicKey> for AuthorityPublicKeyBytes {
    fn from(pk: &AuthorityPublicKey) -> AuthorityPublicKeyBytes {
        AuthorityPublicKeyBytes::from_bytes(pk.as_ref()).unwrap()
    }
}

impl AsRef<[u8]> for AuthorityPublicKeyBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl std::fmt::Debug for AuthorityPublicKeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        self.fmt_impl(f)
    }
}

impl Display for AuthorityPublicKeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        self.fmt_impl(f)
    }
}

impl ToFromBytes for AuthorityPublicKeyBytes {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let bytes: [u8; AuthorityPublicKey::LENGTH] =
            bytes.try_into().map_err(signature::Error::from_source)?;
        Ok(AuthorityPublicKeyBytes(bytes))
    }
}

impl AuthorityPublicKeyBytes {
    /// This ensures it's impossible to construct an instance with other than registered lengths
    pub fn new(bytes: [u8; AuthorityPublicKey::LENGTH]) -> AuthorityPublicKeyBytes
where {
        AuthorityPublicKeyBytes(bytes)
    }

    // this is probably derivable, but we'd rather have it explicitly laid out for instructional purposes,
    // see [#34](https://github.com/MystenLabs/narwhal/issues/34)
    #[allow(dead_code)]
    fn default() -> Self {
        Self([0u8; AuthorityPublicKey::LENGTH])
    }
}

impl FromStr for AuthorityPublicKeyBytes {
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

    fn verify<T>(&self, value: &T, author: AuthorityPublicKeyBytes) -> Result<(), SuiError>
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

    fn verify<T>(&self, value: &T, author: AuthorityPublicKeyBytes) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // is this a cryptographically valid public Key?
        let public_key = AuthorityPublicKey::from_bytes(author.as_ref()).map_err(|_| {
            SuiError::KeyConversionError(
                "Failed to serialize public key bytes to valid public key".to_string(),
            )
        })?;
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

pub fn random_key_pairs<KP: KeypairTraits>(num: usize) -> Vec<KP>
where
    <KP as KeypairTraits>::PubKey: SuiPublicKey,
{
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
pub fn get_key_pair<KP: KeypairTraits>() -> (SuiAddress, KP)
where
    <KP as KeypairTraits>::PubKey: SuiPublicKey,
{
    get_key_pair_from_rng(&mut OsRng)
}

/// Generate a keypair from the specified RNG (useful for testing with seedable rngs).
pub fn get_key_pair_from_rng<KP: KeypairTraits, R>(csprng: &mut R) -> (SuiAddress, KP)
where
    R: rand::CryptoRng + rand::RngCore,
    <KP as KeypairTraits>::PubKey: SuiPublicKey,
{
    let kp = KP::generate(csprng);
    (kp.public().into(), kp)
}

// TODO: C-GETTER
pub fn get_key_pair_from_bytes<KP: KeypairTraits>(bytes: &[u8]) -> SuiResult<(SuiAddress, KP)>
where
    <KP as KeypairTraits>::PubKey: SuiPublicKey,
{
    let priv_length = <KP as KeypairTraits>::PrivKey::LENGTH;
    let sk = <KP as KeypairTraits>::PrivKey::from_bytes(&bytes[..priv_length])
        .map_err(|_| SuiError::InvalidPrivateKey)?;
    let kp: KP = sk.into();
    if kp.public().as_ref() != &bytes[priv_length..] {
        return Err(SuiError::InvalidAddress);
    }
    Ok((kp.public().into(), kp))
}

//
// Account Signatures
//

// Enums for Signatures
#[enum_dispatch]
#[derive(Clone, JsonSchema, PartialEq, Eq, Hash)]
pub enum Signature {
    Ed25519SuiSignature,
    Secp256k1SuiSignature,
}

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes = self.as_ref();

        if serializer.is_human_readable() {
            let s = base64ct::Base64::encode_string(bytes);
            serializer.serialize_str(&s)
        } else {
            serializer.serialize_bytes(bytes)
        }
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        let bytes = if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            base64ct::Base64::decode_vec(&s).map_err(|e| Error::custom(e.to_string()))?
        } else {
            let data: Vec<u8> = Vec::deserialize(deserializer)?;
            data
        };

        Self::from_bytes(&bytes).map_err(|e| Error::custom(e.to_string()))
    }
}

// Can refactor this with a library
impl Signature {
    pub fn new<T>(value: &T, secret: &dyn signature::Signer<Signature>) -> Signature
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        secret.sign(&message)
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        match self {
            Signature::Ed25519SuiSignature(sig) => sig.as_ref(),
            Signature::Secp256k1SuiSignature(sig) => sig.as_ref(),
        }
    }
}

impl signature::Signature for Signature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        match bytes.first() {
            Some(x) => {
                if x == &Ed25519SuiSignature::SCHEME.flag() {
                    Ok(<Ed25519SuiSignature as ToFromBytes>::from_bytes(bytes)
                        .map_err(|_| signature::Error::new())?
                        .into())
                } else if x == &Secp256k1SuiSignature::SCHEME.flag() {
                    Ok(<Secp256k1SuiSignature as ToFromBytes>::from_bytes(bytes)
                        .map_err(|_| signature::Error::new())?
                        .into())
                } else {
                    Err(signature::Error::new())
                }
            }
            _ => Err(signature::Error::new()),
        }
    }
}

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let flag = base64ct::Base64::encode_string(&[self.scheme().flag()]);
        let s = base64ct::Base64::encode_string(self.signature_bytes());
        let p = base64ct::Base64::encode_string(self.public_key_bytes());
        write!(f, "{flag}@{s}@{p}")?;
        Ok(())
    }
}

//
// Ed25519 Sui Signature port
//

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
pub struct Ed25519SuiSignature(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; Ed25519PublicKey::LENGTH + Ed25519Signature::LENGTH + 1],
);

impl SuiSignatureInner for Ed25519SuiSignature {
    type Sig = Ed25519Signature;
    type PubKey = Ed25519PublicKey;
    type KeyPair = Ed25519KeyPair;
    const LENGTH: usize = Ed25519PublicKey::LENGTH + Ed25519Signature::LENGTH + 1;
}

impl SuiPublicKey for Ed25519PublicKey {
    const SIGNATURE_SCHEME: SignatureScheme = SignatureScheme::ED25519;
}

impl AsRef<[u8]> for Ed25519SuiSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl signature::Signature for Ed25519SuiSignature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        if bytes.len() != Self::LENGTH {
            return Err(signature::Error::new());
        }
        let mut sig_bytes = [0; Self::LENGTH];
        sig_bytes.copy_from_slice(bytes);
        Ok(Self(sig_bytes))
    }
}

impl signature::Signer<Signature> for Ed25519KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        Ok(Ed25519SuiSignature::new(self, msg)
            .map_err(|_| signature::Error::new())?
            .into())
    }
}

//
// Secp256k1 Sui Signature port
//
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
pub struct Secp256k1SuiSignature(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; Secp256k1PublicKey::LENGTH + Secp256k1Signature::LENGTH + 1],
);

impl SuiSignatureInner for Secp256k1SuiSignature {
    type Sig = Secp256k1Signature;
    type PubKey = Secp256k1PublicKey;
    type KeyPair = Secp256k1KeyPair;
    const LENGTH: usize = Secp256k1PublicKey::LENGTH + Secp256k1Signature::LENGTH + 1;
}

// impl Default for Secp256k1SuiSignature {
//     []
// }

impl SuiPublicKey for Secp256k1PublicKey {
    const SIGNATURE_SCHEME: SignatureScheme = SignatureScheme::Secp256k1;
}

impl AsRef<[u8]> for Secp256k1SuiSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl signature::Signature for Secp256k1SuiSignature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        if bytes.len() != Self::LENGTH {
            return Err(signature::Error::new());
        }
        let mut sig_bytes = [0; Self::LENGTH];
        sig_bytes.copy_from_slice(bytes);
        Ok(Self(sig_bytes))
    }
}

impl signature::Signer<Signature> for Secp256k1KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        Ok(Secp256k1SuiSignature::new(self, msg)
            .map_err(|_| signature::Error::new())?
            .into())
    }
}
//
// This struct exists due to the limitations of the `enum_dispatch` library.
//
pub trait SuiSignatureInner: Sized + signature::Signature + PartialEq + Eq + Hash {
    type Sig: Authenticator<PubKey = Self::PubKey> + ToObligationSignature;
    type PubKey: VerifyingKey<Sig = Self::Sig> + SuiPublicKey;
    type KeyPair: KeypairTraits<PubKey = Self::PubKey, Sig = Self::Sig>;

    const LENGTH: usize = Self::Sig::LENGTH + Self::PubKey::LENGTH + 1;
    const SCHEME: SignatureScheme = Self::PubKey::SIGNATURE_SCHEME;

    fn get_verification_inputs(&self, author: SuiAddress) -> SuiResult<(Self::Sig, Self::PubKey)> {
        // Is this signature emitted by the expected author?
        let bytes = self.public_key_bytes();
        let pk = Self::PubKey::from_bytes(bytes)
            .map_err(|_| SuiError::KeyConversionError("Invalid public key".to_string()))?;

        let received_addr = SuiAddress::from(&pk);
        if received_addr != author {
            return Err(SuiError::IncorrectSigner {
                error: format!("Signature get_verification_inputs() failure. Author is {author}, received address is {received_addr}")
            });
        }

        // deserialize the signature
        let signature = Self::Sig::from_bytes(self.signature_bytes()).map_err(|err| {
            SuiError::InvalidSignature {
                error: err.to_string(),
            }
        })?;

        Ok((signature, pk))
    }

    fn new(kp: &Self::KeyPair, message: &[u8]) -> SuiResult<Self> {
        let sig = kp
            .try_sign(message)
            .map_err(|_| SuiError::InvalidSignature {
                error: "Failed to sign valid message with keypair".to_string(),
            })?;

        let mut signature_bytes: Vec<u8> = Vec::new();
        signature_bytes
            .extend_from_slice(&[<Self::PubKey as SuiPublicKey>::SIGNATURE_SCHEME.flag()]);
        signature_bytes.extend_from_slice(sig.as_ref());
        signature_bytes.extend_from_slice(kp.public().as_ref());
        Self::from_bytes(&signature_bytes[..]).map_err(|err| SuiError::InvalidSignature {
            error: err.to_string(),
        })
    }
}

pub trait SuiPublicKey: VerifyingKey {
    const SIGNATURE_SCHEME: SignatureScheme;
}

#[enum_dispatch(Signature)]
pub trait SuiSignature: Sized + signature::Signature {
    fn signature_bytes(&self) -> &[u8];
    fn public_key_bytes(&self) -> &[u8];
    fn scheme(&self) -> SignatureScheme;

    fn verify<T>(&self, value: &T, author: SuiAddress) -> SuiResult<()>
    where
        T: Signable<Vec<u8>>;

    fn add_to_verification_obligation_or_verify(
        &self,
        author: SuiAddress,
        obligation: &mut VerificationObligation,
        idx: usize,
    ) -> SuiResult<()>;
}

impl<S: SuiSignatureInner + Sized> SuiSignature for S {
    fn verify<T>(&self, value: &T, author: SuiAddress) -> SuiResult<()>
    where
        T: Signable<Vec<u8>>,
    {
        // Currently done twice - can we improve on this?;
        let (sig, pk) = &self.get_verification_inputs(author)?;
        let mut message = Vec::new();
        value.write(&mut message);
        pk.verify(&message[..], sig)
            .map_err(|_| SuiError::InvalidSignature {
                error: "hello".to_string(),
            })
    }

    fn add_to_verification_obligation_or_verify(
        &self,
        author: SuiAddress,
        obligation: &mut VerificationObligation,
        idx: usize,
    ) -> SuiResult<()> {
        let (sig, pk) = self.get_verification_inputs(author)?;
        match obligation.add_signature_and_public_key(sig.clone(), pk.clone(), idx) {
            Ok(_) => Ok(()),
            Err(err) => {
                let msg = &obligation.messages[idx][..];
                pk.verify(msg, &sig)
                    .map_err(|_| SuiError::InvalidSignature {
                        error: err.to_string(),
                    })
            }
        }
    }

    fn signature_bytes(&self) -> &[u8] {
        &self.as_ref()[1..1 + S::Sig::LENGTH]
    }

    fn public_key_bytes(&self) -> &[u8] {
        &self.as_ref()[S::Sig::LENGTH + 1..]
    }

    fn scheme(&self) -> SignatureScheme {
        S::PubKey::SIGNATURE_SCHEME
    }
}

/// AuthoritySignInfoTrait is a trait used specifically for a few structs in messages.rs
/// to template on whether the struct is signed by an authority. We want to limit how
/// those structs can be instanted on, hence the sealed trait.
/// TODO: We could also add the aggregated signature as another impl of the trait.
///       This will make CertifiedTransaction also an instance of the same struct.
pub trait AuthoritySignInfoTrait: private::SealedAuthoritySignInfoTrait {
    fn verify<T: Signable<Vec<u8>>>(&self, data: &T, committee: &Committee) -> SuiResult;

    fn add_to_verification_obligation(
        &self,
        committee: &Committee,
        obligation: &mut VerificationObligation,
        message_index: usize,
    ) -> SuiResult<()>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EmptySignInfo {}
impl AuthoritySignInfoTrait for EmptySignInfo {
    fn verify<T: Signable<Vec<u8>>>(&self, _data: &T, _committee: &Committee) -> SuiResult {
        Ok(())
    }

    fn add_to_verification_obligation(
        &self,
        _committee: &Committee,
        _obligation: &mut VerificationObligation,
        _message_index: usize,
    ) -> SuiResult<()> {
        Ok(())
    }
}

pub fn add_to_verification_obligation_and_verify<S, T>(
    sig: &S,
    data: &T,
    committee: &Committee,
) -> SuiResult
where
    S: AuthoritySignInfoTrait,
    T: Signable<Vec<u8>>,
{
    let mut obligation = VerificationObligation::default();
    let idx = obligation.add_message(data);
    sig.add_to_verification_obligation(committee, &mut obligation, idx)?;
    obligation.verify_all()?;
    Ok(())
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct AuthoritySignInfo {
    pub epoch: EpochId,
    pub authority: AuthorityName,
    pub signature: AuthoritySignature,
}
impl AuthoritySignInfoTrait for AuthoritySignInfo {
    fn verify<T: Signable<Vec<u8>>>(&self, data: &T, committee: &Committee) -> SuiResult<()> {
        add_to_verification_obligation_and_verify(self, data, committee)
    }

    fn add_to_verification_obligation(
        &self,
        committee: &Committee,
        obligation: &mut VerificationObligation,
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
}

impl Hash for AuthoritySignInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch.hash(state);
        self.authority.hash(state);
    }
}

impl Display for AuthoritySignInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AuthoritySignInfo {{ epoch: {:?}, authority: {} }}",
            self.epoch, self.authority,
        )
    }
}

impl PartialEq for AuthoritySignInfo {
    fn eq(&self, other: &Self) -> bool {
        // We do not compare the signature, because there can be multiple
        // valid signatures for the same epoch and authority.
        self.epoch == other.epoch && self.authority == other.authority
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

impl<const STRONG_THRESHOLD: bool> AuthoritySignInfoTrait
    for AuthorityQuorumSignInfo<STRONG_THRESHOLD>
{
    fn verify<T: Signable<Vec<u8>>>(&self, data: &T, committee: &Committee) -> SuiResult<()> {
        add_to_verification_obligation_and_verify(self, data, committee)
    }

    fn add_to_verification_obligation(
        &self,
        committee: &Committee,
        obligation: &mut VerificationObligation,
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
        println!("{:?}", self);
        fp_ensure!(weight >= threshold, SuiError::CertificateRequiresQuorum);

        Ok(())
    }
}

impl<const STRONG_THRESHOLD: bool> AuthorityQuorumSignInfo<STRONG_THRESHOLD> {
    pub fn new(epoch: EpochId) -> Self {
        AuthorityQuorumSignInfo {
            epoch,
            signature: AggregateAuthoritySignature::default(),
            signers_map: RoaringBitmap::new(),
        }
    }

    pub fn new_with_signatures(
        mut signatures: Vec<(AuthorityPublicKeyBytes, AuthoritySignature)>,
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
            epoch: committee.epoch,
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
}

impl<const S: bool> Display for AuthorityQuorumSignInfo<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} {{ epoch: {:?}, signers_map: {:?} }}",
            if S {
                "AuthorityStrongQuorumSignInfo"
            } else {
                "AuthorityWeakQuorumSignInfo"
            },
            self.epoch,
            self.signers_map,
        )?;
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
///
/// # Safety
/// We protect the access to this marker trait through a "sealed trait" pattern:
/// impls must be add added here (nowehre else) which lets us note those impls
/// MUST be on types that comply with the `serde_name` machinery
/// for the below implementations not to panic. One way to check they work is to write
/// a unit test for serialization to / deserialization from signable bytes.
///
///
mod bcs_signable {

    pub trait BcsSignable: serde::Serialize + serde::de::DeserializeOwned {}
    impl BcsSignable for crate::batch::TransactionBatch {}
    impl BcsSignable for crate::batch::AuthorityBatch {}
    impl BcsSignable for crate::messages_checkpoint::CheckpointSummary {}
    impl BcsSignable for crate::messages_checkpoint::CheckpointContents {}
    impl BcsSignable for crate::messages_checkpoint::CheckpointProposalSummary {}
    impl BcsSignable for crate::messages::TransactionEffects {}
    impl BcsSignable for crate::messages::TransactionData {}
    impl BcsSignable for crate::messages::SenderSignedData {}
    impl BcsSignable for crate::messages::EpochInfo {}
    impl BcsSignable for crate::object::Object {}

    impl BcsSignable for super::bcs_signable_test::Foo {}
    #[cfg(test)]
    impl BcsSignable for super::bcs_signable_test::Bar {}
}

impl<T, W> Signable<W> for T
where
    T: bcs_signable::BcsSignable,
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
    T: bcs_signable::BcsSignable,
{
    fn from_signable_bytes(bytes: &[u8]) -> Result<Self, Error> {
        // Remove name tag before deserialization using BCS
        let name = serde_name::trace_name::<Self>().expect("Self should be a struct or an enum");
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

//
// Helper enum to statically dispatch sender sigs to verification obligation.
//

pub enum ObligationSignature<'a> {
    AuthoritySig((&'a AuthoritySignature, &'a AuthorityPublicKey)),
    None, // Do not verify signature/public key pair
}

pub trait ToObligationSignature: Authenticator {
    fn to_obligation_signature<'a>(
        &'a self,
        _pubkey: &'a <Self as Authenticator>::PubKey,
    ) -> ObligationSignature<'a> {
        ObligationSignature::None
    }
}

impl ToObligationSignature for AuthoritySignature {
    fn to_obligation_signature<'a>(
        &'a self,
        pubkey: &'a <Self as Authenticator>::PubKey,
    ) -> ObligationSignature<'a> {
        ObligationSignature::AuthoritySig((self, pubkey))
    }
}
// Careful, the implementation may be overlapping with the AuthoritySignature implementation. Be sure to fix it if it does:
// TODO: Change all these into macros.
impl ToObligationSignature for Secp256k1Signature {}

#[derive(Default)]
pub struct VerificationObligation {
    pub messages: Vec<Vec<u8>>,
    pub signatures: Vec<AggregateAuthoritySignature>,
    pub public_keys: Vec<Vec<AuthorityPublicKey>>,
}

impl VerificationObligation {
    pub fn new() -> VerificationObligation {
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

        self.signatures.push(AggregateAuthoritySignature::default());
        self.public_keys.push(Vec::new());
        self.messages.push(message);
        self.messages.len() - 1
    }

    // Attempts to add signature and public key to the obligation. If this fails, ensure to call `verify` manually.
    pub fn add_signature_and_public_key<
        S: ToObligationSignature + Authenticator<PubKey = P>,
        P: VerifyingKey<Sig = S>,
    >(
        &mut self,
        signature: S,
        public_key: P,
        idx: usize,
    ) -> SuiResult<()> {
        match signature.to_obligation_signature(&public_key) {
            ObligationSignature::AuthoritySig((sig, pubkey)) => {
                self.public_keys
                    .get_mut(idx)
                    .ok_or(SuiError::InvalidAuthenticator)?
                    .push(pubkey.clone());
                self.signatures
                    .get_mut(idx)
                    .ok_or(SuiError::InvalidAuthenticator)?
                    .add_signature(sig.clone())
                    .map_err(|_| SuiError::InvalidSignature {
                        error: "Failed to add signature to obligation".to_string(),
                    })?;
                Ok(())
            }
            ObligationSignature::None => Err(SuiError::SenderSigUnbatchable),
        }
    }

    pub fn verify_all(self) -> SuiResult<()> {
        AggregateAuthoritySignature::batch_verify(
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

pub mod bcs_signable_test {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    pub struct Foo(pub String);

    #[cfg(test)]
    #[derive(Serialize, Deserialize)]
    pub struct Bar(pub String);

    #[cfg(test)]
    use super::VerificationObligation;

    #[cfg(test)]
    pub fn get_obligation_input<T>(value: &T) -> (VerificationObligation, usize)
    where
        T: super::bcs_signable::BcsSignable,
    {
        let mut obligation = VerificationObligation::default();
        // Add the obligation of the authority signature verifications.
        let idx = obligation.add_message(value);
        (obligation, idx)
    }
}

#[derive(Deserialize, Serialize, JsonSchema, Debug)]
pub enum SignatureScheme {
    ED25519,
    Secp256k1,
}

impl SignatureScheme {
    pub fn flag(&self) -> u8 {
        match self {
            SignatureScheme::ED25519 => 0x00,
            SignatureScheme::Secp256k1 => 0xed,
        }
    }
}
