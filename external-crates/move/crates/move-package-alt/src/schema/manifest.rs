use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Deserializer, de};
use serde_spanned::Spanned;

use crate::dependency::DependencySet;

use super::{Address, EnvironmentName, LocalDepInfo, OnChainDepInfo, PackageName, ResolverName};

// TODO: look at Brandon's serialization code (https://github.com/MystenLabs/sui-rust-sdk/blob/master/crates/sui-sdk-types/src/object.rs)

/// The on-chain identifier for an environment (such as a chain ID); these are bound to environment
/// names in the `[environments]` table of the manifest
pub type EnvironmentID = String;

// Note: [Manifest] objects are immutable and should not implement [serde::Serialize]; any tool
// writing these files should use [toml_edit] to set / preserve the formatting, since these are
// user-editable files
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ParsedManifest {
    pub package: PackageMetadata,

    #[serde(default)]
    pub environments: BTreeMap<Spanned<EnvironmentName>, Spanned<EnvironmentID>>,

    #[serde(default)]
    pub dependencies: BTreeMap<Spanned<PackageName>, DefaultDependency>,

    /// Replace dependencies for the given environment.
    #[serde(default)]
    pub dep_replacements:
        BTreeMap<EnvironmentName, BTreeMap<PackageName, Spanned<ReplacementDependency>>>,
}

/// The `[package]` section of a manifest
#[derive(Debug, Deserialize, Clone)]
pub struct PackageMetadata {
    pub name: Spanned<PackageName>,
    edition: String,
}

/// An entry in the `[dependencies]` section of a manifest
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct DefaultDependency {
    #[serde(flatten)]
    pub dependency_info: ManifestDependencyInfo,

    #[serde(rename = "override", default)]
    pub is_override: bool,

    #[serde(default)]
    pub rename_from: Option<String>,
}

/// An entry in the `[dep-replacements]` section of a manifest
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(bound = "")]
#[serde(rename_all = "kebab-case")]
pub struct ReplacementDependency {
    #[serde(flatten, default)]
    pub dependency: Option<DefaultDependency>,

    #[serde(default)]
    pub published_at: Option<Address>,

    #[serde(default)]
    pub use_environment: Option<EnvironmentName>,
}

/// [ManifestDependencyInfo]s contain the dependency-type-specific things that users write in their
/// Move.toml files in the `dependencies` section.
///
/// There are additional general fields in the manifest format (like `override` or `rename-from`);
/// these are in the [ManifestDependency] or [ManifestDependencyReplacement] types.
#[derive(Debug, Clone)]
pub enum ManifestDependencyInfo {
    Git(ManifestGitDependency),
    External(ExternalDependency),
    Local(LocalDepInfo),
    OnChain(OnChainDepInfo),
}

/// An external dependency has the form `{ r.<res> = <data> }`. External
/// dependencies are resolved by external resolvers.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(try_from = "RField", into = "RField")]
pub struct ExternalDependency {
    /// The `<res>` in `{ r.<res> = <data> }`
    pub resolver: ResolverName,

    /// the `<data>` in `{ r.<res> = <data> }`
    pub data: toml::Value,
}

/// A `{git = "..."}` dependency in a manifest
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ManifestGitDependency {
    /// The repository containing the dependency
    #[serde(rename = "git")]
    pub repo: String,

    /// The git commit or branch for the dependency.
    #[serde(default)]
    pub rev: Option<String>,

    /// The path within the repository
    #[serde(default)]
    pub path: PathBuf,
}

/// Convenience type for serializing/deserializing external deps
#[derive(Deserialize)]
struct RField {
    r: BTreeMap<String, toml::Value>,
}

impl<'de> Deserialize<'de> for ManifestDependencyInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // TODO: maybe write a macro to generate this and other similar things
        let data = toml::value::Value::deserialize(deserializer)?;

        if let Some(tbl) = data.as_table() {
            if tbl.contains_key("git") {
                let dep = ManifestGitDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(ManifestDependencyInfo::Git(dep))
            } else if tbl.contains_key("r") {
                let dep = ExternalDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(ManifestDependencyInfo::External(dep))
            } else if tbl.contains_key("local") {
                let dep = LocalDepInfo::deserialize(data).map_err(de::Error::custom)?;
                Ok(ManifestDependencyInfo::Local(dep))
            } else if tbl.contains_key("on-chain") {
                let dep = OnChainDepInfo::deserialize(data).map_err(de::Error::custom)?;
                Ok(ManifestDependencyInfo::OnChain(dep))
            } else {
                Err(de::Error::custom(
                    "Invalid dependency; dependencies must have exactly one of the following fields: `git`, `r.<resolver>`, `local`, or `on-chain`.",
                ))
            }
        } else {
            Err(de::Error::custom("Dependency must be a table"))
        }
    }
}

impl TryFrom<RField> for ExternalDependency {
    type Error = &'static str;

    /// Convert from [RField] (`{r.<res> = <data>}`) to [ExternalDependency] (`{ res, data }`)
    fn try_from(value: RField) -> Result<Self, Self::Error> {
        if value.r.len() != 1 {
            return Err("Externally resolved dependencies may only have one `r.<resolver>` field");
        }

        let (resolver, data) = value
            .r
            .into_iter()
            .next()
            .expect("iterator of length 1 structure is nonempty");

        Ok(Self { resolver, data })
    }
}
