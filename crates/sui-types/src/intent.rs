// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
pub enum AppId {
    Sui = 0,
}

impl Default for AppId {
    fn default() -> Self {
        Self::Sui
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
    pub scope: IntentScope,
    pub version: IntentVersion,
    pub app_id: AppId,
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
