// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::base_types::{AuthorityName, ConciseableName, SuiAddress};
use crate::committee::CommitteeTrait;
use crate::committee::{Committee, EpochId, StakeUnit};
use crate::error::{SuiError, SuiResult};
use crate::signature::GenericSignature;
use crate::sui_serde::{Readable, SuiBitmap};
use anyhow::{anyhow, Error};
use derive_more::{AsMut, AsRef, From};
pub use enum_dispatch::enum_dispatch;
use eyre::eyre;
use fastcrypto::bls12381::min_sig::{
    BLS12381AggregateSignature, BLS12381AggregateSignatureAsBytes, BLS12381KeyPair,
    BLS12381PrivateKey, BLS12381PublicKey, BLS12381Signature,
};
use fastcrypto::ed25519::{
    Ed25519KeyPair, Ed25519PrivateKey, Ed25519PublicKey, Ed25519PublicKeyAsBytes, Ed25519Signature,
    Ed25519SignatureAsBytes,
};
use fastcrypto::encoding::{Base64, Bech32, Encoding, Hex};
use fastcrypto::error::{FastCryptoError, FastCryptoResult};
use fastcrypto::hash::{Blake2b256, HashFunction};
use fastcrypto::secp256k1::{
    Secp256k1KeyPair, Secp256k1PublicKey, Secp256k1PublicKeyAsBytes, Secp256k1Signature,
    Secp256k1SignatureAsBytes,
};
use fastcrypto::secp256r1::{
    Secp256r1KeyPair, Secp256r1PublicKey, Secp256r1PublicKeyAsBytes, Secp256r1Signature,
    Secp256r1SignatureAsBytes,
};
pub use fastcrypto::traits::KeyPair as KeypairTraits;
pub use fastcrypto::traits::Signer;
pub use fastcrypto::traits::{
    AggregateAuthenticator, Authenticator, EncodeDecodeBase64, SigningKey, ToFromBytes,
    VerifyingKey,
};
use fastcrypto_zkp::bn254::zk_login::ZkLoginInputs;
use fastcrypto_zkp::zk_login_utils::Bn254FrElement;
use rand::rngs::{OsRng, StdRng};
use rand::SeedableRng;
use roaring::RoaringBitmap;
use schemars::JsonSchema;
use serde::ser::Serializer;
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::{serde_as, Bytes};
use shared_crypto::intent::{Intent, IntentMessage, IntentScope};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fmt::{self, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use strum::EnumString;
use tracing::{instrument, warn};

#[cfg(test)]
#[path = "unit_tests/crypto_tests.rs"]
mod crypto_tests;

#[cfg(test)]
#[cfg(feature = "test-utils")]
#[path = "unit_tests/intent_tests.rs"]
mod intent_tests;

// Authority Objects
pub type AuthorityKeyPair = BLS12381KeyPair;
pub type AuthorityPublicKey = BLS12381PublicKey;
pub type AuthorityPrivateKey = BLS12381PrivateKey;
pub type AuthoritySignature = BLS12381Signature;
pub type AggregateAuthoritySignature = BLS12381AggregateSignature;
pub type AggregateAuthoritySignatureAsBytes = BLS12381AggregateSignatureAsBytes;

// TODO(joyqvq): prefix these types with Default, DefaultAccountKeyPair etc
pub type AccountKeyPair = Ed25519KeyPair;
pub type AccountPublicKey = Ed25519PublicKey;
pub type AccountPrivateKey = Ed25519PrivateKey;

pub type NetworkKeyPair = Ed25519KeyPair;
pub type NetworkPublicKey = Ed25519PublicKey;
pub type NetworkPrivateKey = Ed25519PrivateKey;

pub type DefaultHash = Blake2b256;

pub const DEFAULT_EPOCH_ID: EpochId = 0;
pub const SUI_PRIV_KEY_PREFIX: &str = "suiprivkey";

/// Creates a proof of that the authority account address is owned by the
/// holder of authority protocol key, and also ensures that the authority
/// protocol public key exists. A proof of possession is an authority
/// signature committed over the intent message `intent || message || epoch` (See
/// more at [struct IntentMessage] and [struct Intent]) where the message is
/// constructed as `authority_pubkey_bytes || authority_account_address`.
pub fn generate_proof_of_possession(
    keypair: &AuthorityKeyPair,
    address: SuiAddress,
) -> AuthoritySignature {
    let mut msg: Vec<u8> = Vec::new();
    msg.extend_from_slice(keypair.public().as_bytes());
    msg.extend_from_slice(address.as_ref());
    AuthoritySignature::new_secure(
        &IntentMessage::new(Intent::sui_app(IntentScope::ProofOfPossession), msg),
        &DEFAULT_EPOCH_ID,
        keypair,
    )
}

/// Verify proof of possession against the expected intent message,
/// consisting of the protocol pubkey and the authority account address.
pub fn verify_proof_of_possession(
    pop: &narwhal_crypto::Signature,
    protocol_pubkey: &narwhal_crypto::PublicKey,
    sui_address: SuiAddress,
) -> Result<(), SuiError> {
    protocol_pubkey
        .validate()
        .map_err(|_| SuiError::InvalidSignature {
            error: "Fail to validate pubkey".to_string(),
        })?;
    let mut msg = protocol_pubkey.as_bytes().to_vec();
    msg.extend_from_slice(sui_address.as_ref());
    pop.verify_secure(
        &IntentMessage::new(Intent::sui_app(IntentScope::ProofOfPossession), msg),
        DEFAULT_EPOCH_ID,
        protocol_pubkey.into(),
    )
}
///////////////////////////////////////////////
/// Account Keys
///
/// * The following section defines the keypairs that are used by
/// * accounts to interact with Sui.
/// * Currently we support eddsa and ecdsa on Sui.
///

#[allow(clippy::large_enum_variant)]
#[derive(Debug, From, PartialEq, Eq)]
pub enum SuiKeyPair {
    Ed25519(Ed25519KeyPair),
    Secp256k1(Secp256k1KeyPair),
    Secp256r1(Secp256r1KeyPair),
}

impl SuiKeyPair {
    pub fn public(&self) -> PublicKey {
        match self {
            SuiKeyPair::Ed25519(kp) => PublicKey::Ed25519(kp.public().into()),
            SuiKeyPair::Secp256k1(kp) => PublicKey::Secp256k1(kp.public().into()),
            SuiKeyPair::Secp256r1(kp) => PublicKey::Secp256r1(kp.public().into()),
        }
    }

    pub fn copy(&self) -> Self {
        match self {
            SuiKeyPair::Ed25519(kp) => kp.copy().into(),
            SuiKeyPair::Secp256k1(kp) => kp.copy().into(),
            SuiKeyPair::Secp256r1(kp) => kp.copy().into(),
        }
    }
}

impl Signer<Signature> for SuiKeyPair {
    fn sign(&self, msg: &[u8]) -> Signature {
        match self {
            SuiKeyPair::Ed25519(kp) => kp.sign(msg),
            SuiKeyPair::Secp256k1(kp) => kp.sign(msg),
            SuiKeyPair::Secp256r1(kp) => kp.sign(msg),
        }
    }
}

impl EncodeDecodeBase64 for SuiKeyPair {
    fn encode_base64(&self) -> String {
        Base64::encode(self.to_bytes())
    }

    fn decode_base64(value: &str) -> FastCryptoResult<Self> {
        let bytes = Base64::decode(value)?;
        Self::from_bytes(&bytes).map_err(|_| FastCryptoError::InvalidInput)
    }
}
impl SuiKeyPair {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.push(self.public().flag());

        match self {
            SuiKeyPair::Ed25519(kp) => {
                bytes.extend_from_slice(kp.as_bytes());
            }
            SuiKeyPair::Secp256k1(kp) => {
                bytes.extend_from_slice(kp.as_bytes());
            }
            SuiKeyPair::Secp256r1(kp) => {
                bytes.extend_from_slice(kp.as_bytes());
            }
        }
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, eyre::Report> {
        match SignatureScheme::from_flag_byte(bytes.first().ok_or_else(|| eyre!("Invalid length"))?)
        {
            Ok(x) => match x {
                SignatureScheme::ED25519 => Ok(SuiKeyPair::Ed25519(Ed25519KeyPair::from_bytes(
                    bytes.get(1..).ok_or_else(|| eyre!("Invalid length"))?,
                )?)),
                SignatureScheme::Secp256k1 => {
                    Ok(SuiKeyPair::Secp256k1(Secp256k1KeyPair::from_bytes(
                        bytes.get(1..).ok_or_else(|| eyre!("Invalid length"))?,
                    )?))
                }
                SignatureScheme::Secp256r1 => {
                    Ok(SuiKeyPair::Secp256r1(Secp256r1KeyPair::from_bytes(
                        bytes.get(1..).ok_or_else(|| eyre!("Invalid length"))?,
                    )?))
                }
                _ => Err(eyre!("Invalid flag byte")),
            },
            _ => Err(eyre!("Invalid bytes")),
        }
    }

    pub fn to_bytes_no_flag(&self) -> Vec<u8> {
        match self {
            SuiKeyPair::Ed25519(kp) => kp.as_bytes().to_vec(),
            SuiKeyPair::Secp256k1(kp) => kp.as_bytes().to_vec(),
            SuiKeyPair::Secp256r1(kp) => kp.as_bytes().to_vec(),
        }
    }

    /// Encode a SuiKeyPair as `flag || privkey` in Bech32 starting with "suiprivkey" to a string. Note that the pubkey is not encoded.
    pub fn encode(&self) -> Result<String, eyre::Report> {
        Bech32::encode(self.to_bytes(), SUI_PRIV_KEY_PREFIX).map_err(|e| eyre!(e))
    }

    /// Decode a SuiKeyPair from `flag || privkey` in Bech32 starting with "suiprivkey" to SuiKeyPair. The public key is computed directly from the private key bytes.
    pub fn decode(value: &str) -> Result<Self, eyre::Report> {
        let bytes = Bech32::decode(value, SUI_PRIV_KEY_PREFIX)?;
        Self::from_bytes(&bytes)
    }
}

impl Serialize for SuiKeyPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = self.encode_base64();
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for SuiKeyPair {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let s = String::deserialize(deserializer)?;
        SuiKeyPair::decode_base64(&s).map_err(|e| Error::custom(e.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, JsonSchema, Serialize, Deserialize)]
pub enum PublicKey {
    Ed25519(Ed25519PublicKeyAsBytes),
    Secp256k1(Secp256k1PublicKeyAsBytes),
    Secp256r1(Secp256r1PublicKeyAsBytes),
    ZkLogin(ZkLoginPublicIdentifier),
    Passkey(Secp256r1PublicKeyAsBytes),
}

/// A wrapper struct to retrofit in [enum PublicKey] for zkLogin.
/// Useful to construct [struct MultiSigPublicKey].
#[derive(Clone, Debug, PartialEq, Eq, JsonSchema, Serialize, Deserialize)]
pub struct ZkLoginPublicIdentifier(#[schemars(with = "Base64")] pub Vec<u8>);

impl ZkLoginPublicIdentifier {
    /// Consists of iss_bytes_len || iss_bytes || padded_32_byte_address_seed.
    pub fn new(iss: &str, address_seed: &Bn254FrElement) -> SuiResult<Self> {
        let mut bytes = Vec::new();
        let iss_bytes = iss.as_bytes();
        bytes.extend([iss_bytes.len() as u8]);
        bytes.extend(iss_bytes);
        bytes.extend(address_seed.padded());

        Ok(Self(bytes))
    }
}
impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        match self {
            PublicKey::Ed25519(pk) => &pk.0,
            PublicKey::Secp256k1(pk) => &pk.0,
            PublicKey::Secp256r1(pk) => &pk.0,
            PublicKey::ZkLogin(z) => &z.0,
            PublicKey::Passkey(pk) => &pk.0,
        }
    }
}

impl EncodeDecodeBase64 for PublicKey {
    fn encode_base64(&self) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(&[self.flag()]);
        bytes.extend_from_slice(self.as_ref());
        Base64::encode(&bytes[..])
    }

    fn decode_base64(value: &str) -> FastCryptoResult<Self> {
        let bytes = Base64::decode(value)?;
        match bytes.first() {
            Some(x) => {
                if x == &SignatureScheme::ED25519.flag() {
                    let pk: Ed25519PublicKey =
                        Ed25519PublicKey::from_bytes(bytes.get(1..).ok_or(
                            FastCryptoError::InputLengthWrong(Ed25519PublicKey::LENGTH + 1),
                        )?)?;
                    Ok(PublicKey::Ed25519((&pk).into()))
                } else if x == &SignatureScheme::Secp256k1.flag() {
                    let pk = Secp256k1PublicKey::from_bytes(bytes.get(1..).ok_or(
                        FastCryptoError::InputLengthWrong(Secp256k1PublicKey::LENGTH + 1),
                    )?)?;
                    Ok(PublicKey::Secp256k1((&pk).into()))
                } else if x == &SignatureScheme::Secp256r1.flag() {
                    let pk = Secp256r1PublicKey::from_bytes(bytes.get(1..).ok_or(
                        FastCryptoError::InputLengthWrong(Secp256r1PublicKey::LENGTH + 1),
                    )?)?;
                    Ok(PublicKey::Secp256r1((&pk).into()))
                } else if x == &SignatureScheme::PasskeyAuthenticator.flag() {
                    let pk = Secp256r1PublicKey::from_bytes(bytes.get(1..).ok_or(
                        FastCryptoError::InputLengthWrong(Secp256r1PublicKey::LENGTH + 1),
                    )?)?;
                    Ok(PublicKey::Passkey((&pk).into()))
                } else {
                    Err(FastCryptoError::InvalidInput)
                }
            }
            _ => Err(FastCryptoError::InvalidInput),
        }
    }
}

impl PublicKey {
    pub fn flag(&self) -> u8 {
        self.scheme().flag()
    }

    pub fn try_from_bytes(
        curve: SignatureScheme,
        key_bytes: &[u8],
    ) -> Result<PublicKey, eyre::Report> {
        match curve {
            SignatureScheme::ED25519 => Ok(PublicKey::Ed25519(
                (&Ed25519PublicKey::from_bytes(key_bytes)?).into(),
            )),
            SignatureScheme::Secp256k1 => Ok(PublicKey::Secp256k1(
                (&Secp256k1PublicKey::from_bytes(key_bytes)?).into(),
            )),
            SignatureScheme::Secp256r1 => Ok(PublicKey::Secp256r1(
                (&Secp256r1PublicKey::from_bytes(key_bytes)?).into(),
            )),
            SignatureScheme::PasskeyAuthenticator => Ok(PublicKey::Passkey(
                (&Secp256r1PublicKey::from_bytes(key_bytes)?).into(),
            )),
            _ => Err(eyre!("Unsupported curve")),
        }
    }

    pub fn scheme(&self) -> SignatureScheme {
        match self {
            PublicKey::Ed25519(_) => Ed25519SuiSignature::SCHEME,
            PublicKey::Secp256k1(_) => Secp256k1SuiSignature::SCHEME,
            PublicKey::Secp256r1(_) => Secp256r1SuiSignature::SCHEME,
            PublicKey::ZkLogin(_) => SignatureScheme::ZkLoginAuthenticator,
            PublicKey::Passkey(_) => SignatureScheme::PasskeyAuthenticator,
        }
    }

    pub fn from_zklogin_inputs(inputs: &ZkLoginInputs) -> SuiResult<Self> {
        Ok(PublicKey::ZkLogin(ZkLoginPublicIdentifier::new(
            inputs.get_iss(),
            inputs.get_address_seed(),
        )?))
    }
}

/// Defines the compressed version of the public key that we pass around
/// in Sui
#[serde_as]
#[derive(
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    AsRef,
)]
#[as_ref(forward)]
pub struct AuthorityPublicKeyBytes(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    pub [u8; AuthorityPublicKey::LENGTH],
);

impl AuthorityPublicKeyBytes {
    fn fmt_impl(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let s = Hex::encode(self.0);
        write!(f, "k#{}", s)?;
        Ok(())
    }
}

impl<'a> ConciseableName<'a> for AuthorityPublicKeyBytes {
    type ConciseTypeRef = ConciseAuthorityPublicKeyBytesRef<'a>;
    type ConciseType = ConciseAuthorityPublicKeyBytes;

    /// Get a ConciseAuthorityPublicKeyBytesRef. Usage:
    ///
    ///   debug!(name = ?authority.concise());
    ///   format!("{:?}", authority.concise());
    fn concise(&'a self) -> ConciseAuthorityPublicKeyBytesRef<'a> {
        ConciseAuthorityPublicKeyBytesRef(self)
    }

    fn concise_owned(&self) -> ConciseAuthorityPublicKeyBytes {
        ConciseAuthorityPublicKeyBytes(*self)
    }
}

/// A wrapper around AuthorityPublicKeyBytes that provides a concise Debug impl.
pub struct ConciseAuthorityPublicKeyBytesRef<'a>(&'a AuthorityPublicKeyBytes);

impl Debug for ConciseAuthorityPublicKeyBytesRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let s = Hex::encode(self.0 .0.get(0..4).ok_or(std::fmt::Error)?);
        write!(f, "k#{}..", s)
    }
}

impl Display for ConciseAuthorityPublicKeyBytesRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        Debug::fmt(self, f)
    }
}

/// A wrapper around AuthorityPublicKeyBytes but owns it.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConciseAuthorityPublicKeyBytes(AuthorityPublicKeyBytes);

impl Debug for ConciseAuthorityPublicKeyBytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let s = Hex::encode(self.0 .0.get(0..4).ok_or(std::fmt::Error)?);
        write!(f, "k#{}..", s)
    }
}

impl Display for ConciseAuthorityPublicKeyBytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        Debug::fmt(self, f)
    }
}

impl TryFrom<AuthorityPublicKeyBytes> for AuthorityPublicKey {
    type Error = FastCryptoError;

    fn try_from(bytes: AuthorityPublicKeyBytes) -> Result<AuthorityPublicKey, Self::Error> {
        AuthorityPublicKey::from_bytes(bytes.as_ref())
    }
}

impl From<&AuthorityPublicKey> for AuthorityPublicKeyBytes {
    fn from(pk: &AuthorityPublicKey) -> AuthorityPublicKeyBytes {
        AuthorityPublicKeyBytes::from_bytes(pk.as_ref()).unwrap()
    }
}

impl Debug for AuthorityPublicKeyBytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.fmt_impl(f)
    }
}

impl Display for AuthorityPublicKeyBytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.fmt_impl(f)
    }
}

impl ToFromBytes for AuthorityPublicKeyBytes {
    fn from_bytes(bytes: &[u8]) -> Result<Self, fastcrypto::error::FastCryptoError> {
        let bytes: [u8; AuthorityPublicKey::LENGTH] = bytes
            .try_into()
            .map_err(|_| fastcrypto::error::FastCryptoError::InvalidInput)?;
        Ok(AuthorityPublicKeyBytes(bytes))
    }
}

impl AuthorityPublicKeyBytes {
    pub const ZERO: Self = Self::new([0u8; AuthorityPublicKey::LENGTH]);

    /// This ensures it's impossible to construct an instance with other than registered lengths
    pub const fn new(bytes: [u8; AuthorityPublicKey::LENGTH]) -> AuthorityPublicKeyBytes
where {
        AuthorityPublicKeyBytes(bytes)
    }
}

impl FromStr for AuthorityPublicKeyBytes {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = Hex::decode(s).map_err(|e| anyhow!(e))?;
        Self::from_bytes(&value[..]).map_err(|e| anyhow!(e))
    }
}

impl Default for AuthorityPublicKeyBytes {
    fn default() -> Self {
        Self::ZERO
    }
}

//
// Add helper calls for Authority Signature
//

pub trait SuiAuthoritySignature {
    fn verify_secure<T>(
        &self,
        value: &IntentMessage<T>,
        epoch_id: EpochId,
        author: AuthorityPublicKeyBytes,
    ) -> Result<(), SuiError>
    where
        T: Serialize;

    fn new_secure<T>(
        value: &IntentMessage<T>,
        epoch_id: &EpochId,
        secret: &dyn Signer<Self>,
    ) -> Self
    where
        T: Serialize;
}

impl SuiAuthoritySignature for AuthoritySignature {
    #[instrument(level = "trace", skip_all)]
    fn new_secure<T>(value: &IntentMessage<T>, epoch: &EpochId, secret: &dyn Signer<Self>) -> Self
    where
        T: Serialize,
    {
        let mut intent_msg_bytes =
            bcs::to_bytes(&value).expect("Message serialization should not fail");
        epoch.write(&mut intent_msg_bytes);
        secret.sign(&intent_msg_bytes)
    }

    #[instrument(level = "trace", skip_all)]
    fn verify_secure<T>(
        &self,
        value: &IntentMessage<T>,
        epoch: EpochId,
        author: AuthorityPublicKeyBytes,
    ) -> Result<(), SuiError>
    where
        T: Serialize,
    {
        let mut message = bcs::to_bytes(&value).expect("Message serialization should not fail");
        epoch.write(&mut message);

        let public_key = AuthorityPublicKey::try_from(author).map_err(|_| {
            SuiError::KeyConversionError(
                "Failed to serialize public key bytes to valid public key".to_string(),
            )
        })?;
        public_key
            .verify(&message[..], self)
            .map_err(|e| SuiError::InvalidSignature {
                error: format!(
                    "Fail to verify auth sig {} epoch: {} author: {}",
                    e,
                    epoch,
                    author.concise()
                ),
            })
    }
}

// TODO: get_key_pair() and get_key_pair_from_bytes() should return KeyPair only.
// TODO: rename to random_key_pair
pub fn get_key_pair<KP: KeypairTraits>() -> (SuiAddress, KP)
where
    <KP as KeypairTraits>::PubKey: SuiPublicKey,
{
    get_key_pair_from_rng(&mut OsRng)
}

/// Generate a random committee key pairs with a given committee size
pub fn random_committee_key_pairs_of_size(size: usize) -> Vec<AuthorityKeyPair> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..size)
        .map(|_| {
            // TODO: We are generating the keys 4 times to match exactly as how we generate
            // keys in ConfigBuilder::build (sui-config/src/network_config_builder). This is because
            // we are using these key generation functions as fixtures and we call them
            // independently in different paths and exact the results to be the same.
            // We should eliminate them.
            let key_pair = get_key_pair_from_rng::<AuthorityKeyPair, _>(&mut rng);
            get_key_pair_from_rng::<AuthorityKeyPair, _>(&mut rng);
            get_key_pair_from_rng::<AccountKeyPair, _>(&mut rng);
            get_key_pair_from_rng::<AccountKeyPair, _>(&mut rng);
            key_pair.1
        })
        .collect()
}

pub fn deterministic_random_account_key() -> (SuiAddress, AccountKeyPair) {
    let mut rng = StdRng::from_seed([0; 32]);
    get_key_pair_from_rng(&mut rng)
}

pub fn get_account_key_pair() -> (SuiAddress, AccountKeyPair) {
    get_key_pair()
}

pub fn get_authority_key_pair() -> (SuiAddress, AuthorityKeyPair) {
    get_key_pair()
}

/// Generate a keypair from the specified RNG (useful for testing with seedable rngs).
pub fn get_key_pair_from_rng<KP: KeypairTraits, R>(csprng: &mut R) -> (SuiAddress, KP)
where
    R: rand::CryptoRng + rand::RngCore,
    <KP as KeypairTraits>::PubKey: SuiPublicKey,
{
    let kp = KP::generate(&mut StdRng::from_rng(csprng).unwrap());
    (kp.public().into(), kp)
}

// TODO: C-GETTER
pub fn get_key_pair_from_bytes<KP: KeypairTraits>(bytes: &[u8]) -> SuiResult<(SuiAddress, KP)>
where
    <KP as KeypairTraits>::PubKey: SuiPublicKey,
{
    let priv_length = <KP as KeypairTraits>::PrivKey::LENGTH;
    let pub_key_length = <KP as KeypairTraits>::PubKey::LENGTH;
    if bytes.len() != priv_length + pub_key_length {
        return Err(SuiError::KeyConversionError(format!(
            "Invalid input byte length, expected {}: {}",
            priv_length,
            bytes.len()
        )));
    }
    let sk = <KP as KeypairTraits>::PrivKey::from_bytes(
        bytes
            .get(..priv_length)
            .ok_or(SuiError::InvalidPrivateKey)?,
    )
    .map_err(|_| SuiError::InvalidPrivateKey)?;
    let kp: KP = sk.into();
    Ok((kp.public().into(), kp))
}

//
// Account Signatures
//

// Enums for signature scheme signatures
#[enum_dispatch]
#[derive(Clone, JsonSchema, Debug, PartialEq, Eq, Hash)]
pub enum Signature {
    Ed25519SuiSignature,
    Secp256k1SuiSignature,
    Secp256r1SuiSignature,
}

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes = self.as_ref();

        if serializer.is_human_readable() {
            let s = Base64::encode(bytes);
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
            Base64::decode(&s).map_err(|e| Error::custom(e.to_string()))?
        } else {
            let data: Vec<u8> = Vec::deserialize(deserializer)?;
            data
        };

        Self::from_bytes(&bytes).map_err(|e| Error::custom(e.to_string()))
    }
}

impl Signature {
    /// The messaged passed in is already hashed form.
    pub fn new_hashed(hashed_msg: &[u8], secret: &dyn Signer<Signature>) -> Self {
        Signer::sign(secret, hashed_msg)
    }

    pub fn new_secure<T>(value: &IntentMessage<T>, secret: &dyn Signer<Signature>) -> Self
    where
        T: Serialize,
    {
        // Compute the BCS hash of the value in intent message. In the case of transaction data,
        // this is the BCS hash of `struct TransactionData`, different from the transaction digest
        // itself that computes the BCS hash of the Rust type prefix and `struct TransactionData`.
        // (See `fn digest` in `impl Message for SenderSignedData`).
        let mut hasher = DefaultHash::default();
        hasher.update(&bcs::to_bytes(&value).expect("Message serialization should not fail"));

        Signer::sign(secret, &hasher.finalize().digest)
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        match self {
            Signature::Ed25519SuiSignature(sig) => sig.as_ref(),
            Signature::Secp256k1SuiSignature(sig) => sig.as_ref(),
            Signature::Secp256r1SuiSignature(sig) => sig.as_ref(),
        }
    }
}
impl AsMut<[u8]> for Signature {
    fn as_mut(&mut self) -> &mut [u8] {
        match self {
            Signature::Ed25519SuiSignature(sig) => sig.as_mut(),
            Signature::Secp256k1SuiSignature(sig) => sig.as_mut(),
            Signature::Secp256r1SuiSignature(sig) => sig.as_mut(),
        }
    }
}

impl ToFromBytes for Signature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        match bytes.first() {
            Some(x) => {
                if x == &Ed25519SuiSignature::SCHEME.flag() {
                    Ok(<Ed25519SuiSignature as ToFromBytes>::from_bytes(bytes)?.into())
                } else if x == &Secp256k1SuiSignature::SCHEME.flag() {
                    Ok(<Secp256k1SuiSignature as ToFromBytes>::from_bytes(bytes)?.into())
                } else if x == &Secp256r1SuiSignature::SCHEME.flag() {
                    Ok(<Secp256r1SuiSignature as ToFromBytes>::from_bytes(bytes)?.into())
                } else {
                    Err(FastCryptoError::InvalidInput)
                }
            }
            _ => Err(FastCryptoError::InvalidInput),
        }
    }
}

//
// BLS Port
//

impl SuiPublicKey for BLS12381PublicKey {
    const SIGNATURE_SCHEME: SignatureScheme = SignatureScheme::BLS12381;
}

//
// Ed25519 Sui Signature port
//

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash, AsRef, AsMut)]
#[as_ref(forward)]
#[as_mut(forward)]
pub struct Ed25519SuiSignature(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; Ed25519PublicKey::LENGTH + Ed25519Signature::LENGTH + 1],
);

// Implementation useful for simplify testing when mock signature is needed
impl Default for Ed25519SuiSignature {
    fn default() -> Self {
        Self([0; Ed25519PublicKey::LENGTH + Ed25519Signature::LENGTH + 1])
    }
}

impl SuiSignatureInner for Ed25519SuiSignature {
    type Sig = Ed25519Signature;
    type PubKey = Ed25519PublicKey;
    type KeyPair = Ed25519KeyPair;
    const LENGTH: usize = Ed25519PublicKey::LENGTH + Ed25519Signature::LENGTH + 1;
}

impl SuiPublicKey for Ed25519PublicKey {
    const SIGNATURE_SCHEME: SignatureScheme = SignatureScheme::ED25519;
}

impl ToFromBytes for Ed25519SuiSignature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        if bytes.len() != Self::LENGTH {
            return Err(FastCryptoError::InputLengthWrong(Self::LENGTH));
        }
        let mut sig_bytes = [0; Self::LENGTH];
        sig_bytes.copy_from_slice(bytes);
        Ok(Self(sig_bytes))
    }
}

impl Signer<Signature> for Ed25519KeyPair {
    fn sign(&self, msg: &[u8]) -> Signature {
        Ed25519SuiSignature::new(self, msg).into()
    }
}

//
// Secp256k1 Sui Signature port
//
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash, AsRef, AsMut)]
#[as_ref(forward)]
#[as_mut(forward)]
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

impl SuiPublicKey for Secp256k1PublicKey {
    const SIGNATURE_SCHEME: SignatureScheme = SignatureScheme::Secp256k1;
}

impl ToFromBytes for Secp256k1SuiSignature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        if bytes.len() != Self::LENGTH {
            return Err(FastCryptoError::InputLengthWrong(Self::LENGTH));
        }
        let mut sig_bytes = [0; Self::LENGTH];
        sig_bytes.copy_from_slice(bytes);
        Ok(Self(sig_bytes))
    }
}

impl Signer<Signature> for Secp256k1KeyPair {
    fn sign(&self, msg: &[u8]) -> Signature {
        Secp256k1SuiSignature::new(self, msg).into()
    }
}

//
// Secp256r1 Sui Signature port
//
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash, AsRef, AsMut)]
#[as_ref(forward)]
#[as_mut(forward)]
pub struct Secp256r1SuiSignature(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    [u8; Secp256r1PublicKey::LENGTH + Secp256r1Signature::LENGTH + 1],
);

impl SuiSignatureInner for Secp256r1SuiSignature {
    type Sig = Secp256r1Signature;
    type PubKey = Secp256r1PublicKey;
    type KeyPair = Secp256r1KeyPair;
    const LENGTH: usize = Secp256r1PublicKey::LENGTH + Secp256r1Signature::LENGTH + 1;
}

impl SuiPublicKey for Secp256r1PublicKey {
    const SIGNATURE_SCHEME: SignatureScheme = SignatureScheme::Secp256r1;
}

impl ToFromBytes for Secp256r1SuiSignature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        if bytes.len() != Self::LENGTH {
            return Err(FastCryptoError::InputLengthWrong(Self::LENGTH));
        }
        let mut sig_bytes = [0; Self::LENGTH];
        sig_bytes.copy_from_slice(bytes);
        Ok(Self(sig_bytes))
    }
}

impl Signer<Signature> for Secp256r1KeyPair {
    fn sign(&self, msg: &[u8]) -> Signature {
        Secp256r1SuiSignature::new(self, msg).into()
    }
}

//
// This struct exists due to the limitations of the `enum_dispatch` library.
//
pub trait SuiSignatureInner: Sized + ToFromBytes + PartialEq + Eq + Hash {
    type Sig: Authenticator<PubKey = Self::PubKey>;
    type PubKey: VerifyingKey<Sig = Self::Sig> + SuiPublicKey;
    type KeyPair: KeypairTraits<PubKey = Self::PubKey, Sig = Self::Sig>;

    const LENGTH: usize = Self::Sig::LENGTH + Self::PubKey::LENGTH + 1;
    const SCHEME: SignatureScheme = Self::PubKey::SIGNATURE_SCHEME;

    /// Returns the deserialized signature and deserialized pubkey.
    fn get_verification_inputs(&self) -> SuiResult<(Self::Sig, Self::PubKey)> {
        let pk = Self::PubKey::from_bytes(self.public_key_bytes())
            .map_err(|_| SuiError::KeyConversionError("Invalid public key".to_string()))?;

        // deserialize the signature
        let signature = Self::Sig::from_bytes(self.signature_bytes()).map_err(|_| {
            SuiError::InvalidSignature {
                error: "Fail to get pubkey and sig".to_string(),
            }
        })?;

        Ok((signature, pk))
    }

    fn new(kp: &Self::KeyPair, message: &[u8]) -> Self {
        let sig = Signer::sign(kp, message);

        let mut signature_bytes: Vec<u8> = Vec::new();
        signature_bytes
            .extend_from_slice(&[<Self::PubKey as SuiPublicKey>::SIGNATURE_SCHEME.flag()]);
        signature_bytes.extend_from_slice(sig.as_ref());
        signature_bytes.extend_from_slice(kp.public().as_ref());
        Self::from_bytes(&signature_bytes[..])
            .expect("Serialized signature did not have expected size")
    }
}

pub trait SuiPublicKey: VerifyingKey {
    const SIGNATURE_SCHEME: SignatureScheme;
}

#[enum_dispatch(Signature)]
pub trait SuiSignature: Sized + ToFromBytes {
    fn signature_bytes(&self) -> &[u8];
    fn public_key_bytes(&self) -> &[u8];
    fn scheme(&self) -> SignatureScheme;

    fn verify_secure<T>(
        &self,
        value: &IntentMessage<T>,
        author: SuiAddress,
        scheme: SignatureScheme,
    ) -> SuiResult<()>
    where
        T: Serialize;
}

impl<S: SuiSignatureInner + Sized> SuiSignature for S {
    fn signature_bytes(&self) -> &[u8] {
        // Access array slice is safe because the array bytes is initialized as
        // flag || signature || pubkey with its defined length.
        &self.as_ref()[1..1 + S::Sig::LENGTH]
    }

    fn public_key_bytes(&self) -> &[u8] {
        // Access array slice is safe because the array bytes is initialized as
        // flag || signature || pubkey with its defined length.
        &self.as_ref()[S::Sig::LENGTH + 1..]
    }

    fn scheme(&self) -> SignatureScheme {
        S::PubKey::SIGNATURE_SCHEME
    }

    fn verify_secure<T>(
        &self,
        value: &IntentMessage<T>,
        author: SuiAddress,
        scheme: SignatureScheme,
    ) -> Result<(), SuiError>
    where
        T: Serialize,
    {
        let mut hasher = DefaultHash::default();
        hasher.update(&bcs::to_bytes(&value).expect("Message serialization should not fail"));
        let digest = hasher.finalize().digest;

        let (sig, pk) = &self.get_verification_inputs()?;
        match scheme {
            SignatureScheme::ZkLoginAuthenticator => {} // Pass this check because zk login does not derive address from pubkey.
            _ => {
                let address = SuiAddress::from(pk);
                if author != address {
                    return Err(SuiError::IncorrectSigner {
                        error: format!(
                            "Incorrect signer, expected {:?}, got {:?}",
                            author, address
                        ),
                    });
                }
            }
        }

        pk.verify(&digest, sig)
            .map_err(|e| SuiError::InvalidSignature {
                error: format!("Fail to verify user sig {}", e),
            })
    }
}

/// AuthoritySignInfoTrait is a trait used specifically for a few structs in messages.rs
/// to template on whether the struct is signed by an authority. We want to limit how
/// those structs can be instantiated on, hence the sealed trait.
/// TODO: We could also add the aggregated signature as another impl of the trait.
///       This will make CertifiedTransaction also an instance of the same struct.
pub trait AuthoritySignInfoTrait: private::SealedAuthoritySignInfoTrait {
    fn verify_secure<T: Serialize>(
        &self,
        data: &T,
        intent: Intent,
        committee: &Committee,
    ) -> SuiResult;

    fn add_to_verification_obligation<'a>(
        &self,
        committee: &'a Committee,
        obligation: &mut VerificationObligation<'a>,
        message_index: usize,
    ) -> SuiResult<()>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EmptySignInfo {}
impl AuthoritySignInfoTrait for EmptySignInfo {
    fn verify_secure<T: Serialize>(
        &self,
        _data: &T,
        _intent: Intent,
        _committee: &Committee,
    ) -> SuiResult {
        Ok(())
    }

    fn add_to_verification_obligation<'a>(
        &self,
        _committee: &'a Committee,
        _obligation: &mut VerificationObligation<'a>,
        _message_index: usize,
    ) -> SuiResult<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct AuthoritySignInfo {
    pub epoch: EpochId,
    pub authority: AuthorityName,
    pub signature: AuthoritySignature,
}

impl AuthoritySignInfoTrait for AuthoritySignInfo {
    fn verify_secure<T: Serialize>(
        &self,
        data: &T,
        intent: Intent,
        committee: &Committee,
    ) -> SuiResult<()> {
        let mut obligation = VerificationObligation::default();
        let idx = obligation.add_message(data, self.epoch, intent);
        self.add_to_verification_obligation(committee, &mut obligation, idx)?;
        obligation.verify_all()?;
        Ok(())
    }

    fn add_to_verification_obligation<'a>(
        &self,
        committee: &'a Committee,
        obligation: &mut VerificationObligation<'a>,
        message_index: usize,
    ) -> SuiResult<()> {
        fp_ensure!(
            self.epoch == committee.epoch(),
            SuiError::WrongEpoch {
                expected_epoch: committee.epoch(),
                actual_epoch: self.epoch,
            }
        );
        let weight = committee.weight(&self.authority);
        fp_ensure!(
            weight > 0,
            SuiError::UnknownSigner {
                signer: Some(self.authority.concise().to_string()),
                index: None,
                committee: Box::new(committee.clone())
            }
        );

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
                error: "Fail to aggregator auth sig".to_string(),
            })?;
        Ok(())
    }
}

impl AuthoritySignInfo {
    pub fn new<T>(
        epoch: EpochId,
        value: &T,
        intent: Intent,
        name: AuthorityName,
        secret: &dyn Signer<AuthoritySignature>,
    ) -> Self
    where
        T: Serialize,
    {
        Self {
            epoch,
            authority: name,
            signature: AuthoritySignature::new_secure(
                &IntentMessage::new(intent, value),
                &epoch,
                secret,
            ),
        }
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

// Variant of [AuthorityStrongQuorumSignInfo] but with a serialized signature, to be used in
// external APIs.
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SuiAuthorityStrongQuorumSignInfo {
    pub epoch: EpochId,
    pub signature: AggregateAuthoritySignatureAsBytes,
    #[schemars(with = "Base64")]
    #[serde_as(as = "SuiBitmap")]
    pub signers_map: RoaringBitmap,
}

impl From<&AuthorityStrongQuorumSignInfo> for SuiAuthorityStrongQuorumSignInfo {
    fn from(info: &AuthorityStrongQuorumSignInfo) -> Self {
        Self {
            epoch: info.epoch,
            signature: (&info.signature).into(),
            signers_map: info.signers_map.clone(),
        }
    }
}

impl TryFrom<&SuiAuthorityStrongQuorumSignInfo> for AuthorityStrongQuorumSignInfo {
    type Error = FastCryptoError;

    fn try_from(info: &SuiAuthorityStrongQuorumSignInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            epoch: info.epoch,
            signature: (&info.signature).try_into()?,
            signers_map: info.signers_map.clone(),
        })
    }
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
static_assertions::assert_not_impl_any!(AuthorityStrongQuorumSignInfo: Hash, Eq, PartialEq);

impl<const STRONG_THRESHOLD: bool> AuthoritySignInfoTrait
    for AuthorityQuorumSignInfo<STRONG_THRESHOLD>
{
    fn verify_secure<T: Serialize>(
        &self,
        data: &T,
        intent: Intent,
        committee: &Committee,
    ) -> SuiResult {
        let mut obligation = VerificationObligation::default();
        let idx = obligation.add_message(data, self.epoch, intent);
        self.add_to_verification_obligation(committee, &mut obligation, idx)?;
        obligation.verify_all()?;
        Ok(())
    }

    fn add_to_verification_obligation<'a>(
        &self,
        committee: &'a Committee,
        obligation: &mut VerificationObligation<'a>,
        message_index: usize,
    ) -> SuiResult<()> {
        // Check epoch
        fp_ensure!(
            self.epoch == committee.epoch(),
            SuiError::WrongEpoch {
                expected_epoch: committee.epoch(),
                actual_epoch: self.epoch,
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
                .ok_or_else(|| SuiError::UnknownSigner {
                    signer: None,
                    index: Some(authority_index),
                    committee: Box::new(committee.clone()),
                })?;

            // Update weight.
            let voting_rights = committee.weight(authority);
            fp_ensure!(
                voting_rights > 0,
                SuiError::UnknownSigner {
                    signer: Some(authority.concise().to_string()),
                    index: Some(authority_index),
                    committee: Box::new(committee.clone()),
                }
            );
            weight += voting_rights;

            selected_public_keys.push(committee.public_key(authority)?);
        }

        fp_ensure!(
            weight >= Self::quorum_threshold(committee),
            SuiError::CertificateRequiresQuorum
        );

        Ok(())
    }
}

impl<const STRONG_THRESHOLD: bool> AuthorityQuorumSignInfo<STRONG_THRESHOLD> {
    pub fn new_from_auth_sign_infos(
        auth_sign_infos: Vec<AuthoritySignInfo>,
        committee: &Committee,
    ) -> SuiResult<Self> {
        fp_ensure!(
            auth_sign_infos.iter().all(|a| a.epoch == committee.epoch),
            SuiError::InvalidSignature {
                error: "All signatures must be from the same epoch as the committee".to_string()
            }
        );
        let total_stake: StakeUnit = auth_sign_infos
            .iter()
            .map(|a| committee.weight(&a.authority))
            .sum();
        fp_ensure!(
            total_stake >= Self::quorum_threshold(committee),
            SuiError::InvalidSignature {
                error: "Signatures don't have enough stake to form a quorum".to_string()
            }
        );

        let signatures: BTreeMap<_, _> = auth_sign_infos
            .into_iter()
            .map(|a| (a.authority, a.signature))
            .collect();
        let mut map = RoaringBitmap::new();
        for pk in signatures.keys() {
            map.insert(
                committee
                    .authority_index(pk)
                    .ok_or_else(|| SuiError::UnknownSigner {
                        signer: Some(pk.concise().to_string()),
                        index: None,
                        committee: Box::new(committee.clone()),
                    })?,
            );
        }
        let sigs: Vec<AuthoritySignature> = signatures.into_values().collect();

        Ok(AuthorityQuorumSignInfo {
            epoch: committee.epoch,
            signature: AggregateAuthoritySignature::aggregate(&sigs).map_err(|e| {
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

    pub fn quorum_threshold(committee: &Committee) -> StakeUnit {
        committee.threshold::<STRONG_THRESHOLD>()
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
    fn from_signable_bytes(bytes: &[u8]) -> Result<Self, Error>;
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
mod bcs_signable {

    pub trait BcsSignable: serde::Serialize + serde::de::DeserializeOwned {}
    impl BcsSignable for crate::committee::Committee {}
    impl BcsSignable for crate::messages_checkpoint::CheckpointSummary {}
    impl BcsSignable for crate::messages_checkpoint::CheckpointContents {}

    impl BcsSignable for crate::effects::TransactionEffects {}
    impl BcsSignable for crate::effects::TransactionEvents {}
    impl BcsSignable for crate::transaction::TransactionData {}
    impl BcsSignable for crate::transaction::SenderSignedData {}
    impl BcsSignable for crate::object::ObjectInner {}

    impl BcsSignable for crate::accumulator::Accumulator {}

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

impl<W> Signable<W> for EpochId
where
    W: std::io::Write,
{
    fn write(&self, writer: &mut W) {
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
        Ok(bcs::from_bytes(bytes.get(name_byte_len..).ok_or_else(
            || anyhow!("Failed to deserialize to {name}."),
        )?)?)
    }
}

fn hash<S: Signable<H>, H: HashFunction<DIGEST_SIZE>, const DIGEST_SIZE: usize>(
    signable: &S,
) -> [u8; DIGEST_SIZE] {
    let mut digest = H::default();
    signable.write(&mut digest);
    let hash = digest.finalize();
    hash.into()
}

pub fn default_hash<S: Signable<DefaultHash>>(signable: &S) -> [u8; 32] {
    hash::<S, DefaultHash, 32>(signable)
}

#[derive(Default)]
pub struct VerificationObligation<'a> {
    pub messages: Vec<Vec<u8>>,
    pub signatures: Vec<AggregateAuthoritySignature>,
    pub public_keys: Vec<Vec<&'a AuthorityPublicKey>>,
}

impl<'a> VerificationObligation<'a> {
    pub fn new() -> VerificationObligation<'a> {
        VerificationObligation::default()
    }

    /// Add a new message to the list of messages to be verified.
    /// Returns the index of the message.
    pub fn add_message<T>(&mut self, message_value: &T, epoch: EpochId, intent: Intent) -> usize
    where
        T: Serialize,
    {
        let intent_msg = IntentMessage::new(intent, message_value);
        let mut intent_msg_bytes =
            bcs::to_bytes(&intent_msg).expect("Message serialization should not fail");
        epoch.write(&mut intent_msg_bytes);
        self.signatures.push(AggregateAuthoritySignature::default());
        self.public_keys.push(Vec::new());
        self.messages.push(intent_msg_bytes);
        self.messages.len() - 1
    }

    // Attempts to add signature and public key to the obligation. If this fails, ensure to call `verify` manually.
    pub fn add_signature_and_public_key(
        &mut self,
        signature: &AuthoritySignature,
        public_key: &'a AuthorityPublicKey,
        idx: usize,
    ) -> SuiResult<()> {
        self.public_keys
            .get_mut(idx)
            .ok_or(SuiError::InvalidAuthenticator)?
            .push(public_key);
        self.signatures
            .get_mut(idx)
            .ok_or(SuiError::InvalidAuthenticator)?
            .add_signature(signature.clone())
            .map_err(|_| SuiError::InvalidSignature {
                error: "Failed to add signature to obligation".to_string(),
            })?;
        Ok(())
    }

    pub fn verify_all(self) -> SuiResult<()> {
        let mut pks = Vec::with_capacity(self.public_keys.len());
        for pk in self.public_keys.clone() {
            pks.push(pk.into_iter());
        }
        AggregateAuthoritySignature::batch_verify(
            &self.signatures.iter().collect::<Vec<_>>()[..],
            pks,
            &self.messages.iter().map(|x| &x[..]).collect::<Vec<_>>()[..],
        )
        .map_err(|e| {
            let message = format!(
                "pks: {:?}, messages: {:?}, sigs: {:?}",
                &self.public_keys,
                self.messages
                    .iter()
                    .map(Base64::encode)
                    .collect::<Vec<String>>(),
                &self
                    .signatures
                    .iter()
                    .map(|s| Base64::encode(s.as_ref()))
                    .collect::<Vec<String>>()
            );

            let chunk_size = 2048;

            // This error message may be very long, so we print out the error in chunks of to avoid
            // hitting a max log line length on the system.
            for (i, chunk) in message
                .as_bytes()
                .chunks(chunk_size)
                .map(std::str::from_utf8)
                .enumerate()
            {
                warn!(
                    "Failed to batch verify aggregated auth sig: {} (chunk {}): {}",
                    e,
                    i,
                    chunk.unwrap()
                );
            }

            SuiError::InvalidSignature {
                error: format!("Failed to batch verify aggregated auth sig: {}", e),
            }
        })?;
        Ok(())
    }
}

pub mod bcs_signable_test {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Serialize, Deserialize)]
    pub struct Foo(pub String);

    #[cfg(test)]
    #[derive(Serialize, Deserialize)]
    pub struct Bar(pub String);

    #[cfg(test)]
    use super::VerificationObligation;

    #[cfg(test)]
    pub fn get_obligation_input<T>(value: &T) -> (VerificationObligation<'_>, usize)
    where
        T: super::bcs_signable::BcsSignable,
    {
        use shared_crypto::intent::{Intent, IntentScope};

        let mut obligation = VerificationObligation::default();
        // Add the obligation of the authority signature verifications.
        let idx = obligation.add_message(
            value,
            0,
            Intent::sui_app(IntentScope::SenderSignedTransaction),
        );
        (obligation, idx)
    }
}

#[derive(
    Clone,
    Copy,
    Deserialize,
    Serialize,
    JsonSchema,
    Debug,
    EnumString,
    strum_macros::Display,
    PartialEq,
    Eq,
)]
#[strum(serialize_all = "lowercase")]
pub enum SignatureScheme {
    ED25519,
    Secp256k1,
    Secp256r1,
    BLS12381, // This is currently not supported for user Sui Address.
    MultiSig,
    ZkLoginAuthenticator,
    PasskeyAuthenticator,
}

impl SignatureScheme {
    pub fn flag(&self) -> u8 {
        match self {
            SignatureScheme::ED25519 => 0x00,
            SignatureScheme::Secp256k1 => 0x01,
            SignatureScheme::Secp256r1 => 0x02,
            SignatureScheme::MultiSig => 0x03,
            SignatureScheme::BLS12381 => 0x04, // This is currently not supported for user Sui Address.
            SignatureScheme::ZkLoginAuthenticator => 0x05,
            SignatureScheme::PasskeyAuthenticator => 0x06,
        }
    }

    pub fn from_flag(flag: &str) -> Result<SignatureScheme, SuiError> {
        let byte_int = flag
            .parse::<u8>()
            .map_err(|_| SuiError::KeyConversionError("Invalid key scheme".to_string()))?;
        Self::from_flag_byte(&byte_int)
    }

    pub fn from_flag_byte(byte_int: &u8) -> Result<SignatureScheme, SuiError> {
        match byte_int {
            0x00 => Ok(SignatureScheme::ED25519),
            0x01 => Ok(SignatureScheme::Secp256k1),
            0x02 => Ok(SignatureScheme::Secp256r1),
            0x03 => Ok(SignatureScheme::MultiSig),
            0x04 => Ok(SignatureScheme::BLS12381),
            0x05 => Ok(SignatureScheme::ZkLoginAuthenticator),
            0x06 => Ok(SignatureScheme::PasskeyAuthenticator),
            _ => Err(SuiError::KeyConversionError(
                "Invalid key scheme".to_string(),
            )),
        }
    }
}
/// Unlike [enum Signature], [enum CompressedSignature] does not contain public key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub enum CompressedSignature {
    Ed25519(Ed25519SignatureAsBytes),
    Secp256k1(Secp256k1SignatureAsBytes),
    Secp256r1(Secp256r1SignatureAsBytes),
    ZkLogin(ZkLoginAuthenticatorAsBytes),
    Passkey(PasskeyAuthenticatorAsBytes),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ZkLoginAuthenticatorAsBytes(#[schemars(with = "Base64")] pub Vec<u8>);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct PasskeyAuthenticatorAsBytes(#[schemars(with = "Base64")] pub Vec<u8>);

impl AsRef<[u8]> for CompressedSignature {
    fn as_ref(&self) -> &[u8] {
        match self {
            CompressedSignature::Ed25519(sig) => &sig.0,
            CompressedSignature::Secp256k1(sig) => &sig.0,
            CompressedSignature::Secp256r1(sig) => &sig.0,
            CompressedSignature::ZkLogin(sig) => &sig.0,
            CompressedSignature::Passkey(sig) => &sig.0,
        }
    }
}

impl FromStr for Signature {
    type Err = eyre::Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode_base64(s).map_err(|e| eyre!("Fail to decode base64 {}", e.to_string()))
    }
}

impl FromStr for PublicKey {
    type Err = eyre::Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode_base64(s).map_err(|e| eyre!("Fail to decode base64 {}", e.to_string()))
    }
}

impl FromStr for GenericSignature {
    type Err = eyre::Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode_base64(s).map_err(|e| eyre!("Fail to decode base64 {}", e.to_string()))
    }
}

//
// Types for randomness generation
//
pub type RandomnessSignature = fastcrypto_tbls::types::Signature;
pub type RandomnessPartialSignature = fastcrypto_tbls::tbls::PartialSignature<RandomnessSignature>;
pub type RandomnessPrivateKey =
    fastcrypto_tbls::ecies::PrivateKey<fastcrypto::groups::bls12381::G2Element>;

/// Round number of generated randomness.
#[derive(Clone, Copy, Hash, Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RandomnessRound(pub u64);

impl Display for RandomnessRound {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Add for RandomnessRound {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl std::ops::Add<u64> for RandomnessRound {
    type Output = Self;
    fn add(self, other: u64) -> Self {
        Self(self.0 + other)
    }
}

impl std::ops::Sub for RandomnessRound {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl std::ops::Sub<u64> for RandomnessRound {
    type Output = Self;
    fn sub(self, other: u64) -> Self {
        Self(self.0 - other)
    }
}

impl RandomnessRound {
    pub fn new(round: u64) -> Self {
        Self(round)
    }

    pub fn checked_add(self, rhs: u64) -> Option<Self> {
        self.0.checked_add(rhs).map(Self)
    }

    pub fn signature_message(&self) -> Vec<u8> {
        "random_beacon round "
            .as_bytes()
            .iter()
            .cloned()
            .chain(bcs::to_bytes(&self.0).expect("serialization should not fail"))
            .collect()
    }
}
