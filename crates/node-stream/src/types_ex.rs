// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Debug, Display, Formatter},
    str::FromStr,
};
use thiserror::Error as Error1;

// ================= Consumer Errors ===========================

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Error1)]
pub enum NodeStreamConsumerError {
    #[error("UnableToCreteConsumer: {:?}", err)]
    UnableToCreateConsumer { err: kafka::Error },

    #[error("DuplicateSession: old: {}, new: {}", old, new)]
    DuplicateSession {
        old: NodeStreamSessionId,
        new: NodeStreamSessionId,
    },

    #[error(
        "PayloadDeserializeError: unable to deserialize payload for topic: {}, err: {}",
        topic,
        err
    )]
    PayloadDeserializeError { topic: NodeStreamTopic, err: String },

    #[error(
        "UnableToPollMessage: unable to poll for topic: {}, err: {:?}",
        topic,
        err
    )]
    UnableToPollMessage {
        topic: NodeStreamTopic,
        err: kafka::Error,
    },

    #[error(
        "UnableToMarkMessageConsumed: unable to mark message consumed for topic: {}, err: {:?}",
        topic,
        err
    )]
    UnableToMarkMessageConsumed {
        topic: NodeStreamTopic,
        err: kafka::Error,
    },
    #[error(
        "UnableToCommitMessageConsumed: unable to commit message consumed for topic: {}, err: {:?}",
        topic,
        err
    )]
    UnableToCommitMessageConsumed {
        topic: NodeStreamTopic,
        err: kafka::Error,
    },
}

// ================= Producer Errors ===========================

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Error1)]
pub enum NodeStreamProducerError {
    #[error("UnableToCreateProducer: {}", err)]
    UnableToCreateProducer { err: kafka::Error },

    #[error(
        "PayloadSerializeError: unable to serialize payload for topic: {}, err: {}",
        topic,
        err
    )]
    PayloadSerializeError { topic: NodeStreamTopic, err: String },

    #[error(
        "UnableToLoadAllInternalMetatada: unable to load internal metadata. err: {}",
        err
    )]
    UnableToLoadAllInternalMetatada { err: kafka::Error },

    #[error(
        "UnableToLoadTopicInternalMetatada: unable to load internal metadata for topic: {}, err: {}",       
        topic,
        err
    )]
    UnableToLoadTopicInternalMetatada {
        topic: NodeStreamTopic,
        err: kafka::Error,
    },

    #[error("PayloadTooLarge: payload too large. Limit: {} vs {}", limit, size)]
    PayloadTooLarge { limit: u64, size: u64 },

    #[error("MessageSendFailed: unable to send message, err: {}", err)]
    MessageSendFailed { err: kafka::Error },
}

// ================= Topic ===========================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStreamTopic {
    pub(crate) topic: String,
}

impl NodeStreamTopic {
    pub fn new(topic: String) -> NodeStreamTopic {
        NodeStreamTopic { topic }
    }
}

impl Display for NodeStreamTopic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.topic)
    }
}

impl NodeStreamTopic {
    pub fn to_raw(&self) -> String {
        self.topic.clone()
    }
}

pub trait NodeStreamPerEpochTopic<D: std::fmt::Debug, M: std::fmt::Debug> {
    type FromBytesError: std::fmt::Debug;
    type ToBytesError: std::fmt::Debug;

    fn topic_for_epoch(&self, epoch: u64) -> NodeStreamTopic;
    fn payload_from_bytes(
        &self,
        bytes: &[u8],
    ) -> Result<NodeStreamPayload<D, M>, Self::FromBytesError>;
    fn payload_to_bytes(
        &self,
        payload: &NodeStreamPayload<D, M>,
    ) -> Result<Vec<u8>, Self::ToBytesError>;
}

// ================= Session ===========================
const SESSION_ID_LENGTH: usize = 32;
#[derive(Eq, PartialEq, Clone, Copy, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeStreamSessionId([u8; SESSION_ID_LENGTH]);

impl NodeStreamSessionId {
    pub const LENGTH: usize = SESSION_ID_LENGTH;
    pub fn new() -> NodeStreamSessionId {
        let mut rng = rand::rngs::OsRng;
        let buf: [u8; Self::LENGTH] = rng.gen();
        Self(buf)
    }

    pub fn to_group_id(&self) -> String {
        format!("{}", self)
    }
}

impl FromStr for NodeStreamSessionId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = hex::decode(s)?;
        Self::try_from(&value[..]).map_err(|_| anyhow::anyhow!("byte deserialization failed"))
    }
}

impl TryFrom<&[u8]> for NodeStreamSessionId {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != Self::LENGTH {
            return Err(anyhow::anyhow!("invalid length"));
        }

        let mut buf = [0u8; Self::LENGTH];
        buf.copy_from_slice(value);
        Ok(Self(buf))
    }
}

impl Default for NodeStreamSessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::UpperHex for NodeStreamSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }

        for byte in &self.0 {
            write!(f, "{:02X}", byte)?;
        }

        Ok(())
    }
}

impl Display for NodeStreamSessionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02X}", self)
    }
}

impl Debug for NodeStreamSessionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02X}", self)
    }
}

// ================= Payload ===========================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStreamPayload<DataType: Debug, MetadataType: Debug> {
    // Metadata
    pub metdata: MetadataType,
    // Content
    pub data: DataType,
}

// // ================= Sui Specific ===========================
// pub struct SuiTxMetatada {
//     pub publish_timestamp_ms: u64,
//     pub checkpoint_id: u64,
//     pub sender: SuiAddress,
//     pub tx_digest: TransactionDigest,
// }
