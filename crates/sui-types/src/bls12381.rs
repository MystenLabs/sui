// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::base_types::{AuthorityName, SuiAddress};
use crate::committee::{Committee, EpochId};
use crate::crypto::{AuthoritySignInfoTrait};
use crate::crypto_traits::Signable;
use crate::error::{SuiError, SuiResult};
use crate::sui_serde::Base64;
use crate::sui_serde::Readable;
use crate::sui_serde::BlsSignature;
use anyhow::anyhow;
use anyhow::Error;
use base64ct::Encoding;
use digest::Digest;
use ed25519_dalek::{Keypair as DalekKeypair, Verifier};
use narwhal_crypto::ed25519::{Ed25519KeyPair, Ed25519PrivateKey, Ed25519PublicKey};
use once_cell::sync::OnceCell;
use rand::rngs::OsRng;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::Bytes;
use sha3::Sha3_256;
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::slice;
use std::str::FromStr;
use std::{
    fmt::{self, Display},
    mem::MaybeUninit,
};
use hkdf::Hkdf;

use blst::{min_pk as bls, BLST_ERROR};
use ::blst::{blst_scalar, blst_scalar_from_uint64};
use rand::{RngCore};

pub const BLST_SK_SIZE: usize = 32;
pub const BLST_PK_SIZE: usize = 48;
pub const BLST_SIG_SIZE: usize = 96;

pub type Bls12381PublicKey = bls::PublicKey;

#[derive(Debug)]
pub struct BlstKeypair {
    pub public: bls::PublicKey,
    pub secret: bls::SecretKey 
}

impl BlstKeypair {
    fn to_bytes(&self) -> Vec<u8> {
        let key_concat: Vec<u8> = self.public.to_bytes()
        .iter().cloned()
        .chain(self.secret.to_bytes().iter().cloned())
        .collect();
        key_concat
    }

    fn from_bytes(bytes: &[u8]) -> Result<BlstKeypair, BLST_ERROR> {
        let public = bls::PublicKey::from_bytes(&bytes[..BLST_PK_SIZE])?;
        let secret = bls::SecretKey::from_bytes(&bytes[BLST_PK_SIZE..])?;
        Ok(BlstKeypair {
            public,
            secret
        })
    }
}

#[derive(Debug)]
pub struct Bls12381KeyPair {
    pub key_pair: BlstKeypair,
    pub public_key_cell: OnceCell<Bls12381PublicKeyBytes>,
}

impl Bls12381KeyPair {
    pub fn public_key_bytes(&self) -> &Bls12381PublicKeyBytes {
        self.public_key_cell.get_or_init(|| {
            let pk_arr: [u8; BLST_PK_SIZE] = self.key_pair.public.to_bytes();
            Bls12381PublicKeyBytes(pk_arr)
        })
    }

    #[must_use]
    pub fn copy(&self) -> Self {
        Bls12381KeyPair {
            key_pair: BlstKeypair {
                public: bls::PublicKey::from_bytes(&self.key_pair.public.to_bytes()).unwrap(),
                secret: bls::SecretKey::from_bytes(&self.key_pair.secret.to_bytes()).unwrap(),
            },
            public_key_cell: OnceCell::new(),
        }
    }

    pub fn new_deterministic_keypair(seed: &[u8], id: &[u8], domain: &[u8]) -> SuiResult<Bls12381KeyPair>{
        // HKDF<Sha3_256> to deterministically generate an ed25519 private key.
        let hk = Hkdf::<Sha3_256>::new(Some(id), seed);
        let mut okm = [0u8; BLST_SK_SIZE];
        hk.expand(domain, &mut okm)
            .map_err(|e| SuiError::HkdfError(e.to_string()))?;

        let secret = bls::SecretKey::key_gen(&okm, &[])
            .map_err(|e| SuiError::SignatureKeyGenError("Error Filler".to_string()))?;
        let public = secret.sk_to_pk();

        Ok(Bls12381KeyPair {
            key_pair: BlstKeypair {
                public,
                secret 
            },
            public_key_cell: OnceCell::new()
        })
    }

    /// Make a Narwhal-compatible key pair from a Sui keypair.
    pub fn make_narwhal_keypair(&self) -> Ed25519KeyPair {
        let kp = DalekKeypair::generate(&mut OsRng);
        Ed25519KeyPair {
            name: Ed25519PublicKey(kp.public),
            secret: Ed25519PrivateKey(kp.secret),
        }
    }
}

impl Serialize for Bls12381KeyPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let key_concat: Vec<u8> = self.key_pair.public.to_bytes()
            .iter().cloned()
            .chain(self.key_pair.secret.to_bytes().iter().cloned())
            .collect();
        serializer.serialize_str(&base64ct::Base64::encode_string(&key_concat[..]))
    }
}

impl<'de> Deserialize<'de> for Bls12381KeyPair {
    fn deserialize<D>(deserializer: D) -> Result<Bls12381KeyPair, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = base64ct::Base64::decode_vec(&s)
            .map_err(|err| serde::de::Error::custom(err.to_string()))?;
        let public = bls::PublicKey::from_bytes(&value[..BLST_PK_SIZE])
            .map_err(|err| serde::de::Error::custom("BLST FAILURE"))?;
        let secret = bls::SecretKey::from_bytes(&value[BLST_PK_SIZE..])
            .map_err(|err| serde::de::Error::custom("BLST FAILURE"))?;
        Ok(Bls12381KeyPair {
            key_pair: BlstKeypair { public, secret},
            public_key_cell: OnceCell::<Bls12381PublicKeyBytes>::new(),
        })
    }
}

impl FromStr for Bls12381KeyPair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = base64ct::Base64::decode_vec(s).map_err(|e| anyhow!("{}", e.to_string()))?;
        let key = BlstKeypair::from_bytes(&value).map_err(|e| anyhow!(format!("{:?}", e)))?;
        Ok(Bls12381KeyPair{
            key_pair: key,
            public_key_cell: OnceCell::<Bls12381PublicKeyBytes>::new(),
        })
    }
}

pub const DST: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";


impl signature::Signer<Bls12381Signature> for Bls12381KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Bls12381Signature, signature::Error> {
        let signature_bytes = &self.key_pair.secret.sign(msg, DST, &[]).to_bytes();
        let public_key_bytes = self.public_key_bytes().as_ref();
        let mut result_bytes = [0u8; SUI_SIGNATURE_LENGTH];
        result_bytes[..BLST_SIG_SIZE].copy_from_slice(signature_bytes);
        result_bytes[BLST_SIG_SIZE..].copy_from_slice(public_key_bytes);
        Ok(Bls12381Signature(result_bytes))
    }
}

impl signature::Signer<Bls12381AuthoritySignature> for Bls12381KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Bls12381AuthoritySignature, signature::Error> {
        let sig = self.key_pair.secret.sign(msg, DST, &[]);
        Ok(Bls12381AuthoritySignature(sig))
    }
}

#[serde_as]
#[derive(
    Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct Bls12381PublicKeyBytes(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; BLST_PK_SIZE],
);

impl Default for Bls12381PublicKeyBytes {
    fn default() -> Self {
        Bls12381PublicKeyBytes([0; BLST_PK_SIZE])
    }
}

impl Bls12381PublicKeyBytes {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
    /// Make a Narwhal-compatible public key from a Sui pub.
    pub fn make_narwhal_public_key(&self) -> Result<Ed25519PublicKey, signature::Error> {
        let kp = DalekKeypair::generate(&mut OsRng);
        Ok(Ed25519PublicKey(kp.public))
    }
}

impl AsRef<[u8]> for Bls12381PublicKeyBytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl TryInto<bls::PublicKey> for Bls12381PublicKeyBytes {
    type Error = SuiError;

    fn try_into(self) -> Result<bls::PublicKey, Self::Error> {
        // TODO(https://github.com/MystenLabs/sui/issues/101): Do better key validation
        // to ensure the bytes represent a point on the curve.
        bls::PublicKey::from_bytes(self.as_ref()).map_err(|_| SuiError::InvalidAuthenticator)
    }
}

// TODO(https://github.com/MystenLabs/sui/issues/101): more robust key validation
impl TryFrom<&[u8]> for Bls12381PublicKeyBytes {
    type Error = SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, SuiError> {
        let arr: [u8; BLST_PK_SIZE] = bytes
            .try_into()
            .map_err(|_| SuiError::InvalidAuthenticator)?;
        Ok(Self(arr))
    }
}

impl std::fmt::Debug for Bls12381PublicKeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = hex::encode(&self.0);
        write!(f, "k#{}", s)?;
        Ok(())
    }
}

impl Bls12381KeyPair {
    pub fn random_key_pairs(num: usize) -> Vec<Bls12381KeyPair> {
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
        
    // TODO: get_key_pair() and get_key_pair_from_bytes() should return KeyPair only.
    // TODO: rename to random_key_pair
    pub fn get_key_pair() -> (SuiAddress, Bls12381KeyPair) {
    
        Self::get_key_pair_from_rng(&mut OsRng)
    }
    
    /// Generate a keypair from the specified RNG (useful for testing with seedable rngs).
    pub fn get_key_pair_from_rng<R>(csprng: &mut R) -> (SuiAddress, Bls12381KeyPair)
    where
        R: rand::CryptoRng + rand::RngCore,
    {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
    
        let secret = bls::SecretKey::key_gen(&seed, &[]).unwrap();
        let public = secret.sk_to_pk();
    
        let keypair = Bls12381KeyPair {
            key_pair: BlstKeypair { public, secret },
            public_key_cell: OnceCell::new(),
        };
        (SuiAddress::from(keypair.public_key_bytes()), keypair)
    }
    
    // TODO: C-GETTER
    pub fn get_key_pair_from_bytes(bytes: &[u8]) -> (SuiAddress, Bls12381KeyPair) {
        let keypair = Bls12381KeyPair {
            key_pair: BlstKeypair { 
                public: bls::PublicKey::from_bytes(&bytes[..BLST_PK_SIZE]).unwrap(),
                secret: bls::SecretKey::from_bytes(&bytes[BLST_PK_SIZE..]).unwrap(), 
            },
            public_key_cell: OnceCell::new(),
        };
        (SuiAddress::from(keypair.public_key_bytes()), keypair)
    }    
}

// 
// Signatures
// 

// TODO: replace this with a byte interpretation based on multicodec
pub const SUI_SIGNATURE_LENGTH: usize =
    BLST_PK_SIZE + BLST_SIG_SIZE;

#[serde_as]
#[derive(Eq, PartialEq, Copy, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Bls12381Signature(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; SUI_SIGNATURE_LENGTH],
);

impl AsRef<[u8]> for Bls12381Signature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl signature::Signature for Bls12381Signature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let val: [u8; SUI_SIGNATURE_LENGTH] =
            bytes.try_into().map_err(|_| signature::Error::new())?;
        Ok(Self(val))
    }
}

impl std::fmt::Debug for Bls12381Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let s = base64ct::Base64::encode_string(self.signature_bytes());
        let p = base64ct::Base64::encode_string(self.public_key_bytes());
        write!(f, "{s}@{p}")?;
        Ok(())
    }
}

impl Bls12381Signature {
    pub fn new<T>(value: &T, secret: &dyn signature::Signer<Bls12381Signature>) -> Self
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
        &self.0[..BLST_SIG_SIZE]
    }

    pub fn public_key_bytes(&self) -> &[u8] {
        &self.0[BLST_SIG_SIZE..]
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
            &bls::PublicKey::from_bytes(public_key_bytes.as_ref()).map_err(|err| {
                SuiError::InvalidSignature {
                    error: "Error".to_string(),
                }
            })?;

        // perform cryptographic signature check
        let err = signature
            .verify(true,&message, DST, &[], public_key, false);
        match err {
            BLST_ERROR::BLST_SUCCESS => Ok(()),
            _ => Err(SuiError::InvalidSignature {
                error: "Invalid signature 1".to_string()
            })
        }
    }

    pub fn get_verification_inputs<T>(
        &self,
        value: &T,
        author: SuiAddress,
    ) -> Result<(Vec<u8>, bls::Signature, Bls12381PublicKeyBytes), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // Is this signature emitted by the expected author?
        let public_key_bytes: [u8; BLST_PK_SIZE] = self
            .public_key_bytes()
            .try_into()
            .expect("byte lengths match");
        let received_addr = SuiAddress::from(&Bls12381PublicKeyBytes(public_key_bytes));
        if received_addr != author {
            return Err(SuiError::IncorrectSigner {
                error: format!("Signature get_verification_inputs() failure. Author is {author}, received address is {received_addr}")
            });
        }

        // deserialize the signature
        let signature =
            bls::Signature::from_bytes(self.signature_bytes()).map_err(|err| {
                SuiError::InvalidSignature {
                    error: "Error".to_string(),
                }
            })?;

        // serialize the message (see BCS serialization for determinism)
        let mut message = Vec::new();
        value.write(&mut message);

        Ok((message, signature, Bls12381PublicKeyBytes(public_key_bytes)))
    }
}

/// A signature emitted by an authority. It's useful to decouple this from user signatures,
/// as their set of supported schemes will probably diverge
#[serde_as]
#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Bls12381AuthoritySignature(
    #[schemars(with = "Base64")]
    #[serde_as(as = "BlsSignature")]
    pub bls::Signature,
);

impl AsRef<[u8]> for Bls12381AuthoritySignature {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            slice::from_raw_parts(&self.0 as *const _ as  *const u8, BLST_SIG_SIZE)
        }
    }
}

impl signature::Signature for Bls12381AuthoritySignature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let sig = bls::Signature::from_bytes(bytes)
            .map_err(|x| signature::Error::new()).unwrap();
        Ok(Bls12381AuthoritySignature(sig))
    }
}

impl Bls12381AuthoritySignature {
    /// Signs with the provided Signer
    ///
    pub fn new<T>(value: &T, secret: &dyn signature::Signer<Bls12381AuthoritySignature>) -> Self
    where
        T: Signable<Vec<u8>>,
    {
        let mut message = Vec::new();
        value.write(&mut message);
        secret.sign(&message)
    }

    /// Signature verification for a single signature
    pub fn verify<T>(&self, value: &T, author: Bls12381PublicKeyBytes) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // is this a cryptographically valid public Key?
        let public_key: bls::PublicKey = author.try_into()?;

        // access the signature
        let signature = self.0;

        // serialize the message (see BCS serialization for determinism)
        let mut message = Vec::new();
        value.write(&mut message);

        // perform cryptographic signature check
        let err = signature
            .verify(true, &message, DST, &[], &public_key, true);
        match err {
            BLST_ERROR::BLST_SUCCESS => Ok(()),
            _ => Err(SuiError::InvalidSignature {
                error: "Invalid signature 2".to_string()
            })
        }
    }

    /// This performs signature verification for a batch of signatures, equipped with a trusted cache of deserialized Public Keys that
    /// represent their provided authors.
    ///
    /// We check that the provided signatures are valid w.r.t the provided public keys, but do not match them to any Sui Addresses.
    pub fn verify_batch<T, I, K>(
        value: &T,
        votes: I,
        key_cache: &HashMap<Bls12381PublicKeyBytes, bls::PublicKey>,
    ) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
        I: IntoIterator<Item = K>,
        K: Borrow<(Bls12381PublicKeyBytes, Bls12381AuthoritySignature)>,
    {
        let mut msg = Vec::new();
        value.write(&mut msg);
        let mut messages: Vec<&[u8]> = Vec::new();
        let mut signatures: Vec<bls::Signature> = Vec::new();
        let mut public_keys: Vec<bls::PublicKey> = Vec::new();
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

        let aggregated_signature = bls::AggregateSignature::aggregate(&signatures.iter().map(|x| x).collect::<Vec<_>>()[..], true)
            .map_err(|err| SuiError::InvalidSignature {
                error: format!("Invalid signature aggregation 3")
            })?
        .to_signature();

        let verify = aggregated_signature.aggregate_verify(
            true, 
            &messages[..],
            DST, 
            &public_keys.iter().map(|x| x).collect::<Vec<_>>()[..],
            true
        );
        match verify {
            BLST_ERROR::BLST_SUCCESS => {
                Ok(())
            },
            _ => Err(SuiError::InvalidSignature {
                error: format!("ERROR 4"),
            })
        }
    }
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct Bls12381AuthoritySignInfo {
    pub epoch: EpochId,
    pub authority: Bls12381PublicKeyBytes,
    pub signature: Bls12381AuthoritySignature,
}

impl Hash for Bls12381AuthoritySignInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch.hash(state);
        self.authority.hash(state);
    }
}

impl PartialEq for Bls12381AuthoritySignInfo {
    fn eq(&self, other: &Self) -> bool {
        // We do not compare the signature, because there can be multiple
        // valid signatures for the same epoch and authority.
        self.epoch == other.epoch && self.authority == other.authority
    }
}

impl Bls12381AuthoritySignInfo {
    pub fn verify(
        &self,
        message: Vec<u8>
    ) -> SuiResult<()> {
        let key: bls::PublicKey = self.authority.try_into()?;
        let result = self.signature.0.verify(
            true,
            &message[..],
            DST,
            &[],
            &key,
            true
        );
        if result == BLST_ERROR::BLST_SUCCESS {
            Ok(())
        } else {
            Err(SuiError::InvalidSignature {
                error: format!("ERROR {:?}", result),
            })
        }
    }
}

pub fn aggregate_authority_signatures(
    authority_signatures: &[&Bls12381AuthoritySignature]
) -> Result<Bls12381AuthoritySignature, SuiError> {
    Ok(Bls12381AuthoritySignature(aggregate_bls_signature(
        &authority_signatures.iter().map(|s| s.0).collect::<Vec<_>>()[..]
    )?))
}

pub fn aggregate_bls_signature(signatures: &[bls::Signature]) -> Result<bls::Signature, SuiError> {
    let aggregated_signature = bls::AggregateSignature::aggregate(
        &signatures.iter().map(|sig| sig).collect::<Vec<_>>()[..],
        true
    ).map_err(|e| SuiError::InvalidSignature {
        error: format!("{:?}", e)
    })?;
    Ok(aggregated_signature.to_signature())
}


/// Represents at least a quorum (could be more) of authority signatures.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Bls12381AuthorityQuorumSignInfo<const STRONG_THRESHOLD: bool> {
    pub epoch: EpochId,
    pub authorities: Vec<Bls12381PublicKeyBytes>,
    pub aggregated_signature: Option<Bls12381AuthoritySignature>,
}

impl<const STRONG_THRESHOLD: bool> Bls12381AuthorityQuorumSignInfo<STRONG_THRESHOLD> {
    pub fn new(
        epoch: EpochId
    ) -> Self {
        Bls12381AuthorityQuorumSignInfo {
            epoch: epoch,
            authorities: vec![],
            aggregated_signature: None
        } 
    }

    pub fn new_with_signatures(
        epoch: EpochId,
        signatures: Vec<(Bls12381PublicKeyBytes, Bls12381AuthoritySignature)>
    ) -> SuiResult<Self> {
        Ok(Bls12381AuthorityQuorumSignInfo {
            epoch: epoch,
            authorities: signatures.iter().map(|(pk, _)| *pk).collect::<Vec<_>>(),
            aggregated_signature: Some(aggregate_authority_signatures(
                &signatures.iter().map(|(_, sig)| sig).collect::<Vec<_>>()[..]
            )?)
        })
    }

    pub fn authorities(&self) -> Vec<Bls12381PublicKeyBytes> {
        self.authorities.clone()
    }

    pub fn signatures(&self) -> &Option<Bls12381AuthoritySignature> {
        &self.aggregated_signature
    }

    pub fn add_signature(
        &mut self,
        signature: Bls12381AuthoritySignature,
        authority: Bls12381PublicKeyBytes
    ) -> SuiResult<()> { 
        self.aggregated_signature = match self.aggregated_signature {
            Some(prev_sig) => {
                let mut aggr_sig = blst::min_pk::AggregateSignature::from_signature(&prev_sig.0);
                aggr_sig.add_signature(&signature.0, true)
                .map_err(|err| SuiError::InvalidSignature { error: format!("{:?}", err) })?;
                Some(Bls12381AuthoritySignature(aggr_sig.to_signature()))
            }
            None => {
                Some(signature)
            }
        };
        self.authorities.push(authority);
        Ok(())
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
        let mut authorities: Vec<bls::PublicKey> = Vec::new();

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

            authorities.push(committee.public_key(authority)?);
        }

        let threshold = if STRONG_THRESHOLD {
            committee.quorum_threshold()
        } else {
            committee.validity_threshold()
        };
        fp_ensure!(weight >= threshold, SuiError::CertificateRequiresQuorum);

        let result = match self.aggregated_signature {
            Some(signature) => {
                signature.0.fast_aggregate_verify(
                    true,
                    &message[..],
                    DST,
                    &authorities.iter().map(|pk| pk).collect::<Vec<_>>()[..]
                )
            }
            None => {
                return Err(SuiError::InvalidSignature {
                    error: format!("Empty Signature")
                })
            }
        };

        if result == BLST_ERROR::BLST_SUCCESS {
            Ok(())
        } else {
            Err(SuiError::InvalidSignature {
                error: format!("ERROR {:?}", result),
            })
        }
    }    
}
