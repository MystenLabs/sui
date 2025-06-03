use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Deserializer, de};
use serde_spanned::Spanned;

use super::{Address, EnvironmentName, PackageName};

/// The on-chain identifier for an environment (such as a chain ID); these are bound to environment
/// names in the `[environments]` table of the manifest
type EnvironmentID = String;

/// The name of an external resolver
type ResolverName = String;

// Note: [Manifest] objects are immutable and should not implement [serde::Serialize]; any tool
// writing these files should use [toml_edit] to set / preserve the formatting, since these are
// user-editable files
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub package: PackageMetadata,

    #[serde(default)]
    pub environments: BTreeMap<Spanned<EnvironmentName>, Spanned<EnvironmentID>>,

    #[serde(default)]
    pub dependencies: BTreeMap<PackageName, Spanned<ManifestDependency>>,

    /// Replace dependencies for the given environment.
    #[serde(default)]
    pub dep_replacements:
        BTreeMap<EnvironmentName, BTreeMap<PackageName, Spanned<ManifestDependencyReplacement>>>,
}

/// The `[package]` section of a manifest
#[derive(Debug, Deserialize)]
struct PackageMetadata {
    pub name: Spanned<PackageName>,
    pub edition: Spanned<ConstMove2025>,
}

/// The constant string "move2025"
#[derive(Debug, Deserialize)]
#[serde(try_from = "String")]
struct ConstMove2025;

/// An entry in the `[dependencies]` section of the manifest
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct ManifestDependency {
    #[serde(flatten)]
    pub dependency_info: ManifestDependencyInfo,

    #[serde(rename = "override", default)]
    pub is_override: bool,

    #[serde(default)]
    pub rename_from: Option<String>,
}

/// An entry in the `[dep-replacements]` section of the manifest
#[derive(Debug, Deserialize)]
#[serde(bound = "")]
#[serde(rename_all = "kebab-case")]
pub struct ManifestDependencyReplacement {
    #[serde(flatten, default)]
    pub dependency: Option<ManifestDependency>,

    #[serde(default)]
    pub published_at: Option<Address>,

    #[serde(default)]
    pub use_environment: Option<EnvironmentName>,
}

/// [UnpinnedDependencyInfo]s contain the dependency-type-specific things that users write in their
/// Move.toml files in the `dependencies` section.
///
/// TODO: this paragraph will change with upcoming design changes:
/// There are additional general fields in the manifest format (like `override` or `rename-from`)
/// that are not part of the UnpinnedDependencyInfo. We separate these partly because these things
/// are not serialized to the Lock file. See [crate::package::manifest] for the full representation
/// of an entry in the `dependencies` table.
///
// Note: there is a custom Deserializer for this type; be sure to update it if you modify this
#[derive(Debug, Clone)]
pub enum ManifestDependencyInfo {
    Git(ManifestGitDependency),
    External(ExternalDependency),
    Local(LocalDependency),
    OnChain(OnChainDependency),
}

/// A `{local = "<path>"}` dependency
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct LocalDependency {
    /// The path on the filesystem, relative to the location of the containing file (which is
    /// stored in the `Located` wrapper)
    pub local: PathBuf,
}

/// The constant `true`
#[derive(Clone, Debug, Deserialize, PartialEq)]
struct ConstTrue;

/// An on-chain dependency `{on-chain = true}`
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct OnChainDependency {
    #[serde(rename = "on-chain")]
    pub on_chain: ConstTrue,
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
                let dep = LocalDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(ManifestDependencyInfo::Local(dep))
            } else if tbl.contains_key("on-chain") {
                let dep = OnChainDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(ManifestDependencyInfo::OnChain(dep))
            } else {
                Err(de::Error::custom(
                    "Invalid dependency; dependencies must have a field named either `git`, `local`, `on-chain`, or `r`.",
                ))
            }
        } else {
            Err(de::Error::custom("Dependency must be a table"))
        }
    }
}

// TODO: write a macro to generate constants
impl TryFrom<String> for ConstMove2025 {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value != "Move2025" {
            return Err("Unsupported move version {value}; expected `Move2025`");
        }
        Ok(Self)
    }
}

impl TryFrom<bool> for ConstTrue {
    type Error = &'static str;

    fn try_from(value: bool) -> Result<Self, Self::Error> {
        if value != true {
            return Err("Expected the constant `true`");
        }
        Ok(Self)
    }
}

/// Convenience type for serializing/deserializing external deps
#[derive(Deserialize)]
struct RField {
    r: BTreeMap<String, toml::Value>,
}

impl TryFrom<RField> for ExternalDependency {
    type Error = &'static str;

    /// Convert from [RField] (`{r.<res> = <data>}`) to [ExternalDependency] (`{ res, data }`)
    fn try_from(value: RField) -> Result<Self, Self::Error> {
        if value.r.len() != 1 {
            return Err(
                "Externally resolved dependencies should have the form `{r.<resolver-name> = <resolver-data>}`",
            );
        }

        let (resolver, data) = value
            .r
            .into_iter()
            .next()
            .expect("iterator of length 1 structure is nonempty");

        Ok(Self { resolver, data })
    }
}
