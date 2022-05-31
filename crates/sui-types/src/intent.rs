// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::batch::AuthorityBatch;
use crate::messages::{TransactionData, TransactionEffects};
use crate::messages_checkpoint::CheckpointSummary;

use serde::{Deserialize, Serialize};

/// Current Sui version (for compatibility purposes).
/// TODO: Increment value when a non-backwards compatible Sui version is launched (i.e. when
/// changing the format, signature or field ordering of signable types).
const SUI_COMPATIBILITY_VERSION: u8 = 0;
const SUI_CHAIN_ID: u8 = 0;

/// This is the only type we should sign per our serialization-handbook to provide domain separation
/// and avoid accidental serialized-bytes collisions between structs and intents, different versions
/// of the same struct and chains (mainnet, testnet etc).
/// Similarly, `SecureIntent` should be preferred when hashing requires domain separation
/// guarantees.
pub trait SecureIntent: Serialize + serde::de::DeserializeOwned + private::SealedIntent {}

/// Struct required when Sui repository-version and chainID are required for domain
/// separation (i.e. when something is sign-able).
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct VersionPlusChainId {
    version: u8,
    chain_id: u8,
}

impl Default for VersionPlusChainId {
    // TODO: read these values from some config, genesis or epoch.
    fn default() -> Self {
        VersionPlusChainId {
            version: SUI_COMPATIBILITY_VERSION,
            chain_id: SUI_CHAIN_ID,
        }
    }
}

/// `IntentScope` is required to guarantee two different intents will never collide.
pub enum IntentScope {
    TransactionData,
    TransactionEffects,
    AuthorityBatch,
    CheckpointSummary,
    PersonalMessage,
}

impl IntentScope {
    /// Specifically assign a byte per enum element, to avoid accidental issues (i.e., field
    /// swapping, adding in the middle or deletion) which would affect enum's BCS serialization.
    ///
    /// IMPORTANT NOTE: we should ensure forward and backward uniqueness of scope-values. That said,
    /// if a value has been used in the past, then it should NEVER be reassigned to a different
    /// scope.
    pub fn value(&self) -> u8 {
        match *self {
            IntentScope::TransactionData => 0,
            IntentScope::TransactionEffects => 1,
            IntentScope::AuthorityBatch => 2,
            IntentScope::CheckpointSummary => 3,
            IntentScope::PersonalMessage => 4,
        }
    }
}

// --- TransactionData intent ---

/// The intent we should use for TransactionData signing.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct TransactionDataIntent {
    version_plus_chain_id: VersionPlusChainId,
    scope: u8,
    signable: TransactionData,
}

impl From<&TransactionData> for TransactionDataIntent {
    fn from(signable: &TransactionData) -> Self {
        Self {
            version_plus_chain_id: Default::default(),
            scope: IntentScope::TransactionData.value(),
            signable: signable.clone(),
        }
    }
}

impl SecureIntent for TransactionDataIntent {}

// --- TransactionEffects intent ---

/// The intent we should use for TransactionEffects signing.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct TransactionEffectsIntent {
    version_plus_chain_id: VersionPlusChainId,
    scope: u8,
    signable: TransactionEffects,
}

impl From<&TransactionEffects> for TransactionEffectsIntent {
    fn from(signable: &TransactionEffects) -> Self {
        Self {
            version_plus_chain_id: Default::default(),
            scope: IntentScope::TransactionEffects.value(),
            signable: signable.clone(),
        }
    }
}

impl SecureIntent for TransactionEffectsIntent {}

// --- AuthorityBatch intent ---

/// The intent we should use for AuthorityBatch signing.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct AuthorityBatchIntent {
    version_plus_chain_id: VersionPlusChainId,
    scope: u8,
    signable: AuthorityBatch,
}

impl From<&AuthorityBatch> for AuthorityBatchIntent {
    fn from(signable: &AuthorityBatch) -> Self {
        Self {
            version_plus_chain_id: Default::default(),
            scope: IntentScope::AuthorityBatch.value(),
            signable: signable.clone(),
        }
    }
}

impl SecureIntent for AuthorityBatchIntent {}

// --- CheckpointSummary intent ---

/// The intent we should use for CheckpointSummary signing.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CheckpointSummaryIntent {
    version_plus_chain_id: VersionPlusChainId,
    scope: u8,
    signable: CheckpointSummary,
}

impl From<&CheckpointSummary> for CheckpointSummaryIntent {
    fn from(signable: &CheckpointSummary) -> Self {
        Self {
            version_plus_chain_id: Default::default(),
            scope: IntentScope::CheckpointSummary.value(),
            signable: signable.clone(),
        }
    }
}

impl SecureIntent for CheckpointSummaryIntent {}

// --- PersonalMessage intent ---

/// The intent we should use for signing personal messages (similarly to Ethereum's personal_sign).
/// TODO: consider removing this outside this file when personal sign will be utilized in practice.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct PersonalMessage {
    pub message: Vec<u8>,
}

/// The intent we should use for PersonalMessage signing.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct PersonalMessageIntent {
    version_plus_chain_id: VersionPlusChainId,
    scope: u8,
    signable: PersonalMessage,
}

impl From<&PersonalMessage> for PersonalMessageIntent {
    fn from(signable: &PersonalMessage) -> Self {
        Self {
            version_plus_chain_id: Default::default(),
            scope: IntentScope::PersonalMessage.value(),
            signable: signable.clone(),
        }
    }
}

impl SecureIntent for PersonalMessageIntent {}

// --- Define the sealed intents ---

/// A pub(crate) mod hiding a SealedIntent trait and its implementations, allowing
/// us to make sure implementations are constrained to the sui_types crate.
/// See <https://rust-lang.github.io/api-guidelines/future-proofing.html#sealed-traits-protect-against-downstream-implementations-c-sealed>
pub(crate) mod private {
    use crate::intent::{
        AuthorityBatchIntent, CheckpointSummaryIntent, PersonalMessageIntent,
        TransactionDataIntent, TransactionEffectsIntent,
    };

    pub trait SealedIntent {}

    impl SealedIntent for TransactionDataIntent {}
    impl SealedIntent for TransactionEffectsIntent {}
    impl SealedIntent for AuthorityBatchIntent {}
    impl SealedIntent for CheckpointSummaryIntent {}
    impl SealedIntent for PersonalMessageIntent {}
}

#[test]
fn test_personal_message_intent() {
    use crate::crypto::{get_key_pair, Signature};

    let (addr1, sec1) = get_key_pair();

    let message = "Hello".as_bytes().to_vec();
    let p_message = PersonalMessage { message };
    let p_message_bcs = bcs::to_bytes(&p_message).unwrap();

    let intent: PersonalMessageIntent = (&p_message).into();
    let intent_bcs = bcs::to_bytes(&intent).unwrap();

    // Check that the intent length adds up an extra 3 bytes to the original p_message.
    assert_eq!(intent_bcs.len(), p_message_bcs.len() + 3);

    // Check that the first 3 bytes are the domain separation information.
    assert_eq!(
        &intent_bcs[..3],
        vec![
            SUI_COMPATIBILITY_VERSION,
            SUI_CHAIN_ID,
            IntentScope::PersonalMessage.value()
        ]
    );

    // Check that intent's last bytes match the p_message's bsc bytes.
    assert_eq!(&intent_bcs[3..], &p_message_bcs);

    // Let's ensure we can sign and verify intents.
    let s = Signature::new_secure(&intent, &sec1);
    let verification = s.verify_secure(&intent, addr1);
    assert!(verification.is_ok())
}
