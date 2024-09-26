// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use eyre::eyre;
use fastcrypto::encoding::decode_bytes_hex;
use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;
use serde_repr::Serialize_repr;
use std::str::FromStr;

pub const INTENT_PREFIX_LENGTH: usize = 3;

/// The version here is to distinguish between signing different versions of the struct
/// or enum. Serialized output between two different versions of the same struct/enum
/// might accidentally (or maliciously on purpose) match.
#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum IntentVersion {
    V0 = 0,
}

impl TryFrom<u8> for IntentVersion {
    type Error = eyre::Report;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        bcs::from_bytes(&[value]).map_err(|_| eyre!("Invalid IntentVersion"))
    }
}

/// This enums specifies the application ID. Two intents in two different applications
/// (i.e., Narwhal, Sui, Ethereum etc) should never collide, so that even when a signing
/// key is reused, nobody can take a signature designated for app_1 and present it as a
/// valid signature for an (any) intent in app_2.
#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum AppId {
    Sui = 0,
    Narwhal = 1,
    Consensus = 2,
}

// TODO(joyqvq): Use num_derive
impl TryFrom<u8> for AppId {
    type Error = eyre::Report;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        bcs::from_bytes(&[value]).map_err(|_| eyre!("Invalid AppId"))
    }
}

impl Default for AppId {
    fn default() -> Self {
        Self::Sui
    }
}

/// This enums specifies the intent scope. Two intents for different scope should
/// never collide, so no signature provided for one intent scope can be used for
/// another, even when the serialized data itself may be the same.
#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum IntentScope {
    TransactionData = 0,         // Used for a user signature on a transaction data.
    TransactionEffects = 1,      // Used for an authority signature on transaction effects.
    CheckpointSummary = 2,       // Used for an authority signature on a checkpoint summary.
    PersonalMessage = 3,         // Used for a user signature on a personal message.
    SenderSignedTransaction = 4, // Used for an authority signature on a user signed transaction.
    ProofOfPossession = 5, // Used as a signature representing an authority's proof of possession of its authority protocol key.
    HeaderDigest = 6,      // Used for narwhal authority signature on header digest.
    BridgeEventUnused = 7, // for bridge purposes but it's currently not included in messages.
    ConsensusBlock = 8,    // Used for consensus authority signature on block's digest.
    DiscoveryPeers = 9,    // Used for reporting peer addresses in discovery.
}

impl TryFrom<u8> for IntentScope {
    type Error = eyre::Report;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        bcs::from_bytes(&[value]).map_err(|_| eyre!("Invalid IntentScope"))
    }
}

/// An intent is a compact struct serves as the domain separator for a message that a signature commits to.
/// It consists of three parts: [enum IntentScope] (what the type of the message is), [enum IntentVersion], [enum AppId] (what application that the signature refers to).
/// It is used to construct [struct IntentMessage] that what a signature commits to.
///
/// The serialization of an Intent is a 3-byte array where each field is represented by a byte.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Hash)]
pub struct Intent {
    pub scope: IntentScope,
    pub version: IntentVersion,
    pub app_id: AppId,
}

impl Intent {
    pub fn to_bytes(&self) -> [u8; INTENT_PREFIX_LENGTH] {
        [self.scope as u8, self.version as u8, self.app_id as u8]
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, eyre::Report> {
        if bytes.len() != INTENT_PREFIX_LENGTH {
            return Err(eyre!("Invalid Intent"));
        }
        Ok(Self {
            scope: bytes[0].try_into()?,
            version: bytes[1].try_into()?,
            app_id: bytes[2].try_into()?,
        })
    }
}

impl FromStr for Intent {
    type Err = eyre::Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes: Vec<u8> = decode_bytes_hex(s).map_err(|_| eyre!("Invalid Intent"))?;
        Self::from_bytes(bytes.as_slice())
    }
}

impl Intent {
    pub fn sui_app(scope: IntentScope) -> Self {
        Self {
            version: IntentVersion::V0,
            scope,
            app_id: AppId::Sui,
        }
    }

    pub fn sui_transaction() -> Self {
        Self {
            scope: IntentScope::TransactionData,
            version: IntentVersion::V0,
            app_id: AppId::Sui,
        }
    }

    pub fn personal_message() -> Self {
        Self {
            scope: IntentScope::PersonalMessage,
            version: IntentVersion::V0,
            app_id: AppId::Sui,
        }
    }

    pub fn narwhal_app(scope: IntentScope) -> Self {
        Self {
            scope,
            version: IntentVersion::V0,
            app_id: AppId::Narwhal,
        }
    }

    pub fn consensus_app(scope: IntentScope) -> Self {
        Self {
            scope,
            version: IntentVersion::V0,
            app_id: AppId::Consensus,
        }
    }
}

/// Intent Message is a wrapper around a message with its intent. The message can
/// be any type that implements [trait Serialize]. *ALL* signatures in Sui must commits
/// to the intent message, not the message itself. This guarantees any intent
/// message signed in the system cannot collide with another since they are domain
/// separated by intent.
///
/// The serialization of an IntentMessage is compact: it only appends three bytes
/// to the message itself.
#[derive(Debug, PartialEq, Eq, Serialize, Clone, Hash, Deserialize)]
pub struct IntentMessage<T> {
    pub intent: Intent,
    pub value: T,
}

impl<T> IntentMessage<T> {
    pub fn new(intent: Intent, value: T) -> Self {
        Self { intent, value }
    }
}

/// A person message that wraps around a byte array.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct PersonalMessage {
    pub message: Vec<u8>,
}

pub trait SecureIntent: Serialize + private::SealedIntent {}

pub(crate) mod private {
    use super::IntentMessage;

    pub trait SealedIntent {}
    impl<T> SealedIntent for IntentMessage<T> {}
}

/// A 1-byte domain separator for hashing Object ID in Sui. It is starting from 0xf0
/// to ensure no hashing collision for any ObjectID vs SuiAddress which is derived
/// as the hash of `flag || pubkey`. See `sui_types::crypto::SignatureScheme::flag()`.
#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum HashingIntentScope {
    ChildObjectId = 0xf0,
    RegularObjectId = 0xf1,
}
