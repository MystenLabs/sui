// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use std::path::PathBuf;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    object::Object,
};

#[derive(Clone, Debug)]
pub struct InitialSeeds {
    /// Specific accounts to track ownership for
    pub tracked_accounts: Vec<SuiAddress>,

    /// Package IDs to include (beyond system packages which are always included)
    pub additional_packages: Vec<ObjectID>,

    /// Specific objects to pre-fetch
    pub seed_objects: Vec<ObjectID>,
}

impl Default for InitialSeeds {
    fn default() -> Self {
        Self {
            tracked_accounts: Vec::new(),
            additional_packages: Vec::new(),
            seed_objects: Vec::new(),
        }
    }
}

impl InitialSeeds {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_accounts(mut self, accounts: Vec<SuiAddress>) -> Self {
        self.tracked_accounts = accounts;
        self
    }

    pub fn with_packages(mut self, packages: Vec<ObjectID>) -> Self {
        self.additional_packages = packages;
        self
    }

    pub fn with_objects(mut self, objects: Vec<ObjectID>) -> Self {
        self.seed_objects = objects;
        self
    }
}

/// Configuration for the network to fork from
#[derive(Clone, Debug)]
pub enum Network {
    Mainnet,
    Testnet,
    Devnet,
    Custom(String),
}

impl Network {
    pub fn graphql_url(&self) -> String {
        match self {
            Network::Mainnet => "https://sui-mainnet.mystenlabs.com/graphql".to_string(),
            Network::Testnet => "https://sui-testnet.mystenlabs.com/graphql".to_string(),
            Network::Devnet => "https://sui-devnet.mystenlabs.com/graphql".to_string(),
            Network::Custom(url) => url.clone(),
        }
    }
}

impl std::str::FromStr for Network {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "mainnet" => Ok(Network::Mainnet),
            "testnet" => Ok(Network::Testnet),
            "devnet" => Ok(Network::Devnet),
            _ => Ok(Network::Custom(s.to_string())),
        }
    }
}

/// Loader for fetching initial seed data from GraphQL RPC
pub struct SeedLoader {
    graphql_url: String,
    client: reqwest::Client,
}

impl SeedLoader {
    pub fn new(graphql_url: String) -> Self {
        Self {
            graphql_url,
            client: reqwest::Client::new(),
        }
    }

    /// Load all seeds into the checkpoint ingestion directory
    /// This writes checkpoint files that will be consumed by sui-indexer-alt
    pub async fn load_seeds(
        &self,
        ingestion_dir: &PathBuf,
        checkpoint: u64,
        seeds: &InitialSeeds,
    ) -> Result<()> {
        tracing::info!(
            "Loading initial seeds at checkpoint {} into {}",
            checkpoint,
            ingestion_dir.display()
        );

        std::fs::create_dir_all(ingestion_dir)
            .context("Failed to create ingestion directory")?;

        // TODO: Implement actual seed loading
        // 1. Fetch system packages (0x1, 0x2, 0x3) at checkpoint
        // 2. Fetch tracked account objects
        // 3. Fetch additional packages
        // 4. Fetch specific objects
        // 5. Write checkpoint data to ingestion directory

        tracing::info!("Seed loading complete");
        Ok(())
    }

    async fn fetch_system_packages(&self, checkpoint: u64) -> Result<Vec<Object>> {
        // Fetch 0x1 (Move stdlib), 0x2 (Sui framework), 0x3 (Sui system)
        todo!("Implement GraphQL queries for system packages")
    }

    async fn fetch_owned_objects(
        &self,
        account: SuiAddress,
        checkpoint: u64,
    ) -> Result<Vec<Object>> {
        // Query GraphQL for objects owned by account at checkpoint
        todo!("Implement GraphQL query for owned objects")
    }

    async fn fetch_package(&self, package_id: ObjectID, checkpoint: u64) -> Result<Object> {
        // Fetch package by ID at checkpoint
        todo!("Implement GraphQL query for package")
    }

    async fn fetch_object(&self, object_id: ObjectID, checkpoint: u64) -> Result<Object> {
        // Fetch object by ID at checkpoint
        todo!("Implement GraphQL query for object")
    }
}
