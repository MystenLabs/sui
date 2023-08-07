// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use fastcrypto::traits::ToFromBytes;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    verify_proof_of_possession, AuthorityPublicKey, AuthorityPublicKeyBytes, AuthoritySignature,
    NetworkPublicKey,
};
use sui_types::multiaddr::Multiaddr;

const MAX_VALIDATOR_METADATA_LENGTH: usize = 256;

/// Publicly known information about a validator
#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ValidatorInfo {
    pub name: String,
    pub account_address: SuiAddress,
    pub protocol_key: AuthorityPublicKeyBytes,
    pub worker_key: NetworkPublicKey,
    pub network_key: NetworkPublicKey,
    pub gas_price: u64,
    pub commission_rate: u64,
    pub network_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub narwhal_primary_address: Multiaddr,
    pub narwhal_worker_address: Multiaddr,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
}

impl ValidatorInfo {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sui_address(&self) -> SuiAddress {
        self.account_address
    }

    pub fn protocol_key(&self) -> AuthorityPublicKeyBytes {
        self.protocol_key
    }

    pub fn worker_key(&self) -> &NetworkPublicKey {
        &self.worker_key
    }

    pub fn network_key(&self) -> &NetworkPublicKey {
        &self.network_key
    }

    pub fn gas_price(&self) -> u64 {
        self.gas_price
    }

    pub fn commission_rate(&self) -> u64 {
        self.commission_rate
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }

    pub fn narwhal_primary_address(&self) -> &Multiaddr {
        &self.narwhal_primary_address
    }

    pub fn narwhal_worker_address(&self) -> &Multiaddr {
        &self.narwhal_worker_address
    }

    pub fn p2p_address(&self) -> &Multiaddr {
        &self.p2p_address
    }
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenesisValidatorInfo {
    pub info: ValidatorInfo,
    pub proof_of_possession: AuthoritySignature,
}

impl GenesisValidatorInfo {
    pub fn validate(&self) -> anyhow::Result<(), anyhow::Error> {
        if !self.info.name.is_ascii() {
            bail!("name must be ascii");
        }
        if self.info.name.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("name must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if !self.info.description.is_ascii() {
            bail!("description must be ascii");
        }
        if self.info.description.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("description must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if self.info.image_url.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("image url must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if self.info.project_url.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("project url must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if !self.info.network_address.to_string().is_ascii() {
            bail!("network address must be ascii");
        }
        if self.info.network_address.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("network address must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if !self.info.p2p_address.to_string().is_ascii() {
            bail!("p2p address must be ascii");
        }
        if self.info.p2p_address.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("p2p address must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if !self.info.narwhal_primary_address.to_string().is_ascii() {
            bail!("primary address must be ascii");
        }
        if self.info.narwhal_primary_address.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("primary address must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if !self.info.narwhal_worker_address.to_string().is_ascii() {
            bail!("worker address must be ascii");
        }
        if self.info.narwhal_worker_address.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("worker address must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if let Err(e) = self.info.p2p_address.to_anemo_address() {
            bail!("p2p address must be valid anemo address: {e}");
        }
        if let Err(e) = self.info.narwhal_primary_address.to_anemo_address() {
            bail!("primary address must be valid anemo address: {e}");
        }
        if let Err(e) = self.info.narwhal_worker_address.to_anemo_address() {
            bail!("worker address must be valid anemo address: {e}");
        }

        if self.info.commission_rate > 10000 {
            bail!("commissions rate must be lower than 100%");
        }

        let protocol_pubkey = AuthorityPublicKey::from_bytes(self.info.protocol_key.as_ref())?;
        if let Err(e) = verify_proof_of_possession(
            &self.proof_of_possession,
            &protocol_pubkey,
            self.info.account_address,
        ) {
            bail!("proof of possession is incorrect: {e}");
        }

        Ok(())
    }
}

impl From<GenesisValidatorInfo> for GenesisValidatorMetadata {
    fn from(
        GenesisValidatorInfo {
            info,
            proof_of_possession,
        }: GenesisValidatorInfo,
    ) -> Self {
        Self {
            name: info.name,
            description: info.description,
            image_url: info.image_url,
            project_url: info.project_url,
            sui_address: info.account_address,
            gas_price: info.gas_price,
            commission_rate: info.commission_rate,
            protocol_public_key: info.protocol_key.as_bytes().to_vec(),
            proof_of_possession: proof_of_possession.as_ref().to_vec(),
            network_public_key: info.network_key.as_bytes().to_vec(),
            worker_public_key: info.worker_key.as_bytes().to_vec(),
            network_address: info.network_address,
            p2p_address: info.p2p_address,
            primary_address: info.narwhal_primary_address,
            worker_address: info.narwhal_worker_address,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GenesisValidatorMetadata {
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,

    pub sui_address: SuiAddress,

    pub gas_price: u64,
    pub commission_rate: u64,

    pub protocol_public_key: Vec<u8>, //AuthorityPublicKeyBytes,
    pub proof_of_possession: Vec<u8>, // AuthoritySignature,

    pub network_public_key: Vec<u8>, // NetworkPublicKey,
    pub worker_public_key: Vec<u8>,  // NetworkPublicKey,

    pub network_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub primary_address: Multiaddr,
    pub worker_address: Multiaddr,
}
