// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Defines the [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
//! flavor-specific resolvers and stores no additional metadata in the lockfile.

use std::collections::BTreeMap;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::schema::{
    Environment, EnvironmentID, EnvironmentName, LockfileDependencyInfo, OriginalID, PackageName,
    ParsedManifest, PublishedID, ReplacementDependency,
};

use async_trait::async_trait;

use super::{MoveFlavor, OnChainPackageData};
use indexmap::IndexMap;

pub const DEFAULT_ENV_NAME: &str = "_test_env";
pub const DEFAULT_ENV_ID: &str = "_test_env_id";

/// The [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
/// flavor-specific resolvers and stores no additional metadata in the lockfile.
///
/// On-chain packages can be pre-populated via [`Vanilla::with_on_chain_package`].
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Vanilla {
    on_chain_packages: BTreeMap<PublishedID, OnChainPackageData>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub struct PublishedMetadata {
    #[serde(default)]
    build_config: Option<SavedBuildConfig>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedBuildConfig {
    edition: String,
    flavor: String,
}

#[async_trait]
impl MoveFlavor for Vanilla {
    type PublishedMetadata = PublishedMetadata;
    type PackageMetadata = ();
    type AddressInfo = ();

    fn name(&self) -> String {
        "vanilla".to_string()
    }

    fn default_environments(&self) -> IndexMap<EnvironmentName, EnvironmentID> {
        let mut envs = IndexMap::new();
        envs.insert(DEFAULT_ENV_NAME.to_string(), DEFAULT_ENV_ID.to_string());
        envs
    }

    async fn system_deps(
        &self,
        _environment: &EnvironmentID,
    ) -> BTreeMap<String, LockfileDependencyInfo> {
        BTreeMap::new()
    }

    async fn implicit_dependencies(
        &self,
        _environment: &EnvironmentID,
    ) -> BTreeMap<PackageName, ReplacementDependency> {
        BTreeMap::new()
    }

    fn validate_manifest(&self, _: &ParsedManifest) -> Result<(), String> {
        Ok(())
    }

    fn is_system_address(&self, address: &crate::schema::OriginalID) -> bool {
        address == &OriginalID::from(0xBEEF)
    }

    async fn fetch_onchain_package(
        &self,
        address: &PublishedID,
    ) -> anyhow::Result<OnChainPackageData> {
        self.on_chain_packages
            .get(address)
            .cloned()
            .with_context(|| format!("on-chain package not found: {address}"))
    }
}

impl Vanilla {
    /// Create a new `Vanilla` flavor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a pre-populated on-chain package. Returns `self` for chaining.
    pub fn with_on_chain_package(mut self, address: PublishedID, data: OnChainPackageData) -> Self {
        self.on_chain_packages.insert(address, data);
        self
    }

    pub fn default_environment() -> Environment {
        Environment::new(DEFAULT_ENV_NAME.to_string(), DEFAULT_ENV_ID.to_string())
    }
}
