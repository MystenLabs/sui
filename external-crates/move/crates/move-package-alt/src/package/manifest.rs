/*
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Deserialize)]
#[serde(rename = "kebab-case")]
pub struct Manifest {
    package: PackageMetadata,
    environments: BTreeMap<EnvironmentName, ChainID>,
    dependencies: BTreeMap<PackageName, ManifestDependency>,
    dep_overrides: BTreeMap<EnvironmentName, BTreeMap<PackageName, ManifestDependencyOverride>>,
}

#[derive(Deserialize)]
struct PackageMetadata {
    name: PackageName,
    // TODO
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ManifestDependency {
    resolver: String,

    #[serde(rename = "override", default)]
    is_override: bool,

    rename_from: Option<PackageName>,

    #[serde(flatten)]
    fields: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ManifestDependencyOverride {
    #[serde(flatten)]
    dependency: Option<ManifestDependency>,

    #[serde(flatten)]
    address_info: Option<AddressInfo>,

    use_environment: Option<EnvironmentName>,
}

impl Manifest {
    fn read_from(path: impl AsRef<Path>) -> anyhow::Result<Self> {
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

#[test]
fn write_new() {
    let manifest = Manifest::read_from("tests/output/Move.toml".to_string());
}
*/
