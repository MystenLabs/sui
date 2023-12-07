// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::crypto::{BridgeAuthoritySignInfo, BridgeAuthorityPublicKey, BridgeAuthoritySignature};
use crate::{abi::EthBridgeEvent, events::SuiBridgeEvent};
use ethers::types::Address as EthAddress;
use fastcrypto::hash::HashFunction;
use fastcrypto::hash::Keccak256;
use serde::{Deserialize, Serialize};
use shared_crypto::intent::IntentScope;
use std::collections::{BTreeMap, HashMap};
use sui_types::digests::Digest;
use sui_types::error::SuiResult;
use sui_types::message_envelope::{Envelope, Message, VerifiedEnvelope};
use sui_types::multiaddr::Multiaddr;
use sui_types::{base_types::SUI_ADDRESS_LENGTH, committee::EpochId};


#[derive(Debug, Eq, PartialEq, Clone)]
pub struct BridgeAuthority {
    pub pubkey: BridgeAuthorityPublicKey,
    pub voting_power: u64,
    pub bridge_network_address: Multiaddr,
    pub is_blocklisted: bool,
}

// A static Bridge committee implementation
#[derive(Debug)]
pub struct BridgeCommittee {
    pub members: BTreeMap<BridgeAuthorityPublicKey, BridgeAuthority>,
}

impl BridgeCommittee {
    pub fn is_active_member(&self, member: &BridgeAuthorityPublicKey) -> bool {
        self.members.contains_key(&member) && !self.members.get(&member).unwrap().is_blocklisted
    }
}

#[derive(Copy, Clone)]
pub enum BridgeEventType {
    TokenTransfer = 0,
    UpdateCommitteeBlocklist = 1,
    EmergencyButton = 2,
}

pub const SUI_TX_DIGEST_LENGTH: usize = 32;
pub const ETH_TX_HASH_LENGTH: usize = 32;

pub const BRIDGE_MESSAGE_PREFIX: &[u8] = b"SUI_NATIVE_BRIDGE";

#[derive(Copy, Clone)]
pub enum BridgeChainId {
    Sui = 0,
    Eth = 1,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenId {
    Sui = 0,
    BTC = 1,
    ETH = 2,
    USDC = 3,
    USDT = 4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeEvent {
    Sui(SuiBridgeEvent),
    Eth(EthBridgeEvent),
}

impl BridgeEvent {
    /// Convert to message bytes that are verified in Move and Solidity
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add prefix
        bytes.extend_from_slice(BRIDGE_MESSAGE_PREFIX);
        match self {
            BridgeEvent::Sui(SuiBridgeEvent::SuiToEthTokenBridgeV1(e)) => {
                // Add message type
                bytes.push(BridgeEventType::TokenTransfer as u8);
                // Add message version
                bytes.push(1u8);
                // Add nonce
                bytes.extend_from_slice(&e.nonce.to_le_bytes());
                // Add source chain id
                bytes.push(BridgeChainId::Sui as u8);
                // Add source tx id length
                bytes.push(SUI_TX_DIGEST_LENGTH as u8);
                // Add source tx id
                bytes.extend_from_slice(e.sui_tx_digest.as_ref());
                // Add source tx event index
                bytes.push(e.sui_tx_event_index as u8);

                // Add source address length
                bytes.push(SUI_ADDRESS_LENGTH as u8);
                // Add source address
                bytes.extend_from_slice(&e.sui_address.to_vec());
                // Add dest chain id
                bytes.push(BridgeChainId::Eth as u8);
                // Add dest address length
                bytes.push(EthAddress::len_bytes() as u8);
                // Add dest address
                bytes.extend_from_slice(&e.eth_address.as_bytes());

                // Add token id
                bytes.push(e.token_id as u8);

                // Add token amount
                bytes.extend_from_slice(&e.amount.to_le_bytes());
            }
            BridgeEvent::Eth(_e) =>
            // TODO add formats for other events
            {
                unimplemented!()
            }
        }
        bytes
    }

    pub fn keccak256_hash(&self) -> [u8; 32] {
        let mut hash_function = Keccak256::default();
        let bytes = self.to_bytes();
        hash_function.update(&bytes);
        hash_function.finalize().into()
    }
}

pub struct BridgeCommitteeValiditySignInfo {
    pub signatures: HashMap<BridgeAuthorityPublicKey, BridgeAuthoritySignature>,
}

pub type SignedBridgeEvent = Envelope<BridgeEvent, BridgeAuthoritySignInfo>;
pub type VerifiedSignedBridgeEvent = VerifiedEnvelope<BridgeEvent, BridgeAuthoritySignInfo>;


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BridgeEventDigest(Digest);

impl BridgeEventDigest {
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }
}

impl Message for BridgeEvent {
    type DigestType = BridgeEventDigest;

    // this is not used today
    const SCOPE: IntentScope = IntentScope::BridgeEvent;

    // this is not used today
    fn digest(&self) -> Self::DigestType {
        BridgeEventDigest::new(self.keccak256_hash())
    }

    fn verify_user_input(&self) -> SuiResult {
        Ok(())
    }

    fn verify_epoch(&self, _epoch: EpochId) -> SuiResult {
        Ok(())
    }
}
