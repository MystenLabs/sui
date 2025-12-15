// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Defines the [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
//! flavor-specific resolvers and stores no additional metadata in the lockfile.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::schema::{
    Environment, EnvironmentID, EnvironmentName, LockfileDependencyInfo, PackageName,
    ParsedManifest, ReplacementDependency,
};

use super::MoveFlavor;
use indexmap::IndexMap;

pub const DEFAULT_ENV_NAME: &str = "_test_env";
pub const DEFAULT_ENV_ID: &str = "_test_env_id";

pub fn default_environment() -> Environment {
    Environment::new(DEFAULT_ENV_NAME.to_string(), DEFAULT_ENV_ID.to_string())
}

/// The [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
/// flavor-specific resolvers and stores no additional metadata in the lockfile.
#[derive(Debug)]
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

impl MoveFlavor for Vanilla {
    type PublishedMetadata = PublishedMetadata;
    type PackageMetadata = ();
    type AddressInfo = ();

    fn name() -> String {
        "vanilla".to_string()
    }

    fn default_environments() -> IndexMap<EnvironmentName, EnvironmentID> {
        let mut envs = IndexMap::new();
        envs.insert(DEFAULT_ENV_NAME.to_string(), DEFAULT_ENV_ID.to_string());
        envs
    }

    fn system_deps(_environment: &EnvironmentID) -> BTreeMap<String, LockfileDependencyInfo> {
        BTreeMap::new()
    }

    fn implicit_dependencies(
        _environment: &EnvironmentID,
    ) -> BTreeMap<PackageName, ReplacementDependency> {
        BTreeMap::new()
    }

    fn validate_manifest(_: &ParsedManifest) -> Result<(), String> {
        Ok(())
    }
}
