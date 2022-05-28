// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::genesis;
use crate::Config;
use debug_ignore::DebugIgnore;
use multiaddr::Multiaddr;
use narwhal_config::Committee as ConsensusCommittee;
use narwhal_config::Parameters as ConsensusParameters;
use narwhal_crypto::ed25519::Ed25519PublicKey;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use sui_types::base_types::SuiAddress;
use sui_types::committee::{Committee, EpochId};
use sui_types::crypto::{KeyPair, PublicKeyBytes};

#[derive(Debug, Deserialize, Serialize)]
pub struct NodeConfig {
    pub key_pair: KeyPair,
    pub db_path: PathBuf,
    pub network_address: Multiaddr,
    pub metrics_address: Multiaddr,
    pub json_rpc_address: SocketAddr,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus_config: Option<ConsensusConfig>,
    pub committee_config: CommitteeConfig,

    pub genesis: genesis::Genesis,
}

impl Config for NodeConfig {}

impl NodeConfig {
    pub fn key_pair(&self) -> &KeyPair {
        &self.key_pair
    }

    pub fn public_key(&self) -> PublicKeyBytes {
        *self.key_pair.public_key_bytes()
    }

    pub fn sui_address(&self) -> SuiAddress {
        SuiAddress::from(self.public_key())
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }

    pub fn consensus_config(&self) -> Option<&ConsensusConfig> {
        self.consensus_config.as_ref()
    }

    pub fn committee_config(&self) -> &CommitteeConfig {
        &self.committee_config
    }

    pub fn genesis(&self) -> &genesis::Genesis {
        &self.genesis
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConsensusConfig {
    pub consensus_address: Multiaddr,
    pub consensus_db_path: PathBuf,

    //TODO make narwhal config serializable
    #[serde(skip_serializing)]
    #[serde(default)]
    pub narwhal_config: DebugIgnore<ConsensusParameters>,

    pub narwhal_committee: DebugIgnore<ConsensusCommittee<Ed25519PublicKey>>,
}

impl ConsensusConfig {
    pub fn address(&self) -> &Multiaddr {
        &self.consensus_address
    }

    pub fn db_path(&self) -> &Path {
        &self.consensus_db_path
    }

    pub fn narwhal_config(&self) -> &ConsensusParameters {
        &self.narwhal_config
    }

    pub fn narwhal_committee(&self) -> &ConsensusCommittee<Ed25519PublicKey> {
        &self.narwhal_committee
    }
}

//TODO get this information from on-chain + some way to do network discovery
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommitteeConfig {
    pub epoch: EpochId,
    pub validator_set: Vec<ValidatorInfo>,
}

impl CommitteeConfig {
    pub fn epoch(&self) -> EpochId {
        self.epoch
    }

    pub fn validator_set(&self) -> &[ValidatorInfo] {
        &self.validator_set
    }

    pub fn committee(&self) -> Committee {
        let voting_rights = self
            .validator_set()
            .iter()
            .map(|validator| (validator.public_key(), validator.stake()))
            .collect();
        Committee::new(self.epoch(), voting_rights)
    }
}

/// Publicly known information about a validator
/// TODO read most of this from on-chain
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ValidatorInfo {
    pub public_key: PublicKeyBytes,
    pub stake: usize,
    pub network_address: Multiaddr,
}

impl ValidatorInfo {
    pub fn sui_address(&self) -> SuiAddress {
        SuiAddress::from(self.public_key())
    }

    pub fn public_key(&self) -> PublicKeyBytes {
        self.public_key
    }

    pub fn stake(&self) -> usize {
        self.stake
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }
}
