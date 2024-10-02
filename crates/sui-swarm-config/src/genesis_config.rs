// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, SocketAddr};

use anyhow::Result;
use fastcrypto::traits::KeyPair;
use rand::{rngs::StdRng, SeedableRng};
use serde::{Deserialize, Serialize};
use sui_config::genesis::{GenesisCeremonyParameters, TokenAllocation};
use sui_config::node::{DEFAULT_COMMISSION_RATE, DEFAULT_VALIDATOR_GAS_PRICE};
use sui_config::{local_ip_utils, Config};
use sui_genesis_builder::validator_info::{GenesisValidatorInfo, ValidatorInfo};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
    AuthorityPublicKeyBytes, NetworkKeyPair, NetworkPublicKey, PublicKey, SuiKeyPair,
};
use sui_types::multiaddr::Multiaddr;
use tracing::info;

// All information needed to build a NodeConfig for a state sync fullnode.
#[derive(Serialize, Deserialize, Debug)]
pub struct SsfnGenesisConfig {
    pub p2p_address: Multiaddr,
    pub network_key_pair: Option<NetworkKeyPair>,
}

// All information needed to build a NodeConfig for a validator.
#[derive(Serialize, Deserialize)]
pub struct ValidatorGenesisConfig {
    #[serde(default = "default_bls12381_key_pair")]
    pub key_pair: AuthorityKeyPair,
    #[serde(default = "default_ed25519_key_pair")]
    pub worker_key_pair: NetworkKeyPair,
    #[serde(default = "default_sui_key_pair")]
    pub account_key_pair: SuiKeyPair,
    #[serde(default = "default_ed25519_key_pair")]
    pub network_key_pair: NetworkKeyPair,
    pub network_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub p2p_listen_address: Option<SocketAddr>,
    #[serde(default = "default_socket_address")]
    pub metrics_address: SocketAddr,
    #[serde(default = "default_multiaddr_address")]
    pub narwhal_metrics_address: Multiaddr,
    pub gas_price: u64,
    pub commission_rate: u64,
    pub narwhal_primary_address: Multiaddr,
    pub narwhal_worker_address: Multiaddr,
    pub consensus_address: Multiaddr,
    #[serde(default = "default_stake")]
    pub stake: u64,
    pub name: Option<String>,
}

impl ValidatorGenesisConfig {
    pub fn to_validator_info(&self, name: String) -> GenesisValidatorInfo {
        let protocol_key: AuthorityPublicKeyBytes = self.key_pair.public().into();
        let account_key: PublicKey = self.account_key_pair.public();
        let network_key: NetworkPublicKey = self.network_key_pair.public().clone();
        let worker_key: NetworkPublicKey = self.worker_key_pair.public().clone();
        let network_address = self.network_address.clone();

        let info = ValidatorInfo {
            name,
            protocol_key,
            worker_key,
            network_key,
            account_address: SuiAddress::from(&account_key),
            gas_price: self.gas_price,
            commission_rate: self.commission_rate,
            network_address,
            p2p_address: self.p2p_address.clone(),
            narwhal_primary_address: self.narwhal_primary_address.clone(),
            narwhal_worker_address: self.narwhal_worker_address.clone(),
            description: String::new(),
            image_url: String::new(),
            project_url: String::new(),
        };
        let proof_of_possession =
            generate_proof_of_possession(&self.key_pair, (&self.account_key_pair.public()).into());
        GenesisValidatorInfo {
            info,
            proof_of_possession,
        }
    }

    /// Use validator public key as validator name.
    pub fn to_validator_info_with_random_name(&self) -> GenesisValidatorInfo {
        self.to_validator_info(self.key_pair.public().to_string())
    }
}

#[derive(Default)]
pub struct ValidatorGenesisConfigBuilder {
    protocol_key_pair: Option<AuthorityKeyPair>,
    account_key_pair: Option<AccountKeyPair>,
    ip: Option<String>,
    gas_price: Option<u64>,
    /// If set, the validator will use deterministic addresses based on the port offset.
    /// This is useful for benchmarking.
    port_offset: Option<u16>,
    /// Whether to use a specific p2p listen ip address. This is useful for testing on AWS.
    p2p_listen_ip_address: Option<IpAddr>,
}

impl ValidatorGenesisConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_protocol_key_pair(mut self, key_pair: AuthorityKeyPair) -> Self {
        self.protocol_key_pair = Some(key_pair);
        self
    }

    pub fn with_account_key_pair(mut self, key_pair: AccountKeyPair) -> Self {
        self.account_key_pair = Some(key_pair);
        self
    }

    pub fn with_ip(mut self, ip: String) -> Self {
        self.ip = Some(ip);
        self
    }

    pub fn with_gas_price(mut self, gas_price: u64) -> Self {
        self.gas_price = Some(gas_price);
        self
    }

    pub fn with_deterministic_ports(mut self, port_offset: u16) -> Self {
        self.port_offset = Some(port_offset);
        self
    }

    pub fn with_p2p_listen_ip_address(mut self, p2p_listen_ip_address: IpAddr) -> Self {
        self.p2p_listen_ip_address = Some(p2p_listen_ip_address);
        self
    }

    pub fn build<R: rand::RngCore + rand::CryptoRng>(self, rng: &mut R) -> ValidatorGenesisConfig {
        let ip = self.ip.unwrap_or_else(local_ip_utils::get_new_ip);
        let localhost = local_ip_utils::localhost_for_testing();

        let protocol_key_pair = self
            .protocol_key_pair
            .unwrap_or_else(|| get_key_pair_from_rng(rng).1);
        let account_key_pair = self
            .account_key_pair
            .unwrap_or_else(|| get_key_pair_from_rng(rng).1);
        let gas_price = self.gas_price.unwrap_or(DEFAULT_VALIDATOR_GAS_PRICE);

        let (worker_key_pair, network_key_pair): (NetworkKeyPair, NetworkKeyPair) =
            (get_key_pair_from_rng(rng).1, get_key_pair_from_rng(rng).1);

        let (
            network_address,
            p2p_address,
            metrics_address,
            narwhal_metrics_address,
            narwhal_primary_address,
            narwhal_worker_address,
            consensus_address,
        ) = if let Some(offset) = self.port_offset {
            (
                local_ip_utils::new_deterministic_tcp_address_for_testing(&ip, offset),
                local_ip_utils::new_deterministic_udp_address_for_testing(&ip, offset + 1),
                local_ip_utils::new_deterministic_tcp_address_for_testing(&ip, offset + 2)
                    .with_zero_ip(),
                local_ip_utils::new_deterministic_tcp_address_for_testing(&ip, offset + 3)
                    .with_zero_ip(),
                local_ip_utils::new_deterministic_udp_address_for_testing(&ip, offset + 4),
                local_ip_utils::new_deterministic_udp_address_for_testing(&ip, offset + 5),
                local_ip_utils::new_deterministic_tcp_address_for_testing(&ip, offset + 6),
            )
        } else {
            (
                local_ip_utils::new_tcp_address_for_testing(&ip),
                local_ip_utils::new_udp_address_for_testing(&ip),
                local_ip_utils::new_tcp_address_for_testing(&localhost),
                local_ip_utils::new_tcp_address_for_testing(&localhost),
                local_ip_utils::new_udp_address_for_testing(&ip),
                local_ip_utils::new_udp_address_for_testing(&ip),
                local_ip_utils::new_tcp_address_for_testing(&ip),
            )
        };

        let p2p_listen_address = self
            .p2p_listen_ip_address
            .map(|ip| SocketAddr::new(ip, p2p_address.port().unwrap()));

        ValidatorGenesisConfig {
            key_pair: protocol_key_pair,
            worker_key_pair,
            account_key_pair: account_key_pair.into(),
            network_key_pair,
            network_address,
            p2p_address,
            p2p_listen_address,
            metrics_address: metrics_address.to_socket_addr().unwrap(),
            narwhal_metrics_address,
            gas_price,
            commission_rate: DEFAULT_COMMISSION_RATE,
            narwhal_primary_address,
            narwhal_worker_address,
            consensus_address,
            stake: sui_types::governance::VALIDATOR_LOW_STAKE_THRESHOLD_MIST,
            name: None,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct GenesisConfig {
    pub ssfn_config_info: Option<Vec<SsfnGenesisConfig>>,
    pub validator_config_info: Option<Vec<ValidatorGenesisConfig>>,
    pub parameters: GenesisCeremonyParameters,
    pub accounts: Vec<AccountConfig>,
}

impl Config for GenesisConfig {}

impl GenesisConfig {
    pub fn generate_accounts<R: rand::RngCore + rand::CryptoRng>(
        &self,
        mut rng: R,
    ) -> Result<(Vec<AccountKeyPair>, Vec<TokenAllocation>)> {
        let mut addresses = Vec::new();
        let mut allocations = Vec::new();

        info!("Creating accounts and token allocations...");

        let mut keys = Vec::new();
        for account in &self.accounts {
            let address = if let Some(address) = account.address {
                address
            } else {
                let (address, keypair) = get_key_pair_from_rng(&mut rng);
                keys.push(keypair);
                address
            };

            addresses.push(address);

            // Populate gas itemized objects
            account.gas_amounts.iter().for_each(|a| {
                allocations.push(TokenAllocation {
                    recipient_address: address,
                    amount_mist: *a,
                    staked_with_validator: None,
                });
            });
        }

        Ok((keys, allocations))
    }
}

fn default_socket_address() -> SocketAddr {
    local_ip_utils::new_local_tcp_socket_for_testing()
}

fn default_multiaddr_address() -> Multiaddr {
    local_ip_utils::new_local_tcp_address_for_testing()
}

fn default_stake() -> u64 {
    sui_types::governance::VALIDATOR_LOW_STAKE_THRESHOLD_MIST
}

fn default_bls12381_key_pair() -> AuthorityKeyPair {
    get_key_pair_from_rng(&mut rand::rngs::OsRng).1
}

fn default_ed25519_key_pair() -> NetworkKeyPair {
    get_key_pair_from_rng(&mut rand::rngs::OsRng).1
}

fn default_sui_key_pair() -> SuiKeyPair {
    SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut rand::rngs::OsRng).1)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AccountConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<SuiAddress>,
    pub gas_amounts: Vec<u64>,
}

pub const DEFAULT_GAS_AMOUNT: u64 = 30_000_000_000_000_000;
pub const DEFAULT_NUMBER_OF_AUTHORITIES: usize = 4;
const DEFAULT_NUMBER_OF_ACCOUNT: usize = 5;
pub const DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT: usize = 5;

impl GenesisConfig {
    /// A predictable rng seed used to generate benchmark configs. This seed may also be needed
    /// by other crates (e.g. the load generators).
    pub const BENCHMARKS_RNG_SEED: u64 = 0;
    /// Port offset for benchmarks' genesis configs.
    pub const BENCHMARKS_PORT_OFFSET: u16 = 2000;
    /// The gas amount for each genesis gas object.
    const BENCHMARK_GAS_AMOUNT: u64 = 50_000_000_000_000_000;
    /// Trigger epoch change every hour minutes.
    const BENCHMARK_EPOCH_DURATION_MS: u64 = 60 * 60 * 1000;

    pub fn for_local_testing() -> Self {
        Self::custom_genesis(
            DEFAULT_NUMBER_OF_ACCOUNT,
            DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT,
        )
    }

    pub fn for_local_testing_with_addresses(addresses: Vec<SuiAddress>) -> Self {
        Self::custom_genesis_with_addresses(addresses, DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT)
    }

    pub fn custom_genesis(num_accounts: usize, num_objects_per_account: usize) -> Self {
        let mut accounts = Vec::new();
        for _ in 0..num_accounts {
            accounts.push(AccountConfig {
                address: None,
                gas_amounts: vec![DEFAULT_GAS_AMOUNT; num_objects_per_account],
            })
        }

        Self {
            accounts,
            ..Default::default()
        }
    }

    pub fn custom_genesis_with_addresses(
        addresses: Vec<SuiAddress>,
        num_objects_per_account: usize,
    ) -> Self {
        let mut accounts = Vec::new();
        for address in addresses {
            accounts.push(AccountConfig {
                address: Some(address),
                gas_amounts: vec![DEFAULT_GAS_AMOUNT; num_objects_per_account],
            })
        }

        Self {
            accounts,
            ..Default::default()
        }
    }

    /// Generate a genesis config allowing to easily bootstrap a network for benchmarking purposes. This
    /// function is ultimately used to print the genesis blob and all validators configs. All keys and
    /// parameters are predictable to facilitate benchmarks orchestration. Only the main ip addresses of
    /// the validators are specified (as those are often dictated by the cloud provider hosing the testbed).
    pub fn new_for_benchmarks(ips: &[String]) -> Self {
        // Set the validator's configs. They should be the same across multiple runs to ensure reproducibility.
        let mut rng = StdRng::seed_from_u64(Self::BENCHMARKS_RNG_SEED);
        let validator_config_info: Vec<_> = ips
            .iter()
            .enumerate()
            .map(|(i, ip)| {
                ValidatorGenesisConfigBuilder::new()
                    .with_ip(ip.to_string())
                    .with_deterministic_ports(Self::BENCHMARKS_PORT_OFFSET + 10 * i as u16)
                    .with_p2p_listen_ip_address("0.0.0.0".parse().unwrap())
                    .build(&mut rng)
            })
            .collect();

        // Set the initial gas objects with a predictable owner address.
        let account_configs = Self::benchmark_gas_keys(validator_config_info.len())
            .iter()
            .map(|gas_key| {
                let gas_address = SuiAddress::from(&gas_key.public());

                AccountConfig {
                    address: Some(gas_address),
                    // Generate one genesis gas object per validator (this seems a good rule of thumb to produce
                    // enough gas objects for most types of benchmarks).
                    gas_amounts: vec![Self::BENCHMARK_GAS_AMOUNT; 5],
                }
            })
            .collect();

        // Benchmarks require a deterministic genesis. Every validator locally generates it own
        // genesis; it is thus important they have the same parameters.
        let parameters = GenesisCeremonyParameters {
            chain_start_timestamp_ms: 0,
            epoch_duration_ms: Self::BENCHMARK_EPOCH_DURATION_MS,
            ..GenesisCeremonyParameters::new()
        };

        // Make a new genesis configuration.
        GenesisConfig {
            ssfn_config_info: None,
            validator_config_info: Some(validator_config_info),
            parameters,
            accounts: account_configs,
        }
    }

    /// Generate a predictable and fixed key that will own all gas objects used for benchmarks.
    /// This function may be called by other parts of the codebase (e.g. load generators) to
    /// get the same keypair used for genesis (hence the importance of the seedable rng).
    pub fn benchmark_gas_keys(n: usize) -> Vec<SuiKeyPair> {
        let mut rng = StdRng::seed_from_u64(Self::BENCHMARKS_RNG_SEED);
        (0..n)
            .map(|_| SuiKeyPair::Ed25519(NetworkKeyPair::generate(&mut rng)))
            .collect()
    }

    pub fn add_faucet_account(mut self) -> Self {
        self.accounts.push(AccountConfig {
            address: None,
            gas_amounts: vec![DEFAULT_GAS_AMOUNT; DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT],
        });
        self
    }
}
