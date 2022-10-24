// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::messages::TransactionData;
use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;
use serde_repr::Serialize_repr;

#[cfg(test)]
#[path = "unit_tests/intent_tests.rs"]
mod intent_tests;

#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum IntentVersion {
    V0 = 0,
}

#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum ChainId {
    Testing = 0,
}

impl Default for ChainId {
    fn default() -> Self {
        Self::Testing
    }
}
pub trait SecureIntent: Serialize + private::SealedIntent {}

#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum IntentScope {
    TransactionData = 0,
    TransactionEffects = 1,
    AuthorityBatch = 2,
    CheckpointSummary = 3,
    PersonalMessage = 4,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Hash)]
pub struct Intent {
    version: IntentVersion,
    chain_id: ChainId,
    scope: IntentScope,
}

impl Intent {
    pub fn new(version: IntentVersion, chain_id: ChainId, scope: IntentScope) -> Self {
        Self {
            version,
            chain_id,
            scope,
        }
    }

    pub fn with_chain_id(mut self, chain_id: ChainId) -> Self {
        self.chain_id = chain_id;
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
            chain_id: ChainId::Testing,
            scope: IntentScope::TransactionData,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct IntentMessage<T> {
    pub intent: Intent,
    pub value: T,
}

impl<T> IntentMessage<T> {
    pub fn new(intent: Intent, value: T) -> Self {
        Self { intent, value }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<IntentMessage<TransactionData>, anyhow::Error> {
        let intent: Intent = match bcs::from_bytes(&bytes[..3]) {
            Ok(intent) => intent,
            Err(e) => return Err(anyhow::anyhow!("Failed to parse Intent: {:?}", e)),
        };
        fp_ensure!(
            intent.scope == IntentScope::TransactionData,
            anyhow::anyhow!("Unsupported Intent version: {:?}", intent.scope)
        );
        match bcs::from_bytes(&bytes[3..]) {
            Ok(tx_data) => Ok(IntentMessage::new(intent, tx_data)),
            Err(e) => Err(anyhow::anyhow!("Failed to parse IntentMessage: {:?}", e)),
        }
    }
}

// --- PersonalMessage intent ---
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct PersonalMessage {
    pub message: Vec<u8>,
}

pub(crate) mod private {
    use super::IntentMessage;

    pub trait SealedIntent {}
    impl<T> SealedIntent for IntentMessage<T> {}
}
