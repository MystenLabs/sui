// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::base_types::{AuthorityName, SuiAddress};
use crate::committee::EpochId;
use crate::error::{SuiError, SuiResult};
use crate::json_schema;
use crate::readable_serde::encoding::Base64;
use crate::readable_serde::Readable;
use anyhow::anyhow;
use anyhow::Error;
use base64ct::Encoding;
use digest::Digest;
use ed25519_dalek as dalek;
use ed25519_dalek::{Keypair as DalekKeypair, PublicKey, Verifier};
use narwhal_crypto::ed25519::Ed25519KeyPair;
use narwhal_crypto::ed25519::Ed25519PrivateKey;
use narwhal_crypto::ed25519::Ed25519PublicKey;
use once_cell::sync::OnceCell;
use rand::rngs::OsRng;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::Bytes;
use sha3::Sha3_256;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::str::FromStr;

// TODO: Make sure secrets are not copyable and movable to control where they are in memory
#[derive(Debug)]
pub struct KeyPair {
    key_pair: DalekKeypair,
    public_key_cell: OnceCell<PublicKeyBytes>,
}

impl KeyPair {
    pub fn public_key_bytes(&self) -> &PublicKeyBytes {
        self.public_key_cell.get_or_init(|| {
            let pk_arr: [u8; ed25519_dalek::PUBLIC_KEY_LENGTH] = self.key_pair.public.to_bytes();
            PublicKeyBytes(pk_arr)
        })
    }

    // TODO: eradicate unneeded uses (i.e. most)
    /// Avoid implementing `clone` on secret keys to prevent mistakes.
    #[must_use]
    pub fn copy(&self) -> KeyPair {
        KeyPair {
            key_pair: DalekKeypair {
                secret: dalek::SecretKey::from_bytes(self.key_pair.secret.as_bytes()).unwrap(),
                public: dalek::PublicKey::from_bytes(self.public_key_bytes().as_ref()).unwrap(),
            },
            public_key_cell: OnceCell::new(),
        }
    }

    /// Make a Narwhal-compatible key pair from a Sui keypair.
    pub fn make_narwhal_keypair(&self) -> Ed25519KeyPair {
        let key = self.copy();
        Ed25519KeyPair {
            name: Ed25519PublicKey(key.key_pair.public),
            secret: Ed25519PrivateKey(key.key_pair.secret),
        }
    }
}

impl From<DalekKeypair> for KeyPair {
    fn from(dalek_keypair: DalekKeypair) -> Self {
        Self {
            key_pair: dalek_keypair,
            public_key_cell: OnceCell::new(),
        }
    }
}

impl Serialize for KeyPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&base64ct::Base64::encode_string(&self.key_pair.to_bytes()))
    }
}

impl<'de> Deserialize<'de> for KeyPair {
    fn deserialize<D>(deserializer: D) -> Result<KeyPair, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = base64ct::Base64::decode_vec(&s)
            .map_err(|err| serde::de::Error::custom(err.to_string()))?;
        let key = DalekKeypair::from_bytes(&value)
            .map_err(|err| serde::de::Error::custom(err.to_string()))?;
        Ok(KeyPair {
            key_pair: key,
            public_key_cell: OnceCell::new(),
        })
    }
}

impl FromStr for KeyPair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = base64ct::Base64::decode_vec(s).map_err(|e| anyhow!("{}", e.to_string()))?;
        let key = dalek::Keypair::from_bytes(&value).map_err(|e| anyhow!("{}", e.to_string()))?;
        Ok(KeyPair {
            key_pair: key,
            public_key_cell: OnceCell::new(),
        })
    }
}

impl signature::Signer<Signature> for KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        let signature_bytes = self.key_pair.try_sign(msg)?;
        let public_key_bytes = self.public_key_bytes().as_ref();
        let mut result_bytes = [0u8; SUI_SIGNATURE_LENGTH];
        result_bytes[..ed25519_dalek::SIGNATURE_LENGTH].copy_from_slice(signature_bytes.as_ref());
        result_bytes[ed25519_dalek::SIGNATURE_LENGTH..].copy_from_slice(public_key_bytes);
        Ok(Signature(result_bytes))
    }
}

impl signature::Signer<AuthoritySignature> for KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<AuthoritySignature, signature::Error> {
        let sig = self.key_pair.try_sign(msg)?;
        Ok(AuthoritySignature(sig))
    }
}

#[serde_as]
#[derive(
    Eq, Default, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct PublicKeyBytes(
    #[schemars(with = "json_schema::Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; dalek::PUBLIC_KEY_LENGTH],
);

impl PublicKeyBytes {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl AsRef<[u8]> for PublicKeyBytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl TryInto<PublicKey> for PublicKeyBytes {
    type Error = SuiError;

    fn try_into(self) -> Result<PublicKey, Self::Error> {
        // TODO(https://github.com/MystenLabs/sui/issues/101): Do better key validation
        // to ensure the bytes represent a point on the curve.
        PublicKey::from_bytes(self.as_ref()).map_err(|_| SuiError::InvalidAuthenticator)
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
    let kp = DalekKeypair::generate(csprng);
    let keypair = KeyPair {
        key_pair: kp,
        public_key_cell: OnceCell::new(),
    };
    (SuiAddress::from(keypair.public_key_bytes()), keypair)
}

// TODO: C-GETTER
pub fn get_key_pair_from_bytes(bytes: &[u8]) -> (SuiAddress, KeyPair) {
    let keypair = KeyPair {
        key_pair: DalekKeypair::from_bytes(bytes).unwrap(),
        public_key_cell: OnceCell::new(),
    };
    (SuiAddress::from(keypair.public_key_bytes()), keypair)
}

// TODO: replace this with a byte interpretation based on multicodec
pub const SUI_SIGNATURE_LENGTH: usize =
    ed25519_dalek::PUBLIC_KEY_LENGTH + ed25519_dalek::SIGNATURE_LENGTH;

#[serde_as]
#[derive(Eq, PartialEq, Copy, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Signature(
    #[schemars(with = "json_schema::Base64")]
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

    pub fn signature_bytes(&self) -> &[u8] {
        &self.0[..ed25519_dalek::SIGNATURE_LENGTH]
    }

    pub fn public_key_bytes(&self) -> &[u8] {
        &self.0[ed25519_dalek::SIGNATURE_LENGTH..]
    }

    /// This performs signature verification on the passed-in signature, additionally checking
    /// that the signature was performed with a PublicKey belonging to an expected author, indicated by its Sui Address
    pub fn check<T>(&self, value: &T, author: SuiAddress) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // Is this signature emitted by the expected author?
        let public_key_bytes: [u8; ed25519_dalek::PUBLIC_KEY_LENGTH] = self
            .public_key_bytes()
            .try_into()
            .expect("byte lengths match");
        let received_addr = SuiAddress::from(&PublicKeyBytes(public_key_bytes));
        if received_addr != author {
            return Err(SuiError::IncorrectSigner {
                error: format!(
                    "Signature check failure. Author is {}, received address is {}",
                    author, received_addr
                ),
            });
        }

        // is this a cryptographically correct public key?
        // TODO: perform stricter key validation, sp. small order points, see https://github.com/MystenLabs/sui/issues/101
        let public_key =
            ed25519_dalek::PublicKey::from_bytes(self.public_key_bytes()).map_err(|err| {
                SuiError::InvalidSignature {
                    error: err.to_string(),
                }
            })?;

        // deserialize the signature
        let signature =
            ed25519_dalek::Signature::from_bytes(self.signature_bytes()).map_err(|err| {
                SuiError::InvalidSignature {
                    error: err.to_string(),
                }
            })?;

        // serialize the message (see BCS serialization for determinism)
        let mut message = Vec::new();
        value.write(&mut message);

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
    ) -> Result<(Vec<u8>, ed25519_dalek::Signature, PublicKeyBytes), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // Is this signature emitted by the expected author?
        let public_key_bytes: [u8; ed25519_dalek::PUBLIC_KEY_LENGTH] = self
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
        let signature =
            ed25519_dalek::Signature::from_bytes(self.signature_bytes()).map_err(|err| {
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

/// A signature emitted by an authority. It's useful to decouple this from user signatures,
/// as their set of supported schemes will probably diverge
#[serde_as]
#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuthoritySignature(
    #[schemars(with = "json_schema::Base64")]
    #[serde_as(as = "Readable<Base64, _>")]
    pub dalek::Signature,
);
impl AsRef<[u8]> for AuthoritySignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl signature::Signature for AuthoritySignature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let sig = dalek::Signature::from_bytes(bytes)?;
        Ok(AuthoritySignature(sig))
    }
}

impl AuthoritySignature {
    /// Signs with the provided Signer
    ///
    pub fn new<T>(value: &T, secret: &dyn signature::Signer<AuthoritySignature>) -> Self
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        secret.sign(&message)
    }

    /// Signature verification for a single signature
    pub fn check<T>(&self, value: &T, author: PublicKeyBytes) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // is this a cryptographically valid public Key?
        let public_key: PublicKey = author.try_into()?;

        // access the signature
        let signature = self.0;

        // serialize the message (see BCS serialization for determinism)
        let mut message = Vec::new();
        value.write(&mut message);

        // perform cryptographic signature check
        public_key
            .verify(&message, &signature)
            .map_err(|error| SuiError::InvalidSignature {
                error: error.to_string(),
            })
    }

    /// This performs signature verification for a batch of signatures, equipped with a trusted cache of deserialized Public Keys that
    /// represent their provided authors.
    ///
    /// We check that the provided signatures are valid w.r.t the provided public keys, but do not match them to any Sui Addresses.
    pub fn verify_batch<T, I, K>(
        value: &T,
        votes: I,
        key_cache: &HashMap<PublicKeyBytes, PublicKey>,
    ) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
        I: IntoIterator<Item = K>,
        K: Borrow<(PublicKeyBytes, AuthoritySignature)>,
    {
        let mut msg = Vec::new();
        value.write(&mut msg);
        let mut messages: Vec<&[u8]> = Vec::new();
        let mut signatures: Vec<dalek::Signature> = Vec::new();
        let mut public_keys: Vec<dalek::PublicKey> = Vec::new();
        for tuple in votes.into_iter() {
            let (authority, signature) = tuple.borrow();
            // do we know, or can we build a valid public key?
            match key_cache.get(authority) {
                Some(v) => public_keys.push(*v),
                None => {
                    let public_key = (*authority).try_into()?;
                    public_keys.push(public_key);
                }
            }

            // build a signature
            signatures.push(signature.0);

            // collect the message
            messages.push(&msg);
        }
        dalek::verify_batch(&messages[..], &signatures[..], &public_keys[..]).map_err(|error| {
            SuiError::InvalidSignature {
                error: format!("{error}"),
            }
        })
    }
}

/// AuthoritySignInfoTrait is a trait used specifically for a few structs in messages.rs
/// to template on whether the struct is signed by an authority. We want to limit how
/// those structs can be instanted on, hence the sealed trait.
/// TODO: We could also add the aggregated signature as another impl of the trait.
///       This will make CertifiedTransaction also an instance of the same struct.
pub trait AuthoritySignInfoTrait: private::SealedAuthoritySignInfoTrait {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EmptySignInfo {}
impl AuthoritySignInfoTrait for EmptySignInfo {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthoritySignInfo {
    pub epoch: EpochId,
    pub authority: AuthorityName,
    pub signature: AuthoritySignature,
}
impl AuthoritySignInfoTrait for AuthoritySignInfo {}

mod private {
    pub trait SealedAuthoritySignInfoTrait {}
    impl SealedAuthoritySignInfoTrait for super::EmptySignInfo {}
    impl SealedAuthoritySignInfoTrait for super::AuthoritySignInfo {}
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

pub type PubKeyLookup = HashMap<PublicKeyBytes, PublicKey>;

pub fn sha3_hash<S: Signable<Sha3_256>>(signable: &S) -> [u8; 32] {
    let mut digest = Sha3_256::default();
    signable.write(&mut digest);
    let hash = digest.finalize();
    hash.into()
}

#[derive(Default)]
pub struct VerificationObligation {
    lookup: PubKeyLookup,
    pub messages: Vec<Vec<u8>>,
    pub message_index: Vec<usize>,
    pub signatures: Vec<dalek::Signature>,
    pub public_keys: Vec<dalek::PublicKey>,
}

impl VerificationObligation {
    pub fn new(lookup: PubKeyLookup) -> VerificationObligation {
        VerificationObligation {
            lookup,
            ..Default::default()
        }
    }

    pub fn lookup_public_key(&mut self, key_bytes: &PublicKeyBytes) -> Result<PublicKey, SuiError> {
        match self.lookup.get(key_bytes) {
            Some(v) => Ok(*v),
            None => {
                let public_key = (*key_bytes).try_into()?;
                self.lookup.insert(*key_bytes, public_key);
                Ok(public_key)
            }
        }
    }

    pub fn verify_all(self) -> SuiResult<PubKeyLookup> {
        let messages_inner: Vec<_> = self
            .message_index
            .iter()
            .map(|idx| &self.messages[*idx][..])
            .collect();
        dalek::verify_batch(
            &messages_inner[..],
            &self.signatures[..],
            &self.public_keys[..],
        )
        .map_err(|error| SuiError::InvalidSignature {
            error: format!("{error}"),
        })?;

        Ok(self.lookup)
    }
}
