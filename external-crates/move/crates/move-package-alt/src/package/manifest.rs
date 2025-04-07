// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::{Debug, Display, Formatter},
};

use derive_where::derive_where;
use serde::{Deserialize, Serialize};

use crate::{
    dependency::ManifestDependencyInfo,
    flavor::{MoveFlavor, Vanilla},
};

use super::*;

// Note: [Manifest] objects are immutable and should not implement [serde::Serialize]; any tool
// writing these files should use [toml_edit] to set / preserve the formatting, since these are
// user-editable files
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
#[serde(bound = "")]
pub struct Manifest<F: MoveFlavor> {
    package: PackageMetadata<F>,
    environments: BTreeMap<EnvironmentName, F::EnvironmentID>,
    #[serde(default)]
    dependencies: BTreeMap<PackageName, ManifestDependency<F>>,
    #[serde(default)]
    dep_overrides: BTreeMap<EnvironmentName, BTreeMap<PackageName, ManifestDependencyOverride<F>>>,
}

#[derive(Debug, Deserialize)]
#[serde(bound = "")]
struct PackageMetadata<F: MoveFlavor> {
    name: PackageName,
    edition: String,

    #[serde(flatten)]
    metadata: F::PackageMetadata,
}

#[derive(Deserialize, Debug)]
#[serde(bound = "")]
#[serde(rename_all = "kebab-case")]
struct ManifestDependency<F: MoveFlavor> {
    #[serde(flatten)]
    dependency_info: ManifestDependencyInfo<F>,

    #[serde(rename = "override", default)]
    is_override: bool,

    #[serde(default)]
    rename_from: Option<PackageName>,
}

#[derive(Debug, Deserialize)]
#[serde(bound = "")]
#[serde(rename_all = "kebab-case")]
struct ManifestDependencyOverride<F: MoveFlavor> {
    #[serde(flatten, default)]
    dependency: Option<ManifestDependency<F>>,

    #[serde(flatten, default)]
    address_info: Option<F::AddressInfo>,

    #[serde(default)]
    use_environment: Option<EnvironmentName>,
}

impl<F: MoveFlavor> Manifest<F> {
    pub fn read_from(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        Ok(toml_edit::de::from_str(&contents)?)
    }

    fn write_template(path: impl AsRef<Path>, name: &PackageName) -> anyhow::Result<()> {
        std::fs::write(
            path,
            r###"
            "###,
        )?;

        Ok(())
    }
}
