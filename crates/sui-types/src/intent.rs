use std::str::FromStr;

// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;
use serde_repr::Serialize_repr;

use crate::error::SuiError;

#[cfg(test)]
#[path = "unit_tests/intent_tests.rs"]
mod intent_tests;

#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum IntentVersion {
    V0 = 0,
}

#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ChainId {
    Testing = 0,
}

impl FromStr for ChainId {
    type Err = SuiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<ChainId>()
    }
}
pub trait SecureIntent: Serialize + private::SealedIntent {}

#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum IntentScope {
    TransactionData = 0,
    TransactionEffects = 1,
    AuthorityBatch = 2,
    CheckpointSummary = 3,
    PersonalMessage = 4,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
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

    pub fn default_with_scope(scope: IntentScope) -> Self {
        Self {
            version: IntentVersion::V0,
            chain_id: ChainId::Testing,
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
pub struct IntentMessage<'a, T> {
    intent: Intent,
    value: &'a T,
}

impl<'a, T> IntentMessage<'a, T> {
    pub fn new(intent: Intent, value: &'a T) -> Self {
        Self { intent, value }
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
    impl<'a, T> SealedIntent for IntentMessage<'a, T> {}
}
