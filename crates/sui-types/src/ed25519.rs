// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::base_types::{AuthorityName, SuiAddress};
use crate::committee::{Committee, EpochId};
use crate::error::{SuiError, SuiResult};
use crate::sui_serde::Base64;
use crate::sui_serde::Readable;
use anyhow::anyhow;
use base64ct::Encoding;
use ed25519_dalek as dalek;
use ed25519_dalek::{Keypair as DalekKeypair, Verifier};
use narwhal_crypto::ed25519::{Ed25519KeyPair as NarwhalEd25519KeyPair, Ed25519PrivateKey as NarwhalEd25519PrivateKey, Ed25519PublicKey as NarwhalEd25519PublicKey};
use once_cell::sync::OnceCell;
use rand::rngs::OsRng;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::Bytes;
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use crate::crypto_traits::Signable;

// TODO: Make sure secrets are not copyable and movable to control where they are in memory
#[derive(Debug)]
pub struct Ed25519KeyPair {
    key_pair: DalekKeypair,
    public_key_cell: OnceCell<Ed25519PublicKeyBytes>,
}

impl Ed25519KeyPair {
    pub fn public_key_bytes(&self) -> &Ed25519PublicKeyBytes {
        self.public_key_cell.get_or_init(|| {
            let pk_arr: [u8; ed25519_dalek::PUBLIC_KEY_LENGTH] = self.key_pair.public.to_bytes();
            Ed25519PublicKeyBytes(pk_arr)
        })
    }

    // TODO: eradicate unneeded uses (i.e. most)
    /// Avoid implementing `clone` on secret keys to prevent mistakes.
    #[must_use]
    pub fn copy(&self) -> Ed25519KeyPair {
        Ed25519KeyPair {
            key_pair: DalekKeypair {
                secret: dalek::SecretKey::from_bytes(self.key_pair.secret.as_bytes()).unwrap(),
                public: dalek::PublicKey::from_bytes(self.public_key_bytes().as_ref()).unwrap(),
            },
            public_key_cell: OnceCell::new(),
        }
    }

    /// Make a Narwhal-compatible key pair from a Sui keypair.
    pub fn make_narwhal_keypair(&self) -> NarwhalEd25519KeyPair {
        let key = self.copy();
        NarwhalEd25519KeyPair {
            name: NarwhalEd25519PublicKey(key.key_pair.public),
            secret: NarwhalEd25519PrivateKey(key.key_pair.secret),
        }
    }
}

impl From<DalekKeypair> for Ed25519KeyPair {
    fn from(dalek_keypair: DalekKeypair) -> Self {
        Self {
            key_pair: dalek_keypair,
            public_key_cell: OnceCell::new(),
        }
    }
}

impl Serialize for Ed25519KeyPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&base64ct::Base64::encode_string(&self.key_pair.to_bytes()))
    }
}

impl<'de> Deserialize<'de> for Ed25519KeyPair {
    fn deserialize<D>(deserializer: D) -> Result<Ed25519KeyPair, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = base64ct::Base64::decode_vec(&s)
            .map_err(|err| serde::de::Error::custom(err.to_string()))?;
        let key = DalekKeypair::from_bytes(&value)
            .map_err(|err| serde::de::Error::custom(err.to_string()))?;
        Ok(Ed25519KeyPair {
            key_pair: key,
            public_key_cell: OnceCell::new(),
        })
    }
}

impl FromStr for Ed25519KeyPair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = base64ct::Base64::decode_vec(s).map_err(|e| anyhow!("{}", e.to_string()))?;
        let key = dalek::Keypair::from_bytes(&value).map_err(|e| anyhow!("{}", e.to_string()))?;
        Ok(Ed25519KeyPair {
            key_pair: key,
            public_key_cell: OnceCell::new(),
        })
    }
}

impl signature::Signer<Ed25519Signature> for Ed25519KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Ed25519Signature, signature::Error> {
        let signature_bytes = self.key_pair.try_sign(msg)?;
        let public_key_bytes = self.public_key_bytes().as_ref();
        let mut result_bytes = [0u8; SUI_SIGNATURE_LENGTH];
        result_bytes[..ed25519_dalek::SIGNATURE_LENGTH].copy_from_slice(signature_bytes.as_ref());
        result_bytes[ed25519_dalek::SIGNATURE_LENGTH..].copy_from_slice(public_key_bytes);
        Ok(Ed25519Signature(result_bytes))
    }
}

impl signature::Signer<Ed25519AuthoritySignature> for Ed25519KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Ed25519AuthoritySignature, signature::Error> {
        let sig = self.key_pair.try_sign(msg)?;
        Ok(Ed25519AuthoritySignature(sig))
    }
}

#[serde_as]
#[derive(
    Eq, Default, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct Ed25519PublicKeyBytes(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; dalek::PUBLIC_KEY_LENGTH],
);

impl Ed25519PublicKeyBytes {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
    /// Make a Narwhal-compatible public key from a Sui pub.
    pub fn make_narwhal_public_key(&self) -> Result<NarwhalEd25519PublicKey, signature::Error> {
        let pub_key = dalek::PublicKey::from_bytes(&self.0)?;
        Ok(NarwhalEd25519PublicKey(pub_key))
    }
}

impl AsRef<[u8]> for Ed25519PublicKeyBytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl TryInto<dalek::PublicKey> for Ed25519PublicKeyBytes {
    type Error = SuiError;

    fn try_into(self) -> Result<dalek::PublicKey, Self::Error> {
        // TODO(https://github.com/MystenLabs/sui/issues/101): Do better key validation
        // to ensure the bytes represent a point on the curve.
        dalek::PublicKey::from_bytes(self.as_ref()).map_err(|_| SuiError::InvalidAuthenticator)
    }
}

// TODO(https://github.com/MystenLabs/sui/issues/101): more robust key validation
impl TryFrom<&[u8]> for Ed25519PublicKeyBytes {
    type Error = SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, SuiError> {
        let arr: [u8; dalek::PUBLIC_KEY_LENGTH] = bytes
            .try_into()
            .map_err(|_| SuiError::InvalidAuthenticator)?;
        Ok(Self(arr))
    }
}

impl std::fmt::Debug for Ed25519PublicKeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = hex::encode(&self.0);
        write!(f, "k#{}", s)?;
        Ok(())
    }
}

impl Ed25519KeyPair {
    // TODO: get_key_pair() and get_key_pair_from_bytes() should return Ed25519KeyPair only.
    // TODO: rename to random_key_pair
    pub fn get_key_pair() -> (SuiAddress, Ed25519KeyPair) {
        Self::get_key_pair_from_rng(&mut OsRng)
    }

    /// Generate a keypair from the specified RNG (useful for testing with seedable rngs).
    pub fn get_key_pair_from_rng<R>(csprng: &mut R) -> (SuiAddress, Ed25519KeyPair)
    where
        R: rand::CryptoRng + rand::RngCore,
    {
        let kp = DalekKeypair::generate(csprng);
        let keypair = Ed25519KeyPair {
            key_pair: kp,
            public_key_cell: OnceCell::new(),
        };
        (SuiAddress::from(keypair.public_key_bytes()), keypair)
    }

    // TODO: C-GETTER
    pub fn get_key_pair_from_bytes(bytes: &[u8]) -> (SuiAddress, Ed25519KeyPair) {
        let keypair = Ed25519KeyPair {
            key_pair: DalekKeypair::from_bytes(bytes).unwrap(),
            public_key_cell: OnceCell::new(),
        };
        (SuiAddress::from(keypair.public_key_bytes()), keypair)
    }

    pub fn random_key_pairs(num: usize) -> Vec<Ed25519KeyPair> {
        let mut items = num;
        let mut rng = OsRng;
    
        std::iter::from_fn(|| {
            if items == 0 {
                None
            } else {
                items -= 1;
                Some(Self::get_key_pair_from_rng(&mut rng).1)
            }
        })
        .collect::<Vec<_>>()
    }    
}

// TODO: replace this with a byte interpretation based on multicodec
pub const SUI_SIGNATURE_LENGTH: usize =
    ed25519_dalek::PUBLIC_KEY_LENGTH + ed25519_dalek::SIGNATURE_LENGTH;

#[serde_as]
#[derive(Eq, PartialEq, Copy, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Ed25519Signature(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; SUI_SIGNATURE_LENGTH],
);

impl AsRef<[u8]> for Ed25519Signature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl signature::Signature for Ed25519Signature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let val: [u8; SUI_SIGNATURE_LENGTH] =
            bytes.try_into().map_err(|_| signature::Error::new())?;
        Ok(Self(val))
    }
}

impl std::fmt::Debug for Ed25519Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = base64ct::Base64::encode_string(self.signature_bytes());
        let p = base64ct::Base64::encode_string(self.public_key_bytes());
        write!(f, "{s}@{p}")?;
        Ok(())
    }
}

impl Ed25519Signature {
    pub fn new<T>(value: &T, secret: &dyn signature::Signer<Ed25519Signature>) -> Self
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
        &self.0[..ed25519_dalek::SIGNATURE_LENGTH]
    }

    pub fn public_key_bytes(&self) -> &[u8] {
        &self.0[ed25519_dalek::SIGNATURE_LENGTH..]
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
        let public_key =
            dalek::PublicKey::from_bytes(public_key_bytes.as_ref()).map_err(|err| {
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

    pub fn get_verification_inputs<T>(
        &self,
        value: &T,
        author: SuiAddress,
    ) -> Result<(Vec<u8>, ed25519_dalek::Signature, Ed25519PublicKeyBytes), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // Is this signature emitted by the expected author?
        let public_key_bytes: [u8; ed25519_dalek::PUBLIC_KEY_LENGTH] = self
            .public_key_bytes()
            .try_into()
            .expect("byte lengths match");
        let received_addr = SuiAddress::from(&Ed25519PublicKeyBytes(public_key_bytes));
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

        Ok((message, signature, Ed25519PublicKeyBytes(public_key_bytes)))
    }
}

/// A signature emitted by an authority. It's useful to decouple this from user signatures,
/// as their set of supported schemes will probably diverge
#[serde_as]
#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Ed25519AuthoritySignature(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, _>")]
    pub dalek::Signature,
);
impl AsRef<[u8]> for Ed25519AuthoritySignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl signature::Signature for Ed25519AuthoritySignature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let sig = dalek::Signature::from_bytes(bytes)?;
        Ok(Ed25519AuthoritySignature(sig))
    }
}

impl Ed25519AuthoritySignature {
    /// Signs with the provided Signer
    ///
    pub fn new<T>(value: &T, secret: &dyn signature::Signer<Ed25519AuthoritySignature>) -> Self
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        secret.sign(&message)
    }

    /// Signature verification for a single signature
    pub fn verify<T>(&self, value: &T, author: Ed25519PublicKeyBytes) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // is this a cryptographically valid public Key?
        let public_key: dalek::PublicKey = author.try_into()?;

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
        key_cache: &HashMap<Ed25519PublicKeyBytes, dalek::PublicKey>,
    ) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
        I: IntoIterator<Item = K>,
        K: Borrow<(Ed25519PublicKeyBytes, Ed25519AuthoritySignature)>,
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

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct Ed25519AuthoritySignInfo {
    pub epoch: EpochId,
    pub authority: Ed25519PublicKeyBytes,
    pub signature: Ed25519AuthoritySignature,
}

impl Hash for Ed25519AuthoritySignInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch.hash(state);
        self.authority.hash(state);
    }
}

impl PartialEq for Ed25519AuthoritySignInfo {
    fn eq(&self, other: &Self) -> bool {
        // We do not compare the signature, because there can be multiple
        // valid signatures for the same epoch and authority.
        self.epoch == other.epoch && self.authority == other.authority
    }
}

impl Ed25519AuthoritySignInfo {
    pub fn verify(
        &self,
        message: Vec<u8>
    ) -> SuiResult<()> {
        let key: dalek::PublicKey = self.authority.try_into()?;
        key.verify(
            &message[..],
            &self.signature.0,
        ).map_err(|err| SuiError::InvalidSignature { error: format!("{:?}", err) })
    }
}

/// Represents at least a quorum (could be more) of authority signatures.
/// STRONG_THRESHOLD indicates whether to use the quorum threshold for quorum check.
/// When STRONG_THRESHOLD is true, the quorum is valid when the total stake is
/// at least the quorum threshold (2f+1) of the committee; when STRONG_THRESHOLD is false,
/// the quorum is valid when the total stake is at least the validity threshold (f+1) of
/// the committee.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Ed25519AuthorityQuorumSignInfo<const STRONG_THRESHOLD: bool> {
    pub epoch: EpochId,
    pub signatures: Vec<(Ed25519PublicKeyBytes, Ed25519AuthoritySignature)>
}

impl<const STRONG_THRESHOLD: bool> Ed25519AuthorityQuorumSignInfo<STRONG_THRESHOLD> {
    pub fn new_with_signatures(
        epoch: EpochId,
        signatures: Vec<(Ed25519PublicKeyBytes, Ed25519AuthoritySignature)>
    ) -> SuiResult<Self> {
        Ok(Ed25519AuthorityQuorumSignInfo {
            epoch, signatures
        })
    }

    pub fn verify(
        &self, 
        message: Vec<u8>,
        committee: &Committee
    ) -> Result<(), SuiError> {
        fp_ensure!(
            self.epoch == committee.epoch(),
            SuiError::WrongEpoch {
                expected_epoch: committee.epoch()
            }
        );

        let mut weight = 0;
        let mut used_authorities = HashSet::new();
        let mut authorities: Vec<dalek::PublicKey> = Vec::new();
        let mut messages : Vec<&[u8]> = Vec::new();

        for (authority, _) in self.signatures.iter() {
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

            authorities.push(committee.public_key(authority)?);
            messages.push(&message[..])
        }

        let threshold = if STRONG_THRESHOLD {
            committee.quorum_threshold()
        } else {
            committee.validity_threshold()
        };
        fp_ensure!(weight >= threshold, SuiError::CertificateRequiresQuorum);
        
        dalek::verify_batch(
            &messages
                .iter()
                .map(|message| &message[..])
                .collect::<Vec<_>>()[..],
            &self.signatures
                .iter()
                .map(|(_, sig)| sig.0)
                .collect::<Vec<_>>()[..],
            &authorities
                .iter()
                .map(|pk| *pk)
                .collect::<Vec<_>>()[..]
        )
        .map_err(|error| SuiError::InvalidSignature {
            error: format!("{error}"),
        })?;

        Ok(())
    }
}
