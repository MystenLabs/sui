// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Defines the [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
//! flavor-specific resolvers and stores no additional metadata in the lockfile.

use std::{collections::BTreeMap, iter::empty};

use serde::{Deserialize, Serialize};

use crate::schema::{
    Environment, EnvironmentID, EnvironmentName, PackageName, ReplacementDependency,
};

use super::MoveFlavor;

pub const DEFAULT_ENV_NAME: &str = "_test_env";
pub const DEFAULT_ENV_ID: &str = "_test_env_id";

pub fn default_environment() -> Environment {
    Environment::new(DEFAULT_ENV_NAME.to_string(), DEFAULT_ENV_ID.to_string())
}

/// The [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
/// flavor-specific resolvers and stores no additional metadata in the lockfile.
#[derive(Debug)]
pub struct Vanilla;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum VanillaDep {}

impl MoveFlavor for Vanilla {
    type PublishedMetadata = ();
    type PackageMetadata = ();
    type AddressInfo = ();

    fn name() -> String {
        "vanilla".to_string()
    }

    fn default_environments() -> BTreeMap<EnvironmentName, EnvironmentID> {
        let mut envs = BTreeMap::new();
        envs.insert(DEFAULT_ENV_NAME.to_string(), DEFAULT_ENV_ID.to_string());
        envs
    }

    fn implicit_deps(environment: EnvironmentID) -> BTreeMap<PackageName, ReplacementDependency> {
        empty().collect()
    }
}
