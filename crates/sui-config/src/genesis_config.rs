// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Result;
use fastcrypto::traits::KeyPair;
use rand::{rngs::StdRng, SeedableRng};
use serde::{Deserialize, Serialize};
use sui_types::multiaddr::Multiaddr;
use tracing::info;

use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{
    get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, NetworkKeyPair, SuiKeyPair,
};

use crate::genesis::{GenesisCeremonyParameters, TokenAllocation};
use crate::node::{
    DEFAULT_COMMISSION_RATE, DEFAULT_GRPC_CONCURRENCY_LIMIT, DEFAULT_VALIDATOR_GAS_PRICE,
};
use crate::utils;
use crate::Config;

// All information needed to build a NodeConfig for a validator.
#[derive(Serialize, Deserialize)]
pub struct ValidatorConfigInfo {
    pub genesis_info: ValidatorGenesisInfo,
    pub consensus_address: Multiaddr,
    pub consensus_internal_worker_address: Option<Multiaddr>,
}

#[derive(Serialize, Deserialize)]
pub struct GenesisConfig {
    pub validator_config_info: Option<Vec<ValidatorConfigInfo>>,
    pub parameters: GenesisCeremonyParameters,
    pub committee_size: usize,
    pub grpc_load_shed: Option<bool>,
    pub grpc_concurrency_limit: Option<usize>,
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

#[derive(Serialize, Deserialize, Debug)]
pub struct ValidatorGenesisInfo {
    pub key_pair: AuthorityKeyPair,
    pub worker_key_pair: NetworkKeyPair,
    pub account_key_pair: SuiKeyPair,
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
    #[serde(default = "default_stake")]
    pub stake: u64,
}

fn default_socket_address() -> SocketAddr {
    utils::available_local_socket_address()
}

fn default_multiaddr_address() -> Multiaddr {
    let addr = utils::available_local_socket_address();
    format!("/ip4/{:?}/tcp/{}/http", addr.ip(), addr.port())
        .parse()
        .unwrap()
}

fn default_stake() -> u64 {
    sui_types::governance::VALIDATOR_LOW_STAKE_THRESHOLD_MIST
}

impl ValidatorGenesisInfo {
    pub const DEFAULT_NETWORK_PORT: u16 = 1000;
    pub const DEFAULT_P2P_PORT: u16 = 2000;
    pub const DEFAULT_P2P_LISTEN_PORT: u16 = 3000;
    pub const DEFAULT_METRICS_PORT: u16 = 4000;
    pub const DEFAULT_NARWHAL_METRICS_PORT: u16 = 5000;
    pub const DEFAULT_NARWHAL_PRIMARY_PORT: u16 = 6000;
    pub const DEFAULT_NARWHAL_WORKER_PORT: u16 = 7000;

    pub fn from_localhost_for_testing(
        key_pair: AuthorityKeyPair,
        worker_key_pair: NetworkKeyPair,
        account_key_pair: SuiKeyPair,
        network_key_pair: NetworkKeyPair,
        gas_price: u64,
    ) -> Self {
        Self {
            key_pair,
            worker_key_pair,
            account_key_pair,
            network_key_pair,
            network_address: utils::new_tcp_network_address(),
            p2p_address: utils::new_udp_network_address(),
            p2p_listen_address: None,
            metrics_address: utils::available_local_socket_address(),
            narwhal_metrics_address: utils::new_tcp_network_address(),
            gas_price,
            commission_rate: DEFAULT_COMMISSION_RATE,
            narwhal_primary_address: utils::new_udp_network_address(),
            narwhal_worker_address: utils::new_udp_network_address(),
            stake: sui_types::governance::VALIDATOR_LOW_STAKE_THRESHOLD_MIST,
        }
    }

    pub fn from_base_ip(
        key_pair: AuthorityKeyPair,
        worker_key_pair: NetworkKeyPair,
        account_key_pair: SuiKeyPair,
        network_key_pair: NetworkKeyPair,
        p2p_listen_address: Option<IpAddr>,
        ip: String,
        // Port offset allows running many SuiNodes inside the same simulator node, which is
        // helpful for tests that don't use Swarm.
        port_offset: usize,
        gas_price: u64,
    ) -> Self {
        assert!(port_offset < 1000);
        let port_offset: u16 = port_offset.try_into().unwrap();
        let make_tcp_addr =
            |port: u16| -> Multiaddr { format!("/ip4/{ip}/tcp/{port}/http").parse().unwrap() };
        let make_udp_addr =
            |port: u16| -> Multiaddr { format!("/ip4/{ip}/udp/{port}").parse().unwrap() };
        let make_tcp_zero_addr =
            |port: u16| -> Multiaddr { format!("/ip4/0.0.0.0/tcp/{port}/http").parse().unwrap() };

        ValidatorGenesisInfo {
            key_pair,
            worker_key_pair,
            account_key_pair,
            network_key_pair,
            network_address: make_tcp_addr(Self::DEFAULT_NETWORK_PORT + port_offset),
            p2p_address: make_udp_addr(Self::DEFAULT_P2P_PORT + port_offset),
            p2p_listen_address: p2p_listen_address
                .map(|x| SocketAddr::new(x, Self::DEFAULT_P2P_LISTEN_PORT + port_offset)),
            metrics_address: format!("0.0.0.0:{}", Self::DEFAULT_METRICS_PORT + port_offset)
                .parse()
                .unwrap(),
            narwhal_metrics_address: make_tcp_zero_addr(
                Self::DEFAULT_NARWHAL_METRICS_PORT + port_offset,
            ),
            gas_price,
            commission_rate: DEFAULT_COMMISSION_RATE,
            narwhal_primary_address: make_udp_addr(
                Self::DEFAULT_NARWHAL_PRIMARY_PORT + port_offset,
            ),
            narwhal_worker_address: make_udp_addr(Self::DEFAULT_NARWHAL_WORKER_PORT + port_offset),
            stake: sui_types::governance::VALIDATOR_LOW_STAKE_THRESHOLD_MIST,
        }
    }
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
    pub const BENCHMARKS_PORT_OFFSET: usize = 500;

    pub fn for_local_testing() -> Self {
        Self::custom_genesis(
            DEFAULT_NUMBER_OF_AUTHORITIES,
            DEFAULT_NUMBER_OF_ACCOUNT,
            DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT,
        )
    }

    pub fn for_local_testing_with_addresses(addresses: Vec<SuiAddress>) -> Self {
        Self::custom_genesis_with_addresses(
            DEFAULT_NUMBER_OF_AUTHORITIES,
            addresses,
            DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT,
        )
    }

    pub fn custom_genesis(
        num_authorities: usize,
        num_accounts: usize,
        num_objects_per_account: usize,
    ) -> Self {
        assert!(
            num_authorities > 0,
            "num_authorities should be larger than 0"
        );

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
        num_authorities: usize,
        addresses: Vec<SuiAddress>,
        num_objects_per_account: usize,
    ) -> Self {
        assert!(
            num_authorities > 0,
            "num_authorities should be larger than 0"
        );

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
        // Set the validator's configs.
        let mut rng = StdRng::seed_from_u64(Self::BENCHMARKS_RNG_SEED);
        let validator_config_info: Vec<_> = ips
            .iter()
            .map(|ip| {
                ValidatorConfigInfo {
                    consensus_address: "/ip4/127.0.0.1/tcp/8083/http".parse().unwrap(),
                    consensus_internal_worker_address: None,
                    genesis_info: ValidatorGenesisInfo::from_base_ip(
                        AuthorityKeyPair::generate(&mut rng), // key_pair
                        NetworkKeyPair::generate(&mut rng),   // worker_key_pair
                        SuiKeyPair::Ed25519(NetworkKeyPair::generate(&mut rng)), // account_key_pair
                        NetworkKeyPair::generate(&mut rng),   // network_key_pair
                        Some(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))), // p2p_listen_address
                        ip.to_string(),
                        Self::BENCHMARKS_PORT_OFFSET,
                        DEFAULT_VALIDATOR_GAS_PRICE,
                    ),
                }
            })
            .collect();

        // Make a predictable address that will own all gas objects.
        let gas_key = Self::benchmark_gas_key();
        let gas_address = SuiAddress::from(&gas_key.public());

        // Set the initial gas objects.
        let account_config = AccountConfig {
            address: Some(gas_address),
            // Generate one genesis gas object per validator (this seems a good rule of thumb to produce
            // enough gas objects for most types of benchmarks).
            gas_amounts: vec![DEFAULT_GAS_AMOUNT; validator_config_info.len()],
        };

        // Benchmarks require a deterministic genesis. Every validator locally generates it own
        // genesis; it is thus important they have the same parameters.
        let parameters = GenesisCeremonyParameters {
            chain_start_timestamp_ms: 0,
            ..GenesisCeremonyParameters::new()
        };

        // Make a new genesis configuration.
        GenesisConfig {
            validator_config_info: Some(validator_config_info),
            parameters,
            committee_size: ips.len(),
            grpc_load_shed: None,
            grpc_concurrency_limit: None,
            accounts: vec![account_config],
        }
    }

    /// Generate a predictable and fixed key that will own all gas objects used for benchmarks.
    /// This function may be called by other parts of the codebase (e.g. load generators) to
    /// get the same keypair used for genesis (hence the importance of the seedable rng).
    pub fn benchmark_gas_key() -> SuiKeyPair {
        let mut rng = StdRng::seed_from_u64(Self::BENCHMARKS_RNG_SEED);
        SuiKeyPair::Ed25519(NetworkKeyPair::generate(&mut rng))
    }

    /// Generate several predictable and fixed gas object id offsets for benchmarks. Load generators
    /// and other benchmark facilities may also need to retrieve these id offsets (hence the importance
    /// of the seedable rng).
    pub fn benchmark_gas_object_id_offsets(quantity: usize) -> Vec<ObjectID> {
        let mut rng = StdRng::seed_from_u64(Self::BENCHMARKS_RNG_SEED);
        (0..quantity)
            .map(|_| ObjectID::random_from_rng(&mut rng))
            .collect()
    }
}

impl Default for GenesisConfig {
    fn default() -> Self {
        Self {
            validator_config_info: None,
            parameters: Default::default(),
            committee_size: 0, // default value indicates the field is not set
            grpc_load_shed: None,
            grpc_concurrency_limit: Some(DEFAULT_GRPC_CONCURRENCY_LIMIT),
            accounts: vec![],
        }
    }
}
