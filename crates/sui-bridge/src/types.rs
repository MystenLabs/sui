// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::ExampleContractEvents;
use crate::crypto::BridgeAuthorityPublicKeyBytes;
use crate::crypto::{BridgeAuthorityPublicKey, BridgeAuthoritySignInfo, BridgeAuthoritySignature};
use crate::error::{BridgeError, BridgeResult};
use crate::events::EmittedSuiToEthTokenBridgeV1;
use ethers::core::rand::Rng;
use ethers::types::Address as EthAddress;
pub use ethers::types::H256 as EthTransactionHash;
use serde::{Deserialize, Serialize};
use shared_crypto::intent::IntentScope;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use sui_types::committee::CommitteeTrait;
use sui_types::committee::StakeUnit;
use sui_types::digests::{Digest, TransactionDigest};
use sui_types::error::SuiResult;
use sui_types::message_envelope::{Envelope, Message, VerifiedEnvelope};
use sui_types::multiaddr::Multiaddr;
use sui_types::{base_types::SUI_ADDRESS_LENGTH, committee::EpochId};

pub const BRIDGE_AUTHORITY_TOTAL_VOTING_POWER: u64 = 10000;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct BridgeAuthority {
    pub pubkey: BridgeAuthorityPublicKey,
    pub voting_power: u64,
    pub bridge_network_address: Multiaddr,
    pub is_blocklisted: bool,
}

impl BridgeAuthority {
    pub fn pubkey_bytes(&self) -> BridgeAuthorityPublicKeyBytes {
        BridgeAuthorityPublicKeyBytes::from(&self.pubkey)
    }
}

// A static Bridge committee implementation
#[derive(Debug, Clone)]
pub struct BridgeCommittee {
    members: BTreeMap<BridgeAuthorityPublicKeyBytes, BridgeAuthority>,
}

impl BridgeCommittee {
    pub fn new(members: Vec<BridgeAuthority>) -> BridgeResult<Self> {
        let mut members_map = BTreeMap::new();
        let mut total_stake = 0;
        for member in members {
            let public_key = BridgeAuthorityPublicKeyBytes::from(&member.pubkey);
            if members_map.contains_key(&public_key) {
                return Err(BridgeError::InvalidBridgeCommittee(
                    "Duplicate BridgeAuthority Public key".into(),
                ));
            }
            // TODO: should we disallow identical network addresses?
            total_stake += member.voting_power;
            members_map.insert(public_key, member);
        }
        if total_stake != BRIDGE_AUTHORITY_TOTAL_VOTING_POWER {
            return Err(BridgeError::InvalidBridgeCommittee(
                "Total voting power does not equal to 10000".into(),
            ));
        }
        Ok(Self {
            members: members_map,
        })
    }

    pub fn is_active_member(&self, member: &BridgeAuthorityPublicKeyBytes) -> bool {
        self.members.contains_key(member) && !self.members.get(member).unwrap().is_blocklisted
    }

    pub fn members(&self) -> &BTreeMap<BridgeAuthorityPublicKeyBytes, BridgeAuthority> {
        &self.members
    }
}

impl CommitteeTrait<BridgeAuthorityPublicKeyBytes> for BridgeCommittee {
    // Note this function does not shuffle today.
    fn shuffle_by_stake_with_rng(
        &self,
        // try these authorities first
        _preferences: Option<&BTreeSet<BridgeAuthorityPublicKeyBytes>>,
        // only attempt from these authorities.
        restrict_to: Option<&BTreeSet<BridgeAuthorityPublicKeyBytes>>,
        _rng: &mut impl Rng,
    ) -> Vec<BridgeAuthorityPublicKeyBytes> {
        // TODO does BridgeCommittee need shuffling?

        self.members
            .keys()
            .filter(|name| {
                if let Some(restrict_to) = restrict_to {
                    restrict_to.contains(name)
                } else {
                    true
                }
            })
            .cloned()
            .collect()
    }

    fn weight(&self, author: &BridgeAuthorityPublicKeyBytes) -> StakeUnit {
        self.members
            .get(author)
            .map(|a| a.voting_power)
            .unwrap_or(0)
    }
}

#[derive(Copy, Clone)]
#[repr(u8)]
pub enum BridgeActionType {
    TokenTransfer = 0,
    UpdateCommitteeBlocklist = 1,
    EmergencyButton = 2,
}

pub const SUI_TX_DIGEST_LENGTH: usize = 32;
pub const ETH_TX_HASH_LENGTH: usize = 32;

pub const BRIDGE_MESSAGE_PREFIX: &[u8] = b"SUI_NATIVE_BRIDGE";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum BridgeChainId {
    SuiMainnet = 0,
    SuiTestnet = 1,
    SuiDevnet = 2,

    EthMainnet = 10,
    EthSepolia = 11,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TokenId {
    Sui = 0,
    BTC = 1,
    ETH = 2,
    USDC = 3,
    USDT = 4,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuiToEthBridgeAction {
    // Digest of the transaction where the event was emitted
    pub sui_tx_digest: TransactionDigest,
    // The index of the event in the transaction
    pub sui_tx_event_index: u16,
    pub sui_bridge_event: EmittedSuiToEthTokenBridgeV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EthToSuiBridgeAction {
    // Digest of the transaction where the event was emitted
    pub eth_tx_hash: EthTransactionHash,
    // The index of the event in the transaction
    pub eth_event_index: u16,
    // TODO placeholder
    pub eth_bridge_event: ExampleContractEvents,
}

/// The type of actions Bridge Committee verify and sign off to execution.
/// Its relationship with BridgeEvent is similar to the relationship between
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BridgeAction {
    /// Sui to Eth bridge action
    SuiToEthBridgeAction(SuiToEthBridgeAction),
    /// Eth to sui bridge action
    EthToSuiBridgeAction(EthToSuiBridgeAction),
    // TODO: add other bridge actions such as blocklist & emergency button
}

pub const TOKEN_TRANSFER_MESSAGE_VERSION: u8 = 1;

impl BridgeAction {
    /// Convert to message bytes that are verified in Move and Solidity
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add prefix
        bytes.extend_from_slice(BRIDGE_MESSAGE_PREFIX);
        match self {
            BridgeAction::SuiToEthBridgeAction(a) => {
                let e = &a.sui_bridge_event;
                // Add message type
                bytes.push(BridgeActionType::TokenTransfer as u8);
                // Add message version
                bytes.push(TOKEN_TRANSFER_MESSAGE_VERSION);
                // Add nonce
                bytes.extend_from_slice(&e.nonce.to_le_bytes());
                // Add source chain id
                bytes.push(BridgeChainId::SuiTestnet as u8);
                // Add source tx id length
                bytes.push(SUI_TX_DIGEST_LENGTH as u8);
                // Add source tx id
                bytes.extend_from_slice(a.sui_tx_digest.as_ref());
                // Add source tx event index
                bytes.extend_from_slice(&a.sui_tx_event_index.to_le_bytes());

                // Add source address length
                bytes.push(SUI_ADDRESS_LENGTH as u8);
                // Add source address
                bytes.extend_from_slice(&e.sui_address.to_vec());
                // Add dest chain id
                bytes.push(BridgeChainId::EthSepolia as u8);
                // Add dest address length
                bytes.push(EthAddress::len_bytes() as u8);
                // Add dest address
                bytes.extend_from_slice(e.eth_address.as_bytes());

                // Add token id
                bytes.push(e.token_id as u8);

                // Add token amount
                bytes.extend_from_slice(&e.amount.to_le_bytes());
            }
            BridgeAction::EthToSuiBridgeAction(_e) =>
            // TODO add formats for other events
            {
                unimplemented!()
            }
        }
        bytes
    }
}

pub struct BridgeCommitteeValiditySignInfo {
    pub signatures: HashMap<BridgeAuthorityPublicKey, BridgeAuthoritySignature>,
}

pub type SignedBridgeAction = Envelope<BridgeAction, BridgeAuthoritySignInfo>;
pub type VerifiedSignedBridgeAction = VerifiedEnvelope<BridgeAction, BridgeAuthoritySignInfo>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BridgeEventDigest(Digest);

impl BridgeEventDigest {
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }
}

impl Message for BridgeAction {
    type DigestType = BridgeEventDigest;

    // this is not encoded in message today
    const SCOPE: IntentScope = IntentScope::BridgeEventUnused;

    // this is not used today
    fn digest(&self) -> Self::DigestType {
        unreachable!("BridgeEventDigest is not used today")
    }

    fn verify_user_input(&self) -> SuiResult {
        Ok(())
    }

    fn verify_epoch(&self, _epoch: EpochId) -> SuiResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::types::TokenId;
    use ethers::types::Address as EthAddress;
    use fastcrypto::traits::KeyPair;
    use prometheus::Registry;
    use sui_types::{
        base_types::{SuiAddress, TransactionDigest},
        crypto::get_key_pair,
    };

    use super::*;

    #[test]
    fn test_bridge_message_encoding() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let nonce = 54321u64;
        let sui_tx_digest = TransactionDigest::random();
        let sui_chain_id = BridgeChainId::SuiTestnet;
        let sui_tx_event_index = 1u16;
        let eth_chain_id = BridgeChainId::EthSepolia;
        let sui_address = SuiAddress::random_for_testing_only();
        let eth_address = EthAddress::random();
        let token_id = TokenId::USDC;
        let amount = 1_000_000u128;

        let sui_bridge_event = EmittedSuiToEthTokenBridgeV1 {
            nonce,
            sui_chain_id,
            eth_chain_id,
            sui_address,
            eth_address,
            token_id,
            amount,
        };

        let encoded_bytes = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest,
            sui_tx_event_index,
            sui_bridge_event,
        })
        .to_bytes();

        // Construct the expected bytes
        let prefix_bytes = BRIDGE_MESSAGE_PREFIX.to_vec(); // len: 17
        let message_type = vec![BridgeActionType::TokenTransfer as u8]; // len: 1
        let message_version = vec![TOKEN_TRANSFER_MESSAGE_VERSION]; // len: 1
        let nonce_bytes = nonce.to_le_bytes().to_vec(); // len: 8
        let source_chain_id_bytes = vec![sui_chain_id as u8]; // len: 1
        let source_tx_digest_length_bytes = vec![SUI_TX_DIGEST_LENGTH as u8]; // len: 1
        let source_tx_digest_bytes = sui_tx_digest.inner().to_vec(); // len: 32
        let source_event_index_bytes = sui_tx_event_index.to_le_bytes().to_vec(); // len: 2

        let sui_address_length_bytes = vec![SUI_ADDRESS_LENGTH as u8]; // len: 1
        let sui_address_bytes = sui_address.to_vec(); // len: 32
        let dest_chain_id_bytes = vec![eth_chain_id as u8]; // len: 1
        let eth_address_length_bytes = vec![EthAddress::len_bytes() as u8]; // len: 1
        let eth_address_bytes = eth_address.as_bytes().to_vec(); // len: 20

        let token_id_bytes = vec![token_id as u8]; // len: 1
        let token_amount_bytes = amount.to_le_bytes().to_vec(); // len: 16

        let mut combined_bytes = Vec::new();
        combined_bytes.extend_from_slice(&prefix_bytes);
        combined_bytes.extend_from_slice(&message_type);
        combined_bytes.extend_from_slice(&message_version);
        combined_bytes.extend_from_slice(&nonce_bytes);
        combined_bytes.extend_from_slice(&source_chain_id_bytes);
        combined_bytes.extend_from_slice(&source_tx_digest_length_bytes);
        combined_bytes.extend_from_slice(&source_tx_digest_bytes);
        combined_bytes.extend_from_slice(&source_event_index_bytes);
        combined_bytes.extend_from_slice(&sui_address_length_bytes);
        combined_bytes.extend_from_slice(&sui_address_bytes);
        combined_bytes.extend_from_slice(&dest_chain_id_bytes);
        combined_bytes.extend_from_slice(&eth_address_length_bytes);
        combined_bytes.extend_from_slice(&eth_address_bytes);
        combined_bytes.extend_from_slice(&token_id_bytes);
        combined_bytes.extend_from_slice(&token_amount_bytes);

        assert_eq!(combined_bytes, encoded_bytes);

        // Assert fixed length
        // TODO: for each action type add a test to assert the length
        assert_eq!(
            combined_bytes.len(),
            17 + 1 + 1 + 8 + 1 + 1 + 32 + 2 + 1 + 32 + 1 + 20 + 1 + 1 + 16
        );

        Ok(())
    }

    #[test]
    fn test_bridge_committee_construction() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let (_, kp): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
        let pubkey = kp.public().clone();
        let mut authority = BridgeAuthority {
            pubkey: pubkey.clone(),
            voting_power: 10000,
            bridge_network_address: Multiaddr::try_from("/ip4/127.0.0.1/tcp/9999/http".to_string())
                .unwrap(),
            is_blocklisted: false,
        };
        // This is ok
        let _ = BridgeCommittee::new(vec![authority.clone()]).unwrap();

        // This is not ok - total voting power != 10000
        authority.voting_power = 9999;
        let _ = BridgeCommittee::new(vec![authority.clone()]).unwrap_err();

        // This is not ok - total voting power != 10000
        authority.voting_power = 10001;
        let _ = BridgeCommittee::new(vec![authority.clone()]).unwrap_err();

        // This is ok
        authority.voting_power = 5000;
        let mut authority_2 = authority.clone();
        let (_, kp): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
        let pubkey = kp.public().clone();
        authority_2.pubkey = pubkey.clone();
        let _ = BridgeCommittee::new(vec![authority.clone(), authority_2.clone()]).unwrap();

        // This is not ok - duplicate pub key
        authority_2.pubkey = authority.pubkey.clone();
        let _ = BridgeCommittee::new(vec![authority.clone(), authority.clone()]).unwrap_err();
        Ok(())
    }
}
