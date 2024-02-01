// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use consensus_config::{NetworkKeySignature, NetworkPublicKey};
use fastcrypto::error::FastCryptoError;
use fastcrypto::traits::{Signer, VerifyingKey};
use serde::Serialize;

use crate::block::BlockDigest;
use shared_crypto::intent::{Intent, IntentMessage, IntentScope};

/// An interface to facilitate the signing of messages using the intent mechanism.
pub trait AuthoritySignature<T: Serialize> {
    /// Create a new signature over an intent message.
    fn new(value: &IntentMessage<T>, secret: &dyn Signer<Self>) -> Self;

    /// Verify the signature on an intent message against the public key.
    fn verify(
        &self,
        value: &IntentMessage<T>,
        public_key: &NetworkPublicKey,
    ) -> Result<(), FastCryptoError>;
}

impl AuthoritySignature<BlockDigest> for NetworkKeySignature {
    fn new(value: &IntentMessage<BlockDigest>, secret: &dyn Signer<Self>) -> Self {
        let message = bcs::to_bytes(&value).expect("Message serialization should not fail");
        secret.sign(&message)
    }

    fn verify(
        &self,
        value: &IntentMessage<BlockDigest>,
        public_key: &NetworkPublicKey,
    ) -> Result<(), FastCryptoError> {
        let message = bcs::to_bytes(&value).expect("Message serialization should not fail");
        public_key.verify(&message, self)
    }
}

/// Wrap a message in an intent message. Currently in Consensus, the scope is always IntentScope::ConsensusBlock and the app id is AppId::Consensus.
pub fn to_consensus_block_intent(value: BlockDigest) -> IntentMessage<BlockDigest> {
    IntentMessage::new(Intent::consensus_app(IntentScope::ConsensusBlock), value)
}
