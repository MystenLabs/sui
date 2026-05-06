// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Defines the [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
//! flavor-specific resolvers and stores no additional metadata in the lockfile.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::schema::{
    Environment, EnvironmentID, EnvironmentName, LockfileDependencyInfo, OriginalID, PackageName,
    ParsedManifest, ReplacementDependency,
};

use async_trait::async_trait;

use super::MoveFlavor;
use indexmap::IndexMap;

pub const DEFAULT_ENV_NAME: &str = "_test_env";
pub const DEFAULT_ENV_ID: &str = "_test_env_id";

/// The [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
/// flavor-specific resolvers and stores no additional metadata in the lockfile.
#[derive(Debug, Default)]
pub struct Vanilla;

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
}

impl Vanilla {
    pub fn default_environment() -> Environment {
        Environment::new(DEFAULT_ENV_NAME.to_string(), DEFAULT_ENV_ID.to_string())
    }
}
