// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use fastcrypto::secp256k1::{Secp256k1PublicKey, Secp256k1Signature};
use sui_types::multiaddr::Multiaddr;

pub type BridgeCommitteePublicKey = Secp256k1PublicKey;
pub type BridgeCommitteeSignature = Secp256k1Signature;

#[derive(Debug, Eq, PartialEq)]
pub struct CommitteeMember {
    pub pubkey: BridgeCommitteePublicKey,
    pub voting_power: u64,
    pub bridge_network_address: Multiaddr,
    pub is_blocklisted: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub struct BridgeCommittee {
    pub members: Vec<CommitteeMember>,
}

pub struct BridgeCommitteeValiditySignInfo {
    pub signatures: HashMap<BridgeCommitteePublicKey, BridgeCommitteeSignature>,
}
