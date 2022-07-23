// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::{self, Display},
    mem::MaybeUninit,
    str::FromStr,
};

use ::blst::{blst_scalar, blst_scalar_from_uint64, BLST_ERROR};
use base64ct::{Base64, Encoding};
use blst::min_sig as blst;

use once_cell::sync::OnceCell;
use rand::{rngs::OsRng, RngCore};

use crate::{
    pubkey_bytes::PublicKeyBytes,
    serde_helpers::{keypair_decode_base64, BlsSignature},
};
use serde::{
    de::{self},
    Deserialize, Serialize,
};
use serde_with::serde_as;

use signature::{Signature, Signer, Verifier};

use crate::traits::{
    AggregateAuthenticator, Authenticator, EncodeDecodeBase64, KeyPair, SigningKey, ToFromBytes,
    VerifyingKey,
};

pub const BLS_PRIVATE_KEY_LENGTH: usize = 32;
pub const BLS_PUBLIC_KEY_LENGTH: usize = 96;
pub const BLS_SIGNATURE_LENGTH: usize = 48;
pub const DST: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";

///
/// Define Structs
///

#[readonly::make]
#[derive(Default, Debug, Clone)]
pub struct BLS12381PublicKey {
    pub pubkey: blst::PublicKey,
    pub bytes: OnceCell<[u8; BLS_PUBLIC_KEY_LENGTH]>,
}

pub type BLS12381PublicKeyBytes = PublicKeyBytes<BLS12381PublicKey, { BLS12381PublicKey::LENGTH }>;

#[readonly::make]
#[derive(Default, Debug)]
pub struct BLS12381PrivateKey {
    pub privkey: blst::SecretKey,
    pub bytes: OnceCell<[u8; BLS_PRIVATE_KEY_LENGTH]>,
}

// There is a strong requirement for this specific impl. in Fab benchmarks
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")] // necessary so as not to deser under a != type
pub struct BLS12381KeyPair {
    name: BLS12381PublicKey,
    secret: BLS12381PrivateKey,
}

#[readonly::make]
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BLS12381Signature {
    #[serde_as(as = "BlsSignature")]
    pub sig: blst::Signature,
    #[serde(skip)]
    pub bytes: OnceCell<[u8; BLS_SIGNATURE_LENGTH]>,
}

#[readonly::make]
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BLS12381AggregateSignature {
    #[serde_as(as = "Option<BlsSignature>")]
    pub sig: Option<blst::Signature>,
    #[serde(skip)]
    pub bytes: OnceCell<[u8; BLS_SIGNATURE_LENGTH]>,
}

///
/// Implement SigningKey
///

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

impl Display for BLS12381PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", Base64::encode_string(self.as_ref()))
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

impl<'a> From<&'a BLS12381PrivateKey> for BLS12381PublicKey {
    fn from(secret: &'a BLS12381PrivateKey) -> Self {
        let inner = &secret.privkey;
        let pubkey = inner.sk_to_pk();
        BLS12381PublicKey {
            pubkey,
            bytes: OnceCell::new(),
        }
    }
}

impl VerifyingKey for BLS12381PublicKey {
    type PrivKey = BLS12381PrivateKey;
    type Sig = BLS12381Signature;

    const LENGTH: usize = BLS_PUBLIC_KEY_LENGTH;

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

///
/// Implement Authenticator
///

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

impl Display for BLS12381Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", Base64::encode_string(self.as_ref()))
    }
}

impl Authenticator for BLS12381Signature {
    type PubKey = BLS12381PublicKey;
    type PrivKey = BLS12381PrivateKey;
    const LENGTH: usize = BLS_SIGNATURE_LENGTH;
}

///
/// Implement SigningKey
///

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
    const LENGTH: usize = BLS_PRIVATE_KEY_LENGTH;
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

///
/// Implement KeyPair
///

impl From<BLS12381PrivateKey> for BLS12381KeyPair {
    fn from(secret: BLS12381PrivateKey) -> Self {
        let name = BLS12381PublicKey::from(&secret);
        BLS12381KeyPair { name, secret }
    }
}

impl EncodeDecodeBase64 for BLS12381KeyPair {
    fn decode_base64(value: &str) -> Result<Self, eyre::Report> {
        keypair_decode_base64(value)
    }

    fn encode_base64(&self) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(self.secret.as_ref());
        bytes.extend_from_slice(self.name.as_ref());
        base64ct::Base64::encode_string(&bytes[..])
    }
}

impl KeyPair for BLS12381KeyPair {
    type PubKey = BLS12381PublicKey;
    type PrivKey = BLS12381PrivateKey;
    type Sig = BLS12381Signature;

    #[cfg(feature = "copy_key")]
    fn copy(&self) -> Self {
        BLS12381KeyPair {
            name: self.name.clone(),
            secret: BLS12381PrivateKey::from_bytes(self.secret.as_ref()).unwrap(),
        }
    }

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

impl FromStr for BLS12381KeyPair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let kp = Self::decode_base64(s).map_err(|e| anyhow::anyhow!("{}", e.to_string()))?;
        Ok(kp)
    }
}

///
/// Implement AggregateAuthenticator
///

// Don't try to use this externally
impl AsRef<[u8]> for BLS12381AggregateSignature {
    fn as_ref(&self) -> &[u8] {
        match self.sig {
            Some(sig) => self
                .bytes
                .get_or_try_init::<_, eyre::Report>(|| Ok(sig.to_bytes()))
                .expect("OnceCell invariant violated"),
            None => &[],
        }
    }
}

impl Display for BLS12381AggregateSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", Base64::encode_string(self.as_ref()))
    }
}

// see [#34](https://github.com/MystenLabs/narwhal/issues/34)
impl Default for BLS12381AggregateSignature {
    fn default() -> Self {
        BLS12381AggregateSignature {
            sig: None,
            bytes: OnceCell::new(),
        }
    }
}

impl AggregateAuthenticator for BLS12381AggregateSignature {
    type PrivKey = BLS12381PrivateKey;
    type PubKey = BLS12381PublicKey;
    type Sig = BLS12381Signature;

    /// Parse a key from its byte representation
    fn aggregate(signatures: Vec<Self::Sig>) -> Result<Self, signature::Error> {
        blst::AggregateSignature::aggregate(
            &signatures.iter().map(|x| &x.sig).collect::<Vec<_>>()[..],
            true,
        )
        .map(|sig| BLS12381AggregateSignature {
            sig: Some(sig.to_signature()),
            bytes: OnceCell::new(),
        })
        .map_err(|_| signature::Error::new())
    }

    fn add_signature(&mut self, signature: Self::Sig) -> Result<(), signature::Error> {
        match self.sig {
            Some(ref mut sig) => {
                let mut aggr_sig = blst::AggregateSignature::from_signature(sig);
                aggr_sig
                    .add_signature(&signature.sig, true)
                    .map_err(|_| signature::Error::new())?;
                self.sig = Some(aggr_sig.to_signature());
                Ok(())
            }
            None => {
                self.sig = Some(signature.sig);
                Ok(())
            }
        }
    }

    fn add_aggregate(&mut self, signature: Self) -> Result<(), signature::Error> {
        match self.sig {
            Some(ref mut sig) => match signature.sig {
                Some(to_add) => {
                    let result = blst::AggregateSignature::aggregate(&[sig, &to_add], true)
                        .map_err(|_| signature::Error::new())?
                        .to_signature();
                    self.sig = Some(result);
                    Ok(())
                }
                None => Ok(()),
            },
            None => {
                self.sig = signature.sig;
                Ok(())
            }
        }
    }

    fn verify(
        &self,
        pks: &[<Self::Sig as Authenticator>::PubKey],
        message: &[u8],
    ) -> Result<(), signature::Error> {
        let result = self
            .sig
            .ok_or_else(signature::Error::new)?
            .fast_aggregate_verify(
                true,
                message,
                DST,
                &pks.iter().map(|x| &x.pubkey).collect::<Vec<_>>()[..],
            );
        if result != BLST_ERROR::BLST_SUCCESS {
            return Err(signature::Error::new());
        }
        Ok(())
    }

    fn batch_verify(
        signatures: &[Self],
        pks: &[&[Self::PubKey]],
        messages: &[&[u8]],
    ) -> Result<(), signature::Error> {
        if signatures.len() != pks.len() || signatures.len() != messages.len() {
            return Err(signature::Error::new());
        }
        for i in 0..signatures.len() {
            let sig = signatures[i].sig;
            let result = sig
                .ok_or_else(signature::Error::new)?
                .fast_aggregate_verify(
                    true,
                    messages[i],
                    DST,
                    &pks[i].iter().map(|x| &x.pubkey).collect::<Vec<_>>()[..],
                );
            if result != BLST_ERROR::BLST_SUCCESS {
                return Err(signature::Error::new());
            }
        }
        Ok(())
    }
}

///
/// Implement VerifyingKeyBytes
///

impl TryFrom<BLS12381PublicKeyBytes> for BLS12381PublicKey {
    type Error = signature::Error;

    fn try_from(bytes: BLS12381PublicKeyBytes) -> Result<BLS12381PublicKey, Self::Error> {
        BLS12381PublicKey::from_bytes(bytes.as_ref()).map_err(|_| Self::Error::new())
    }
}

impl From<&BLS12381PublicKey> for BLS12381PublicKeyBytes {
    fn from(pk: &BLS12381PublicKey) -> BLS12381PublicKeyBytes {
        BLS12381PublicKeyBytes::from_bytes(pk.as_ref()).unwrap()
    }
}
