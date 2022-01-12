// Copyright(C) 2022, Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::traits::{EncodeDecodeBase64, ToFromBytes};
use ::ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_bls12_377::{Fr, G1Affine, G1Projective, G2Affine, G2Projective};
use ark_ec::{AffineCurve, ProjectiveCurve};
use ark_ff::{
    bytes::{FromBytes, ToBytes},
    Zero,
};
use celo_bls::PublicKey;
use once_cell::sync::OnceCell;
use serde::{de, Deserialize, Serialize};
use serde_with::serde_as;
use signature::{Signer, Verifier};

use crate::traits::{Authenticator, KeyPair, SigningKey, VerifyingKey};

mod ark_serialize;

mod rng_wrapper;

pub const CELO_BLS_PRIVATE_KEY_LENGTH: usize = 32;
pub const CELO_BLS_PUBLIC_KEY_LENGTH: usize = 96;
pub const CELO_BLS_SIGNATURE_LENGTH: usize = 48;

#[readonly::make]
#[derive(Debug, Clone)]
pub struct BLS12377PublicKey {
    pub pubkey: celo_bls::PublicKey,
    pub bytes: OnceCell<[u8; CELO_BLS_PUBLIC_KEY_LENGTH]>,
}

#[readonly::make]
#[derive(Debug)]
pub struct BLS12377PrivateKey {
    pub privkey: celo_bls::PrivateKey,
    pub bytes: OnceCell<[u8; CELO_BLS_PRIVATE_KEY_LENGTH]>,
}

#[readonly::make]
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BLS12377Signature {
    #[serde_as(as = "ark_serialize::SerdeAs")]
    pub sig: celo_bls::Signature,
    #[serde(skip)]
    #[serde(default = "OnceCell::new")]
    pub bytes: OnceCell<[u8; CELO_BLS_SIGNATURE_LENGTH]>,
}

impl signature::Signature for BLS12377Signature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let g1 = <G1Affine as CanonicalDeserialize>::deserialize(bytes)
            .map_err(|_| signature::Error::new())?;
        Ok(BLS12377Signature {
            sig: g1.into_projective().into(),
            bytes: OnceCell::new(),
        })
    }
}
// see [#34](https://github.com/MystenLabs/narwhal/issues/34)
impl Default for BLS12377Signature {
    fn default() -> Self {
        let g1 = G1Projective::zero();
        BLS12377Signature {
            sig: g1.into(),
            bytes: OnceCell::new(),
        }
    }
}

impl AsRef<[u8]> for BLS12377Signature {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| {
                let mut bytes = [0u8; CELO_BLS_SIGNATURE_LENGTH];
                self.sig.as_ref().into_affine().serialize(&mut bytes[..])?;
                Ok(bytes)
            })
            .expect("OnceCell invariant violated")
    }
}

impl PartialEq for BLS12377Signature {
    fn eq(&self, other: &Self) -> bool {
        self.sig == other.sig
    }
}

impl Eq for BLS12377Signature {}

impl Authenticator for BLS12377Signature {
    type PubKey = BLS12377PublicKey;

    type PrivKey = BLS12377PrivateKey;
}

impl Default for BLS12377PublicKey {
    // eprint.iacr.org/2021/323 should incite us to remove our usage of Default,
    // see https://github.com/MystenLabs/narwhal/issues/34
    fn default() -> Self {
        let public: PublicKey = G2Projective::zero().into();
        BLS12377PublicKey {
            pubkey: public,
            bytes: OnceCell::new(),
        }
    }
}

impl std::hash::Hash for BLS12377PublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.pubkey.hash(state);
    }
}

impl PartialEq for BLS12377PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.pubkey == other.pubkey
    }
}

impl Eq for BLS12377PublicKey {}

impl AsRef<[u8]> for BLS12377PublicKey {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| {
                let mut bytes = [0u8; CELO_BLS_PUBLIC_KEY_LENGTH];
                self.pubkey
                    .as_ref()
                    .into_affine()
                    .serialize(&mut bytes[..])
                    .unwrap();
                Ok(bytes)
            })
            .expect("OnceCell invariant violated")
    }
}

impl PartialOrd for BLS12377PublicKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.as_ref().partial_cmp(other.as_ref())
    }
}

impl Ord for BLS12377PublicKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl ToFromBytes for BLS12377PublicKey {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let g2 = <G2Affine as CanonicalDeserialize>::deserialize(bytes)
            .map_err(|_| signature::Error::new())?
            .into_projective();
        Ok(BLS12377PublicKey {
            pubkey: g2.into(),
            bytes: OnceCell::new(),
        })
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
impl Serialize for BLS12377PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.encode_base64())
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
impl<'de> Deserialize<'de> for BLS12377PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s = <String as serde::Deserialize>::deserialize(deserializer)?;
        let value = Self::decode_base64(&s).map_err(|e| de::Error::custom(e.to_string()))?;
        Ok(value)
    }
}

impl Verifier<BLS12377Signature> for BLS12377PublicKey {
    fn verify(&self, msg: &[u8], signature: &BLS12377Signature) -> Result<(), signature::Error> {
        let hash_to_g1 = &*celo_bls::hash_to_curve::try_and_increment::COMPOSITE_HASH_TO_G1;

        self.pubkey
            .verify(msg, &[], &signature.sig, hash_to_g1)
            .map_err(|_| signature::Error::new())
    }
}

impl VerifyingKey for BLS12377PublicKey {
    type PrivKey = BLS12377PrivateKey;

    type Sig = BLS12377Signature;

    fn verify_batch(msg: &[u8], pks: &[Self], sigs: &[Self::Sig]) -> Result<(), signature::Error> {
        if pks.len() != sigs.len() {
            return Err(signature::Error::new());
        }
        let mut batch = celo_bls::bls::Batch::new(msg, &[]);
        pks.iter()
            .zip(sigs)
            .for_each(|(pk, sig)| batch.add(pk.pubkey.clone(), sig.sig.clone()));
        let hash_to_g1 = &*celo_bls::hash_to_curve::try_and_increment::COMPOSITE_HASH_TO_G1;
        batch
            .verify(hash_to_g1)
            .map_err(|_| signature::Error::new())
    }
}

impl AsRef<[u8]> for BLS12377PrivateKey {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| {
                let mut bytes = [0u8; CELO_BLS_PRIVATE_KEY_LENGTH];
                self.privkey.as_ref().write(&mut bytes[..])?;
                Ok(bytes)
            })
            .expect("OnceCell invariant violated")
    }
}

impl ToFromBytes for BLS12377PrivateKey {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let fr = <Fr as FromBytes>::read(bytes).map_err(|_| signature::Error::new())?;
        Ok(BLS12377PrivateKey {
            privkey: fr.into(),
            bytes: OnceCell::new(),
        })
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
impl Serialize for BLS12377PrivateKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.encode_base64())
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
impl<'de> Deserialize<'de> for BLS12377PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s = <String as serde::Deserialize>::deserialize(deserializer)?;
        let value = Self::decode_base64(&s).map_err(|e| de::Error::custom(e.to_string()))?;
        Ok(value)
    }
}

impl SigningKey for BLS12377PrivateKey {
    type PubKey = BLS12377PublicKey;

    type Sig = BLS12377Signature;
}

impl Signer<BLS12377Signature> for BLS12377PrivateKey {
    fn try_sign(&self, msg: &[u8]) -> Result<BLS12377Signature, signature::Error> {
        let hash_to_g1 = &*celo_bls::hash_to_curve::try_and_increment::COMPOSITE_HASH_TO_G1;

        let celo_bls_sig = self
            .privkey
            .sign(msg, &[], hash_to_g1)
            .map_err(|_| signature::Error::new())?;

        Ok(BLS12377Signature {
            sig: celo_bls_sig,
            bytes: OnceCell::new(),
        })
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")] // necessary so as not to deser under a != type
pub struct BLS12377KeyPair {
    name: BLS12377PublicKey,
    secret: BLS12377PrivateKey,
}

impl KeyPair for BLS12377KeyPair {
    type PubKey = BLS12377PublicKey;

    type PrivKey = BLS12377PrivateKey;

    fn public(&'_ self) -> &'_ Self::PubKey {
        &self.name
    }

    fn private(self) -> Self::PrivKey {
        self.secret
    }

    fn generate<R: rand::CryptoRng + rand::RngCore>(rng: &mut R) -> Self {
        let celo_privkey = celo_bls::PrivateKey::generate(&mut rng_wrapper::RngWrapper(rng));
        let celo_pubkey = (&celo_privkey).into();
        BLS12377KeyPair {
            name: BLS12377PublicKey {
                pubkey: celo_pubkey,
                bytes: OnceCell::new(),
            },
            secret: BLS12377PrivateKey {
                privkey: celo_privkey,
                bytes: OnceCell::new(),
            },
        }
    }
}

impl Signer<BLS12377Signature> for BLS12377KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<BLS12377Signature, signature::Error> {
        let hash_to_g1 = &*celo_bls::hash_to_curve::try_and_increment::COMPOSITE_HASH_TO_G1;

        let celo_bls_sig = self
            .secret
            .privkey
            .sign(msg, &[], hash_to_g1)
            .map_err(|_| signature::Error::new())?;

        Ok(BLS12377Signature {
            sig: celo_bls_sig,
            bytes: OnceCell::new(),
        })
    }
}
