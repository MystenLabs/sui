// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::mem::MaybeUninit;

use ::blst::{blst_scalar, blst_scalar_from_uint64, BLST_ERROR};
use blst::min_sig as blst;
use once_cell::sync::OnceCell;
use rand::{rngs::OsRng, RngCore};

use serde::{
    de::{self, Error as SerdeError},
    Deserialize, Serialize,
};
use serde_bytes::ByteBuf as SerdeByteBuf;

use signature::{Signature, Signer, Verifier};

use crate::traits::{
    Authenticator, EncodeDecodeBase64, KeyPair, SigningKey, ToFromBytes, VerifyingKey,
};

pub const BLS_PRIVATE_KEY_LENGTH: usize = 32;
pub const BLS_PUBLIC_KEY_LENGTH: usize = 96;
pub const BLS_SIGNATURE_LENGTH: usize = 48;
pub const DST: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";

#[readonly::make]
#[derive(Default, Debug, Clone)]
pub struct BLS12381PublicKey {
    pub pubkey: blst::PublicKey,
    pub bytes: OnceCell<[u8; BLS_PUBLIC_KEY_LENGTH]>,
}

#[readonly::make]
#[derive(Default, Debug)]
pub struct BLS12381PrivateKey {
    pub privkey: blst::SecretKey,
    pub bytes: OnceCell<[u8; BLS_PRIVATE_KEY_LENGTH]>,
}

#[readonly::make]
#[derive(Debug, Clone)]
pub struct BLS12381Signature {
    pub sig: blst::Signature,
    pub bytes: OnceCell<[u8; BLS_SIGNATURE_LENGTH]>,
}

impl AsRef<[u8]> for BLS12381PublicKey {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| Ok(self.pubkey.to_bytes()))
            .expect("OnceCell invariant violated")
    }
}

impl ToFromBytes for BLS12381PublicKey {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let pubkey = blst::PublicKey::from_bytes(bytes).map_err(|_| signature::Error::new())?;
        Ok(BLS12381PublicKey {
            pubkey,
            bytes: OnceCell::new(),
        })
    }
}

impl std::hash::Hash for BLS12381PublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl PartialEq for BLS12381PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.pubkey == other.pubkey
    }
}

impl Eq for BLS12381PublicKey {}

impl PartialOrd for BLS12381PublicKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.as_ref().partial_cmp(other.as_ref())
    }
}
impl Ord for BLS12381PublicKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
impl Serialize for BLS12381PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.encode_base64())
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
impl<'de> Deserialize<'de> for BLS12381PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s = <String as serde::Deserialize>::deserialize(deserializer)?;
        let value = Self::decode_base64(&s).map_err(|e| de::Error::custom(e.to_string()))?;
        Ok(value)
    }
}

impl Verifier<BLS12381Signature> for BLS12381PublicKey {
    fn verify(&self, msg: &[u8], signature: &BLS12381Signature) -> Result<(), signature::Error> {
        let err = signature
            .sig
            .verify(true, msg, DST, &[], &self.pubkey, true);
        if err == BLST_ERROR::BLST_SUCCESS {
            Ok(())
        } else {
            Err(signature::Error::new())
        }
    }
}

impl VerifyingKey for BLS12381PublicKey {
    type PrivKey = BLS12381PrivateKey;

    type Sig = BLS12381Signature;

    fn verify_batch(msg: &[u8], pks: &[Self], sigs: &[Self::Sig]) -> Result<(), signature::Error> {
        let num_sigs = sigs.len();
        if pks.len() != num_sigs {
            return Err(signature::Error::new());
        }
        let mut rands: Vec<blst_scalar> = Vec::with_capacity(num_sigs);
        let mut rng = OsRng;

        for _i in 0..num_sigs {
            let mut vals = [0u64; 4];
            vals[0] = rng.next_u64();
            while vals[0] == 0 {
                // Reject zero as it is used for multiplication.
                vals[0] = rng.next_u64();
            }
            let mut rand_i = MaybeUninit::<blst_scalar>::uninit();
            unsafe {
                blst_scalar_from_uint64(rand_i.as_mut_ptr(), vals.as_ptr());
                rands.push(rand_i.assume_init());
            }
        }

        // TODO: fix this, the identical message opens up a rogue key attack
        let msgs_refs = (0..num_sigs).map(|_| msg).collect::<Vec<_>>();

        let result = blst::Signature::verify_multiple_aggregate_signatures(
            &msgs_refs[..],
            DST,
            &pks.iter().map(|pk| &pk.pubkey).collect::<Vec<_>>()[..],
            false,
            &sigs.iter().map(|sig| &sig.sig).collect::<Vec<_>>()[..],
            true,
            &rands,
            64,
        );
        if result == BLST_ERROR::BLST_SUCCESS {
            Ok(())
        } else {
            Err(signature::Error::new())
        }
    }
}

impl AsRef<[u8]> for BLS12381Signature {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| Ok(self.sig.to_bytes()))
            .expect("OnceCell invariant violated")
    }
}

impl std::hash::Hash for BLS12381Signature {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl PartialEq for BLS12381Signature {
    fn eq(&self, other: &Self) -> bool {
        self.sig == other.sig
    }
}

impl Eq for BLS12381Signature {}

impl Signature for BLS12381Signature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let sig = blst::Signature::from_bytes(bytes).map_err(|_e| signature::Error::new())?;
        Ok(BLS12381Signature {
            sig,
            bytes: OnceCell::new(),
        })
    }
}

impl Serialize for BLS12381Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_ref().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BLS12381Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = <SerdeByteBuf>::deserialize(deserializer)?;
        Self::from_bytes(&bytes).map_err(SerdeError::custom)
    }
}
impl Default for BLS12381Signature {
    fn default() -> Self {
        // TODO: improve this!
        let ikm: [u8; 32] = [
            0x93, 0xad, 0x7e, 0x65, 0xde, 0xad, 0x05, 0x2a, 0x08, 0x3a, 0x91, 0x0c, 0x8b, 0x72,
            0x85, 0x91, 0x46, 0x4c, 0xca, 0x56, 0x60, 0x5b, 0xb0, 0x56, 0xed, 0xfe, 0x2b, 0x60,
            0xa6, 0x3c, 0x48, 0x99,
        ];

        let sk = blst::SecretKey::key_gen(&ikm, &[]).unwrap();

        let msg = b"hello foo";
        let sig = sk.sign(msg, DST, &[]);
        BLS12381Signature {
            sig,
            bytes: OnceCell::new(),
        }
    }
}

impl Authenticator for BLS12381Signature {
    type PubKey = BLS12381PublicKey;

    type PrivKey = BLS12381PrivateKey;
}

impl AsRef<[u8]> for BLS12381PrivateKey {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| Ok(self.privkey.to_bytes()))
            .expect("OnceCell invariant violated")
    }
}

impl ToFromBytes for BLS12381PrivateKey {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let privkey = blst::SecretKey::from_bytes(bytes).map_err(|_e| signature::Error::new())?;
        Ok(BLS12381PrivateKey {
            privkey,
            bytes: OnceCell::new(),
        })
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
impl Serialize for BLS12381PrivateKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.encode_base64())
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
impl<'de> Deserialize<'de> for BLS12381PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s = <String as serde::Deserialize>::deserialize(deserializer)?;
        let value = Self::decode_base64(&s).map_err(|e| de::Error::custom(e.to_string()))?;
        Ok(value)
    }
}

impl SigningKey for BLS12381PrivateKey {
    type PubKey = BLS12381PublicKey;

    type Sig = BLS12381Signature;
}

impl Signer<BLS12381Signature> for BLS12381PrivateKey {
    fn try_sign(&self, msg: &[u8]) -> Result<BLS12381Signature, signature::Error> {
        let sig = self.privkey.sign(msg, DST, &[]);

        Ok(BLS12381Signature {
            sig,
            bytes: OnceCell::new(),
        })
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")] // necessary so as not to deser under a != type
pub struct BLS12381KeyPair {
    name: BLS12381PublicKey,
    secret: BLS12381PrivateKey,
}

impl KeyPair for BLS12381KeyPair {
    type PubKey = BLS12381PublicKey;

    type PrivKey = BLS12381PrivateKey;

    fn public(&'_ self) -> &'_ Self::PubKey {
        &self.name
    }

    fn private(self) -> Self::PrivKey {
        self.secret
    }

    fn generate<R: rand::CryptoRng + rand::RngCore>(rng: &mut R) -> Self {
        let mut ikm = [0u8; 32];
        rng.fill_bytes(&mut ikm);
        let privkey = blst::SecretKey::key_gen(&ikm, &[]).expect("ikm length should be higher");
        let pubkey = privkey.sk_to_pk();
        BLS12381KeyPair {
            name: BLS12381PublicKey {
                pubkey,
                bytes: OnceCell::new(),
            },
            secret: BLS12381PrivateKey {
                privkey,
                bytes: OnceCell::new(),
            },
        }
    }
}

impl Signer<BLS12381Signature> for BLS12381KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<BLS12381Signature, signature::Error> {
        let blst_priv: &blst::SecretKey = &self.secret.privkey;
        let sig = blst_priv.sign(msg, DST, &[]);

        Ok(BLS12381Signature {
            sig,
            bytes: OnceCell::new(),
        })
    }
}
