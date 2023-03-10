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
        match value {
            0 => Ok(Self::V0),
            _ => Err(eyre!("Invalid IntentVersion")),
        }
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
}

// TODO(joyqvq): Use num_derive
impl TryFrom<u8> for AppId {
    type Error = eyre::Report;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Sui),
            _ => Err(eyre!("Invalid AppId")),
        }
    }
}

impl Default for AppId {
    fn default() -> Self {
        Self::Sui
    }
}

#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum IntentScope {
    TransactionData = 0,
    TransactionEffects = 1,
    CheckpointSummary = 2,
    PersonalMessage = 3,
    SenderSignedTransaction = 4,
    ProofOfPossession = 5,
    HeaderDigest = 6,
}

impl TryFrom<u8> for IntentScope {
    type Error = eyre::Report;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::TransactionData),
            1 => Ok(Self::TransactionEffects),
            2 => Ok(Self::CheckpointSummary),
            3 => Ok(Self::PersonalMessage),
            4 => Ok(Self::SenderSignedTransaction),
            5 => Ok(Self::ProofOfPossession),
            _ => Err(eyre!("Invalid IntentScope")),
        }
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

impl FromStr for Intent {
    type Err = eyre::Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s: Vec<u8> = decode_bytes_hex(s).map_err(|_| eyre!("Invalid Intent"))?;
        if s.len() != 3 {
            return Err(eyre!("Invalid Intent"));
        }
        Ok(Self {
            scope: s[0].try_into()?,
            version: s[1].try_into()?,
            app_id: s[2].try_into()?,
        })
    }
}

impl Intent {
    pub fn with_app_id(mut self, app_id: AppId) -> Self {
        self.app_id = app_id;
        self
    }

    pub fn with_scope(mut self, scope: IntentScope) -> Self {
        self.scope = scope;
        self
    }
}

impl Default for Intent {
    fn default() -> Self {
        Self {
            version: IntentVersion::V0,
            scope: IntentScope::TransactionData,
            app_id: AppId::Sui,
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
