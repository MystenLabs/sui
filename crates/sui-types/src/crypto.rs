// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::base_types::{AuthorityName, SuiAddress};
use crate::committee::{Committee, EpochId};
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

use blst::min_sig::{SecretKey as BLSSecretKey, PublicKey as BLSPublicKey, AggregateSignature};
use blst::{min_sig as bls, BLST_ERROR};
use ::blst::{blst_scalar, blst_scalar_from_uint64};
use rand::{RngCore};


// TODO: Make sure secrets are not copyable and movable to control where they are in memory
#[derive(Debug)]
pub struct KeyPair {
    pub public_key: BLSPublicKey,
    pub secret_key: BLSSecretKey,
    pub public_key_cell: OnceCell<PublicKeyBytes>,
}

pub const BLST_SK_SIZE: usize = 32;
pub const BLST_PK_SIZE: usize = 96;
pub const BLST_SIG_SIZE: usize = 48;

impl KeyPair {
    pub fn public_key_bytes(&self) -> &PublicKeyBytes {
        self.public_key_cell.get_or_init(|| {
            let pk_arr: [u8; BLST_PK_SIZE] = self.public_key.to_bytes();
            PublicKeyBytes(pk_arr)
        })
    }

    // TODO: eradicate unneeded uses (i.e. most)
    /// Avoid implementing `clone` on secret keys to prevent mistakes.
    #[must_use]
    pub fn copy(&self) -> KeyPair {
        KeyPair {
            public_key: BLSPublicKey::from_bytes(&self.public_key.to_bytes()).unwrap(),
            secret_key: BLSSecretKey::from_bytes(&self.secret_key.to_bytes()).unwrap(),
            public_key_cell: OnceCell::new(),
        }
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

// impl From<DalekKeypair> for KeyPair {
//     fn from(dalek_keypair: DalekKeypair) -> Self {
//         Self {
//             key_pair: dalek_keypair,
//             public_key_cell: OnceCell::new(),
//         }
//     }
// }

impl Serialize for KeyPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let key_concat: Vec<u8> = self.public_key.to_bytes()
            .iter().cloned()
            .chain(self.secret_key.to_bytes().iter().cloned())
            .collect();
        serializer.serialize_str(&base64ct::Base64::encode_string(&key_concat[..]))
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
        let public_key = BLSPublicKey::from_bytes(&value[..BLST_PK_SIZE])
            .map_err(|err| serde::de::Error::custom("BLST FAILURE"))?;
        let secret_key = BLSSecretKey::from_bytes(&value[BLST_PK_SIZE..])
            .map_err(|err| serde::de::Error::custom("BLST FAILURE"))?;
        Ok(KeyPair {
            public_key: public_key,
            secret_key: secret_key,
            public_key_cell: OnceCell::new(),
        })
    }
}

impl FromStr for KeyPair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = base64ct::Base64::decode_vec(s).map_err(|e| anyhow!("{}", e.to_string()))?;

        Ok(KeyPair {
            secret_key: BLSSecretKey::from_bytes(&value[..BLST_SK_SIZE]).map_err(|e| anyhow!("{}", "temp"))?,
            public_key: BLSPublicKey::from_bytes(&value[BLST_SK_SIZE..]).map_err(|e| anyhow!("{}", "temp"))?,
            public_key_cell: OnceCell::new(),
        })
    }
}

pub const DST: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";

impl signature::Signer<Signature> for KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        let signature_bytes = &self.secret_key.sign(msg, DST, &[]).to_bytes();
        let public_key_bytes = self.public_key_bytes().as_ref();
        let mut result_bytes = [0u8; SUI_SIGNATURE_LENGTH];
        result_bytes[..BLST_SIG_SIZE].copy_from_slice(signature_bytes);
        result_bytes[BLST_SIG_SIZE..].copy_from_slice(public_key_bytes);
        Ok(Signature(result_bytes))
    }
}

impl signature::Signer<AuthoritySignature> for KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<AuthoritySignature, signature::Error> {
        let sig = self.secret_key.sign(msg, DST, &[]);
        Ok(AuthoritySignature(sig))
    }
}

#[serde_as]
#[derive(
    Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct PublicKeyBytes(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; BLST_PK_SIZE],
);

impl Default for PublicKeyBytes {
    fn default() -> Self {
        PublicKeyBytes([0; BLST_PK_SIZE])
    }
}

impl PublicKeyBytes {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
    // /// Make a Narwhal-compatible public key from a Sui pub.
    pub fn make_narwhal_public_key(&self) -> Result<Ed25519PublicKey, signature::Error> {
        let kp = DalekKeypair::generate(&mut OsRng);
        Ok(Ed25519PublicKey(kp.public))
    }
}

impl AsRef<[u8]> for PublicKeyBytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl TryInto<BLSPublicKey> for PublicKeyBytes {
    type Error = SuiError;

    fn try_into(self) -> Result<BLSPublicKey, Self::Error> {
        // TODO(https://github.com/MystenLabs/sui/issues/101): Do better key validation
        // to ensure the bytes represent a point on the curve.
        BLSPublicKey::from_bytes(self.as_ref()).map_err(|_| SuiError::InvalidAuthenticator)
    }
}

// TODO(https://github.com/MystenLabs/sui/issues/101): more robust key validation
impl TryFrom<&[u8]> for PublicKeyBytes {
    type Error = SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, SuiError> {
        let arr: [u8; BLST_PK_SIZE] = bytes
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

// 
// Keypair Generation
// 

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
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);

    let sk = BLSSecretKey::key_gen(&seed, &[]).unwrap();
    let pk = sk.sk_to_pk();

    let keypair = KeyPair {
        public_key: pk,
        secret_key: sk,
        public_key_cell: OnceCell::new(),
    };
    (SuiAddress::from(keypair.public_key_bytes()), keypair)
}

// TODO: C-GETTER
pub fn get_key_pair_from_bytes(bytes: &[u8]) -> (SuiAddress, KeyPair) {
    let keypair = KeyPair {
        public_key: BLSPublicKey::from_bytes(&bytes[..BLST_PK_SIZE]).unwrap(),
        secret_key: BLSSecretKey::from_bytes(&bytes[BLST_PK_SIZE..]).unwrap(), 
        public_key_cell: OnceCell::new(),
    };
    (SuiAddress::from(keypair.public_key_bytes()), keypair)
}

// 
// Signatures
// 

// TODO: replace this with a byte interpretation based on multicodec
pub const SUI_SIGNATURE_LENGTH: usize =
    BLST_PK_SIZE + BLST_SIG_SIZE;

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
            &BLSPublicKey::from_bytes(public_key_bytes.as_ref()).map_err(|err| {
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
    ) -> Result<(Vec<u8>, bls::Signature, PublicKeyBytes), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // Is this signature emitted by the expected author?
        let public_key_bytes: [u8; BLST_PK_SIZE] = self
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
            bls::Signature::from_bytes(self.signature_bytes()).map_err(|err| {
                SuiError::InvalidSignature {
                    error: "Error".to_string(),
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
    #[schemars(with = "Base64")]
    #[serde_as(as = "BlsSignature")]
    pub bls::Signature,
);

impl AsRef<[u8]> for AuthoritySignature {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            slice::from_raw_parts(&self.0 as *const _ as  *const u8, BLST_SIG_SIZE)
        }
    }
}

impl signature::Signature for AuthoritySignature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let sig = bls::Signature::from_bytes(bytes)
            .map_err(|x| signature::Error::new()).unwrap();
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
    pub fn verify<T>(&self, value: &T, author: PublicKeyBytes) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
    {
        // is this a cryptographically valid public Key?
        let public_key: BLSPublicKey = author.try_into()?;

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
        key_cache: &HashMap<PublicKeyBytes, BLSPublicKey>,
    ) -> Result<(), SuiError>
    where
        T: Signable<Vec<u8>>,
        I: IntoIterator<Item = K>,
        K: Borrow<(PublicKeyBytes, AuthoritySignature)>,
    {
        let mut msg = Vec::new();
        value.write(&mut msg);
        let mut messages: Vec<&[u8]> = Vec::new();
        let mut signatures: Vec<bls::Signature> = Vec::new();
        let mut public_keys: Vec<BLSPublicKey> = Vec::new();
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

        let aggregated_signature = AggregateSignature::aggregate(&signatures.iter().map(|x| x).collect::<Vec<_>>()[..], true)
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
        obligation: &mut VerificationObligation,
        message_index: usize,
    ) -> SuiResult<()> {
        obligation
            .public_keys
            .push(committee.public_key(&self.authority)?);
        obligation.aggregated_signature = match obligation.aggregated_signature {
            Some(signature) => {
                let mut aggr_sig = AggregateSignature::from_signature(&signature);
                aggr_sig.add_signature(&self.signature.0, true)
                .map_err(|err| SuiError::InvalidSignature { error: format!("{:?}", err) })?;
                Some(aggr_sig.to_signature())
            },
            None => Some(self.signature.0)
        };
        obligation.message_index.push(message_index);
        Ok(())
    }
}

/// Represents at least a quorum (could be more) of authority signatures.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuthorityQuorumSignInfo {
    pub epoch: EpochId,
    pub authorities: Vec<AuthorityName>,
    pub aggregated_signature: Option<AuthoritySignature>,
}
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
//
static_assertions::assert_not_impl_any!(AuthorityQuorumSignInfo: Hash, Eq, PartialEq);
impl AuthoritySignInfoTrait for AuthorityQuorumSignInfo {}

impl AuthorityQuorumSignInfo {
    pub fn add_to_verification_obligation(
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
        let mut used_authorities = HashSet::new();

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
                .public_keys
                .push(committee.public_key(authority)?);
            obligation.message_index.push(message_index);
        }

        obligation.aggregated_signature = 
            match obligation.aggregated_signature {
                Some(prev_sig) => {
                    match self.aggregated_signature {
                        Some(prev_aggr_sig) => {
                            let mut aggr_sig = AggregateSignature::from_signature(&prev_sig);
                            aggr_sig.add_signature(&prev_aggr_sig.0, true)
                            .map_err(|err| SuiError::InvalidSignature { error: format!("{:?}", err) })?;
                            Some(aggr_sig.to_signature())
                        }
                        None => Some(prev_sig)
                    }
                }
                None => {
                    match self.aggregated_signature {
                        Some(sig) => Some(sig.0),
                        None => None
                    }
                }
            };


        fp_ensure!(
            weight >= committee.quorum_threshold(),
            SuiError::CertificateRequiresQuorum
        );

        Ok(())
    }
}

mod private {
    pub trait SealedAuthoritySignInfoTrait {}
    impl SealedAuthoritySignInfoTrait for super::EmptySignInfo {}
    impl SealedAuthoritySignInfoTrait for super::AuthoritySignInfo {}
    impl SealedAuthoritySignInfoTrait for super::AuthorityQuorumSignInfo {}
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

pub type PubKeyLookup = HashMap<PublicKeyBytes, BLSPublicKey>;

pub fn sha3_hash<S: Signable<Sha3_256>>(signable: &S) -> [u8; 32] {
    let mut digest = Sha3_256::default();
    signable.write(&mut digest);
    let hash = digest.finalize();
    hash.into()
}

pub struct VerificationObligation {
    lookup: PubKeyLookup,
    messages: Vec<Vec<u8>>,
    pub message_index: Vec<usize>,
    pub aggregated_signature: Option<bls::Signature>,
    pub public_keys: Vec<BLSPublicKey>,
}

pub fn aggregate_authority_signatures(
    authority_signatures: &[&AuthoritySignature]
) -> Result<AuthoritySignature, SuiError> {
    Ok(AuthoritySignature(aggregate_bls_signature(
        &authority_signatures.iter().map(|s| s.0).collect::<Vec<_>>()[..]
    )?))
}

pub fn aggregate_bls_signature(signatures: &[bls::Signature]) -> Result<bls::Signature, SuiError> {
    let aggregated_signature = AggregateSignature::aggregate(
        &signatures.iter().map(|sig| sig).collect::<Vec<_>>()[..],
        true
    ).map_err(|e| SuiError::InvalidSignature {
        error: format!("{:?}", e)
    })?;
    Ok(aggregated_signature.to_signature())
}

impl Default for VerificationObligation {
    fn default() -> Self {
        VerificationObligation {
            lookup: PubKeyLookup::default(),
            messages: Vec::default(),
            message_index: Vec::default(),
            aggregated_signature: None,
            public_keys: Vec::default()
        }
    }
}

impl VerificationObligation {
    pub fn new(lookup: PubKeyLookup) -> VerificationObligation {
        VerificationObligation {
            lookup,
            ..Default::default()
        }
    }

    pub fn lookup_public_key(
        &mut self,
        key_bytes: &PublicKeyBytes,
    ) -> Result<BLSPublicKey, SuiError> {
        match self.lookup.get(key_bytes) {
            Some(v) => Ok(*v),
            None => {
                let public_key = (*key_bytes).try_into()?;
                self.lookup.insert(*key_bytes, public_key);
                Ok(public_key)
            }
        }
    }

    /// Add a new message to the list of messages to be verified.
    /// Returns the index of the message.
    pub fn add_message(&mut self, message: Vec<u8>) -> usize {
        let idx = self.messages.len();
        self.messages.push(message);
        idx
    }

    pub fn verify_all(self) -> SuiResult<PubKeyLookup> {
        let messages_inner: Vec<_> = self
            .message_index
            .iter()
            .map(|idx| &self.messages[*idx][..])
            .collect();

        if (messages_inner.len() == 0 && self.aggregated_signature == None) {
           return Ok(self.lookup);
        }

        let result = match self.aggregated_signature {
            Some(signature) => {
                signature.aggregate_verify(
                    true,
                    &messages_inner[..],
                    DST,
                    &self.public_keys.iter().map(|pk| pk).collect::<Vec<_>>()[..],
                    true
                )
            }
            None => {
                println!("{:?}", self.public_keys.len());
                return Err(SuiError::InvalidSignature {
                    error: format!("Empty Signature")
                })
            }
        };

        if result == BLST_ERROR::BLST_SUCCESS {
            Ok(self.lookup)
        } else {
            Err(SuiError::InvalidSignature {
                error: format!("ERROR {:?} {:?}", result, self.public_keys.len()),
            })
        }
    }
}
