// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{env, fmt};

use crate::{error::SuiError, sui_serde::Readable};
use fastcrypto::encoding::{Base58, Encoding, Hex};
use once_cell::sync::{Lazy, OnceCell};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};
use sui_protocol_config::Chain;
use tracing::info;

/// A representation of a 32 byte digest
#[serde_as]
#[derive(
    Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct Digest(
    #[schemars(with = "Base58")]
    #[serde_as(as = "Readable<Base58, Bytes>")]
    [u8; 32],
);

impl Digest {
    pub const ZERO: Self = Digest([0; 32]);

    pub const fn new(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    pub fn generate<R: rand::RngCore + rand::CryptoRng>(mut rng: R) -> Self {
        let mut bytes = [0; 32];
        rng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn random() -> Self {
        Self::generate(rand::thread_rng())
    }

    pub const fn inner(&self) -> &[u8; 32] {
        &self.0
    }

    pub const fn into_inner(self) -> [u8; 32] {
        self.0
    }

    pub fn next_lexicographical(&self) -> Option<Self> {
        let mut next_digest = *self;
        let pos = next_digest.0.iter().rposition(|&byte| byte != 255)?;
        next_digest.0[pos] += 1;
        next_digest
            .0
            .iter_mut()
            .skip(pos + 1)
            .for_each(|byte| *byte = 0);
        Some(next_digest)
    }
}

impl AsRef<[u8]> for Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8; 32]> for Digest {
    fn as_ref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<Digest> for [u8; 32] {
    fn from(digest: Digest) -> Self {
        digest.into_inner()
    }
}

impl From<[u8; 32]> for Digest {
    fn from(digest: [u8; 32]) -> Self {
        Self::new(digest)
    }
}

impl TryFrom<Vec<u8>> for Digest {
    type Error = SuiError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, SuiError> {
        let bytes: [u8; 32] =
            <[u8; 32]>::try_from(&bytes[..]).map_err(|_| SuiError::InvalidDigestLength {
                expected: 32,
                actual: bytes.len(),
            })?;

        Ok(Self::from(bytes))
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO avoid the allocation
        f.write_str(&Base58::encode(self.0))
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::LowerHex for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }

        for byte in self.0 {
            write!(f, "{:02x}", byte)?;
        }

        Ok(())
    }
}

impl fmt::UpperHex for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }

        for byte in self.0 {
            write!(f, "{:02X}", byte)?;
        }

        Ok(())
    }
}

/// Representation of a network's identifier by the genesis checkpoint's digest
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub struct ChainIdentifier(CheckpointDigest);

pub const MAINNET_CHAIN_IDENTIFIER_BASE58: &str = "4btiuiMPvEENsttpZC7CZ53DruC3MAgfznDbASZ7DR6S";
pub const TESTNET_CHAIN_IDENTIFIER_BASE58: &str = "69WiPg3DAQiwdxfncX6wYQ2siKwAe6L9BZthQea3JNMD";

pub static MAINNET_CHAIN_IDENTIFIER: OnceCell<ChainIdentifier> = OnceCell::new();
pub static TESTNET_CHAIN_IDENTIFIER: OnceCell<ChainIdentifier> = OnceCell::new();

/// For testing purposes or bootstrapping regenesis chain configuration, you can set
/// this environment variable to force protocol config to use a specific Chain.
const SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE_ENV_VAR_NAME: &str = "SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE";

static SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE: Lazy<Option<Chain>> = Lazy::new(|| {
    if let Ok(s) = env::var(SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE_ENV_VAR_NAME) {
        info!("SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE: {:?}", s);
        match s.as_str() {
            "mainnet" => Some(Chain::Mainnet),
            "testnet" => Some(Chain::Testnet),
            "" => None,
            _ => panic!("unrecognized SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE: {s:?}"),
        }
    } else {
        None
    }
});

impl ChainIdentifier {
    /// take a short 4 byte identifier and convert it into a ChainIdentifier
    /// short ids come from the JSON RPC getChainIdentifier and are encoded in hex
    pub fn from_chain_short_id(short_id: &String) -> Option<Self> {
        if Hex::from_bytes(&Base58::decode(MAINNET_CHAIN_IDENTIFIER_BASE58).ok()?)
            .encoded_with_format()
            .starts_with(&format!("0x{}", short_id))
        {
            Some(get_mainnet_chain_identifier())
        } else if Hex::from_bytes(&Base58::decode(TESTNET_CHAIN_IDENTIFIER_BASE58).ok()?)
            .encoded_with_format()
            .starts_with(&format!("0x{}", short_id))
        {
            Some(get_testnet_chain_identifier())
        } else {
            None
        }
    }

    pub fn chain(&self) -> Chain {
        let mainnet_id = get_mainnet_chain_identifier();
        let testnet_id = get_testnet_chain_identifier();

        let chain = match self {
            id if *id == mainnet_id => Chain::Mainnet,
            id if *id == testnet_id => Chain::Testnet,
            _ => Chain::Unknown,
        };
        if let Some(override_chain) = *SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE {
            if chain != Chain::Unknown {
                panic!("not allowed to override real chain {chain:?}");
            }
            return override_chain;
        }

        chain
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.inner()
    }
}

pub fn get_mainnet_chain_identifier() -> ChainIdentifier {
    let digest = MAINNET_CHAIN_IDENTIFIER.get_or_init(|| {
        let digest = CheckpointDigest::new(
            Base58::decode(MAINNET_CHAIN_IDENTIFIER_BASE58)
                .expect("mainnet genesis checkpoint digest literal is invalid")
                .try_into()
                .expect("Mainnet genesis checkpoint digest literal has incorrect length"),
        );
        ChainIdentifier::from(digest)
    });
    *digest
}

pub fn get_testnet_chain_identifier() -> ChainIdentifier {
    let digest = TESTNET_CHAIN_IDENTIFIER.get_or_init(|| {
        let digest = CheckpointDigest::new(
            Base58::decode(TESTNET_CHAIN_IDENTIFIER_BASE58)
                .expect("testnet genesis checkpoint digest literal is invalid")
                .try_into()
                .expect("Testnet genesis checkpoint digest literal has incorrect length"),
        );
        ChainIdentifier::from(digest)
    });
    *digest
}

impl fmt::Display for ChainIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 .0 .0[0..4].iter() {
            write!(f, "{:02x}", byte)?;
        }

        Ok(())
    }
}

impl From<CheckpointDigest> for ChainIdentifier {
    fn from(digest: CheckpointDigest) -> Self {
        Self(digest)
    }
}

/// Representation of a Checkpoint's digest
#[derive(
    Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct CheckpointDigest(Digest);

impl CheckpointDigest {
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }

    pub fn generate<R: rand::RngCore + rand::CryptoRng>(rng: R) -> Self {
        Self(Digest::generate(rng))
    }

    pub fn random() -> Self {
        Self(Digest::random())
    }

    pub const fn inner(&self) -> &[u8; 32] {
        self.0.inner()
    }

    pub const fn into_inner(self) -> [u8; 32] {
        self.0.into_inner()
    }

    pub fn base58_encode(&self) -> String {
        Base58::encode(self.0)
    }

    pub fn next_lexicographical(&self) -> Option<Self> {
        self.0.next_lexicographical().map(Self)
    }
}

impl AsRef<[u8]> for CheckpointDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<[u8; 32]> for CheckpointDigest {
    fn as_ref(&self) -> &[u8; 32] {
        self.0.as_ref()
    }
}

impl From<CheckpointDigest> for [u8; 32] {
    fn from(digest: CheckpointDigest) -> Self {
        digest.into_inner()
    }
}

impl From<[u8; 32]> for CheckpointDigest {
    fn from(digest: [u8; 32]) -> Self {
        Self::new(digest)
    }
}

impl TryFrom<Vec<u8>> for CheckpointDigest {
    type Error = SuiError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, SuiError> {
        Digest::try_from(bytes).map(CheckpointDigest)
    }
}

impl fmt::Display for CheckpointDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for CheckpointDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CheckpointDigest").field(&self.0).finish()
    }
}

impl fmt::LowerHex for CheckpointDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for CheckpointDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl std::str::FromStr for CheckpointDigest {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0; 32];
        let buffer = Base58::decode(s).map_err(|e| anyhow::anyhow!(e))?;
        if buffer.len() != 32 {
            return Err(anyhow::anyhow!("Invalid digest length. Expected 32 bytes"));
        }
        result.copy_from_slice(&buffer);
        Ok(CheckpointDigest::new(result))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct CheckpointContentsDigest(Digest);

impl CheckpointContentsDigest {
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }

    pub fn generate<R: rand::RngCore + rand::CryptoRng>(rng: R) -> Self {
        Self(Digest::generate(rng))
    }

    pub fn random() -> Self {
        Self(Digest::random())
    }

    pub const fn inner(&self) -> &[u8; 32] {
        self.0.inner()
    }

    pub const fn into_inner(self) -> [u8; 32] {
        self.0.into_inner()
    }

    pub fn base58_encode(&self) -> String {
        Base58::encode(self.0)
    }

    pub fn next_lexicographical(&self) -> Option<Self> {
        self.0.next_lexicographical().map(Self)
    }
}

impl AsRef<[u8]> for CheckpointContentsDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<[u8; 32]> for CheckpointContentsDigest {
    fn as_ref(&self) -> &[u8; 32] {
        self.0.as_ref()
    }
}

impl TryFrom<Vec<u8>> for CheckpointContentsDigest {
    type Error = SuiError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, SuiError> {
        Digest::try_from(bytes).map(CheckpointContentsDigest)
    }
}

impl From<CheckpointContentsDigest> for [u8; 32] {
    fn from(digest: CheckpointContentsDigest) -> Self {
        digest.into_inner()
    }
}

impl From<[u8; 32]> for CheckpointContentsDigest {
    fn from(digest: [u8; 32]) -> Self {
        Self::new(digest)
    }
}

impl fmt::Display for CheckpointContentsDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for CheckpointContentsDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CheckpointContentsDigest")
            .field(&self.0)
            .finish()
    }
}

impl std::str::FromStr for CheckpointContentsDigest {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0; 32];
        let buffer = Base58::decode(s).map_err(|e| anyhow::anyhow!(e))?;
        if buffer.len() != 32 {
            return Err(anyhow::anyhow!("Invalid digest length. Expected 32 bytes"));
        }
        result.copy_from_slice(&buffer);
        Ok(CheckpointContentsDigest::new(result))
    }
}

impl fmt::LowerHex for CheckpointContentsDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for CheckpointContentsDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

/// A digest of a certificate, which commits to the signatures as well as the tx.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CertificateDigest(Digest);

impl CertificateDigest {
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }

    pub fn random() -> Self {
        Self(Digest::random())
    }
}

impl fmt::Debug for CertificateDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CertificateDigest").field(&self.0).finish()
    }
}

/// A digest of a SenderSignedData, which commits to the signatures as well as the tx.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SenderSignedDataDigest(Digest);

impl SenderSignedDataDigest {
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }
}

impl fmt::Debug for SenderSignedDataDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SenderSignedDataDigest")
            .field(&self.0)
            .finish()
    }
}

/// A transaction will have a (unique) digest.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct TransactionDigest(Digest);

impl Default for TransactionDigest {
    fn default() -> Self {
        Self::ZERO
    }
}

impl TransactionDigest {
    pub const ZERO: Self = Self(Digest::ZERO);

    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }

    pub const fn from_digest(digest: Digest) -> Self {
        Self(digest)
    }

    /// A digest we use to signify the parent transaction was the genesis,
    /// ie. for an object there is no parent digest.
    /// Note that this is not the same as the digest of the genesis transaction,
    /// which cannot be known ahead of time.
    // TODO(https://github.com/MystenLabs/sui/issues/65): we can pick anything here
    pub const fn genesis_marker() -> Self {
        Self::ZERO
    }

    pub fn generate<R: rand::RngCore + rand::CryptoRng>(rng: R) -> Self {
        Self(Digest::generate(rng))
    }

    pub fn random() -> Self {
        Self(Digest::random())
    }

    pub fn inner(&self) -> &[u8; 32] {
        self.0.inner()
    }

    pub fn into_inner(self) -> [u8; 32] {
        self.0.into_inner()
    }

    pub fn base58_encode(&self) -> String {
        Base58::encode(self.0)
    }

    pub fn next_lexicographical(&self) -> Option<Self> {
        self.0.next_lexicographical().map(Self)
    }
}

impl AsRef<[u8]> for TransactionDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<[u8; 32]> for TransactionDigest {
    fn as_ref(&self) -> &[u8; 32] {
        self.0.as_ref()
    }
}

impl From<TransactionDigest> for [u8; 32] {
    fn from(digest: TransactionDigest) -> Self {
        digest.into_inner()
    }
}

impl From<[u8; 32]> for TransactionDigest {
    fn from(digest: [u8; 32]) -> Self {
        Self::new(digest)
    }
}

impl fmt::Display for TransactionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for TransactionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("TransactionDigest").field(&self.0).finish()
    }
}

impl fmt::LowerHex for TransactionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for TransactionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl TryFrom<&[u8]> for TransactionDigest {
    type Error = crate::error::SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, crate::error::SuiError> {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| crate::error::SuiError::InvalidTransactionDigest)?;
        Ok(Self::new(arr))
    }
}

impl TryFrom<Vec<u8>> for TransactionDigest {
    type Error = crate::error::SuiError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, SuiError> {
        Digest::try_from(bytes).map(TransactionDigest)
    }
}

impl std::str::FromStr for TransactionDigest {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0; 32];
        let buffer = Base58::decode(s).map_err(|e| anyhow::anyhow!(e))?;
        if buffer.len() != 32 {
            return Err(anyhow::anyhow!("Invalid digest length. Expected 32 bytes"));
        }
        result.copy_from_slice(&buffer);
        Ok(TransactionDigest::new(result))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct TransactionEffectsDigest(Digest);

impl TransactionEffectsDigest {
    pub const ZERO: Self = Self(Digest::ZERO);

    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }

    pub fn generate<R: rand::RngCore + rand::CryptoRng>(rng: R) -> Self {
        Self(Digest::generate(rng))
    }

    pub fn random() -> Self {
        Self(Digest::random())
    }

    pub const fn inner(&self) -> &[u8; 32] {
        self.0.inner()
    }

    pub const fn into_inner(self) -> [u8; 32] {
        self.0.into_inner()
    }

    pub fn base58_encode(&self) -> String {
        Base58::encode(self.0)
    }

    pub fn next_lexicographical(&self) -> Option<Self> {
        self.0.next_lexicographical().map(Self)
    }
}

impl AsRef<[u8]> for TransactionEffectsDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<[u8; 32]> for TransactionEffectsDigest {
    fn as_ref(&self) -> &[u8; 32] {
        self.0.as_ref()
    }
}

impl TryFrom<Vec<u8>> for TransactionEffectsDigest {
    type Error = SuiError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, SuiError> {
        Digest::try_from(bytes).map(TransactionEffectsDigest)
    }
}

impl From<TransactionEffectsDigest> for [u8; 32] {
    fn from(digest: TransactionEffectsDigest) -> Self {
        digest.into_inner()
    }
}

impl From<[u8; 32]> for TransactionEffectsDigest {
    fn from(digest: [u8; 32]) -> Self {
        Self::new(digest)
    }
}

impl fmt::Display for TransactionEffectsDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for TransactionEffectsDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("TransactionEffectsDigest")
            .field(&self.0)
            .finish()
    }
}

impl fmt::LowerHex for TransactionEffectsDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for TransactionEffectsDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

#[serde_as]
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema)]
pub struct TransactionEventsDigest(Digest);

impl TransactionEventsDigest {
    pub const ZERO: Self = Self(Digest::ZERO);

    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }

    pub fn random() -> Self {
        Self(Digest::random())
    }

    pub fn next_lexicographical(&self) -> Option<Self> {
        self.0.next_lexicographical().map(Self)
    }

    pub fn into_inner(self) -> [u8; 32] {
        self.0.into_inner()
    }
}

impl fmt::Debug for TransactionEventsDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("TransactionEventsDigest")
            .field(&self.0)
            .finish()
    }
}

impl AsRef<[u8]> for TransactionEventsDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<[u8; 32]> for TransactionEventsDigest {
    fn as_ref(&self) -> &[u8; 32] {
        self.0.as_ref()
    }
}

impl TryFrom<Vec<u8>> for TransactionEventsDigest {
    type Error = SuiError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, SuiError> {
        Digest::try_from(bytes).map(TransactionEventsDigest)
    }
}

impl std::str::FromStr for TransactionEventsDigest {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0; 32];
        let buffer = Base58::decode(s).map_err(|e| anyhow::anyhow!(e))?;
        if buffer.len() != 32 {
            return Err(anyhow::anyhow!("Invalid digest length. Expected 32 bytes"));
        }
        result.copy_from_slice(&buffer);
        Ok(Self::new(result))
    }
}

#[serde_as]
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, JsonSchema)]
pub struct EffectsAuxDataDigest(Digest);

impl EffectsAuxDataDigest {
    pub const ZERO: Self = Self(Digest::ZERO);

    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }

    pub fn random() -> Self {
        Self(Digest::random())
    }

    pub fn next_lexicographical(&self) -> Option<Self> {
        self.0.next_lexicographical().map(Self)
    }

    pub fn into_inner(self) -> [u8; 32] {
        self.0.into_inner()
    }
}

impl fmt::Debug for EffectsAuxDataDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EffectsAuxDataDigest")
            .field(&self.0)
            .finish()
    }
}

impl AsRef<[u8]> for EffectsAuxDataDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<[u8; 32]> for EffectsAuxDataDigest {
    fn as_ref(&self) -> &[u8; 32] {
        self.0.as_ref()
    }
}

impl std::str::FromStr for EffectsAuxDataDigest {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0; 32];
        let buffer = Base58::decode(s).map_err(|e| anyhow::anyhow!(e))?;
        if buffer.len() != 32 {
            return Err(anyhow::anyhow!("Invalid digest length. Expected 32 bytes"));
        }
        result.copy_from_slice(&buffer);
        Ok(Self::new(result))
    }
}

// Each object has a unique digest
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ObjectDigest(Digest);

impl ObjectDigest {
    pub const MIN: ObjectDigest = Self::new([u8::MIN; 32]);
    pub const MAX: ObjectDigest = Self::new([u8::MAX; 32]);
    pub const OBJECT_DIGEST_DELETED_BYTE_VAL: u8 = 99;
    pub const OBJECT_DIGEST_WRAPPED_BYTE_VAL: u8 = 88;
    pub const OBJECT_DIGEST_CANCELLED_BYTE_VAL: u8 = 77;

    /// A marker that signifies the object is deleted.
    pub const OBJECT_DIGEST_DELETED: ObjectDigest =
        Self::new([Self::OBJECT_DIGEST_DELETED_BYTE_VAL; 32]);

    /// A marker that signifies the object is wrapped into another object.
    pub const OBJECT_DIGEST_WRAPPED: ObjectDigest =
        Self::new([Self::OBJECT_DIGEST_WRAPPED_BYTE_VAL; 32]);

    pub const OBJECT_DIGEST_CANCELLED: ObjectDigest =
        Self::new([Self::OBJECT_DIGEST_CANCELLED_BYTE_VAL; 32]);

    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }

    pub fn generate<R: rand::RngCore + rand::CryptoRng>(rng: R) -> Self {
        Self(Digest::generate(rng))
    }

    pub fn random() -> Self {
        Self(Digest::random())
    }

    pub const fn inner(&self) -> &[u8; 32] {
        self.0.inner()
    }

    pub const fn into_inner(self) -> [u8; 32] {
        self.0.into_inner()
    }

    pub fn is_alive(&self) -> bool {
        *self != Self::OBJECT_DIGEST_DELETED && *self != Self::OBJECT_DIGEST_WRAPPED
    }

    pub fn is_deleted(&self) -> bool {
        *self == Self::OBJECT_DIGEST_DELETED
    }

    pub fn is_wrapped(&self) -> bool {
        *self == Self::OBJECT_DIGEST_WRAPPED
    }

    pub fn base58_encode(&self) -> String {
        Base58::encode(self.0)
    }
}

impl AsRef<[u8]> for ObjectDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<[u8; 32]> for ObjectDigest {
    fn as_ref(&self) -> &[u8; 32] {
        self.0.as_ref()
    }
}

impl From<ObjectDigest> for [u8; 32] {
    fn from(digest: ObjectDigest) -> Self {
        digest.into_inner()
    }
}

impl From<[u8; 32]> for ObjectDigest {
    fn from(digest: [u8; 32]) -> Self {
        Self::new(digest)
    }
}

impl fmt::Display for ObjectDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for ObjectDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "o#{}", self.0)
    }
}

impl fmt::LowerHex for ObjectDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for ObjectDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl TryFrom<&[u8]> for ObjectDigest {
    type Error = crate::error::SuiError;

    fn try_from(bytes: &[u8]) -> Result<Self, crate::error::SuiError> {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| crate::error::SuiError::InvalidTransactionDigest)?;
        Ok(Self::new(arr))
    }
}

impl std::str::FromStr for ObjectDigest {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0; 32];
        let buffer = Base58::decode(s).map_err(|e| anyhow::anyhow!(e))?;
        if buffer.len() != 32 {
            return Err(anyhow::anyhow!("Invalid digest length. Expected 32 bytes"));
        }
        result.copy_from_slice(&buffer);
        Ok(ObjectDigest::new(result))
    }
}

/// A digest of a ZkLoginInputs, which commits to the signatures as well as the tx.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ZKLoginInputsDigest(Digest);

impl ZKLoginInputsDigest {
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ConsensusCommitDigest(Digest);

impl ConsensusCommitDigest {
    pub const ZERO: Self = Self(Digest::ZERO);

    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }

    pub const fn inner(&self) -> &[u8; 32] {
        self.0.inner()
    }

    pub const fn into_inner(self) -> [u8; 32] {
        self.0.into_inner()
    }

    pub fn random() -> Self {
        Self(Digest::random())
    }
}

impl Default for ConsensusCommitDigest {
    fn default() -> Self {
        Self::ZERO
    }
}

impl From<ConsensusCommitDigest> for [u8; 32] {
    fn from(digest: ConsensusCommitDigest) -> Self {
        digest.into_inner()
    }
}

impl From<[u8; 32]> for ConsensusCommitDigest {
    fn from(digest: [u8; 32]) -> Self {
        Self::new(digest)
    }
}

impl fmt::Display for ConsensusCommitDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for ConsensusCommitDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ConsensusCommitDigest")
            .field(&self.0)
            .finish()
    }
}

mod test {
    #[allow(unused_imports)]
    use crate::digests::ChainIdentifier;
    // check that the chain id returns mainnet
    #[test]
    fn test_chain_id_mainnet() {
        let chain_id = ChainIdentifier::from_chain_short_id(&String::from("35834a8a"));
        assert_eq!(
            chain_id.unwrap().chain(),
            sui_protocol_config::Chain::Mainnet
        );
    }

    #[test]
    fn test_chain_id_testnet() {
        let chain_id = ChainIdentifier::from_chain_short_id(&String::from("4c78adac"));
        assert_eq!(
            chain_id.unwrap().chain(),
            sui_protocol_config::Chain::Testnet
        );
    }

    #[test]
    fn test_chain_id_unknown() {
        let chain_id = ChainIdentifier::from_chain_short_id(&String::from("unknown"));
        assert_eq!(chain_id, None);
    }
}
