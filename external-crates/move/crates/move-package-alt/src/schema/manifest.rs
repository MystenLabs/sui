use std::{collections::BTreeMap, path::PathBuf};

use serde::{
    Deserialize, Deserializer,
    de::{self, Visitor},
};
use serde_spanned::Spanned;

use super::{
    EnvironmentName, LocalDepInfo, OnChainDepInfo, PackageName, PublishAddresses, ResolverName,
};

/// The on-chain identifier for an environment (such as a chain ID); these are bound to environment
/// names in the `[environments]` table of the manifest
pub type EnvironmentID = String;

// Note: [Manifest] objects are immutable and should not implement [serde::Serialize]; any tool
// writing these files should use [toml_edit] to set / preserve the formatting, since these are
// user-editable files
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
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
#[serde(rename_all = "kebab-case")]
pub struct PackageMetadata {
    pub name: Spanned<PackageName>,
    pub edition: String,

    #[serde(default)]
    pub implicit_deps: ImplicitDepMode,
}

/// The `implicit-deps` field of a manifest
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplicitDepMode {
    /// There is no `implicit-deps` field
    Enabled,

    /// `implicit-deps = false`
    Disabled,

    /// this is only possible in a legacy package
    Legacy,

    /// `implicit-deps = "internal"`
    Testing,
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
    pub rename_from: Option<PackageName>,
}

/// An entry in the `[dep-replacements]` section of a manifest
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(bound = "")]
#[serde(rename_all = "kebab-case")]
pub struct ReplacementDependency {
    #[serde(flatten, default)]
    pub dependency: Option<DefaultDependency>,

    #[serde(flatten, default)]
    pub addresses: Option<PublishAddresses>,

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

    /// The subdir within the repository
    #[serde(default)]
    pub subdir: PathBuf,
}

/// Convenience type for serializing/deserializing external deps
#[derive(Deserialize)]
struct RField {
    r: BTreeMap<String, toml::Value>,
}

impl Default for ImplicitDepMode {
    fn default() -> Self {
        Self::Enabled
    }
}

impl<'de> Deserialize<'de> for ImplicitDepMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ImplicitDepModeVisitor;
        impl Visitor<'_> for ImplicitDepModeVisitor {
            type Value = ImplicitDepMode;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                // there's other things you can write, but we won't advertise that
                formatter.write_str("the value false")
            }

            fn visit_bool<E: de::Error>(self, b: bool) -> Result<Self::Value, E> {
                if b {
                    Err(E::custom(
                        "implicit-deps = true is the default behavior, so should be omitted",
                    ))
                } else {
                    Ok(Self::Value::Disabled)
                }
            }

            fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
                if s == "internal" {
                    Ok(Self::Value::Testing)
                } else {
                    // We hide the truth from the users! For testing in the monorepo, you may also pass
                    // `implicit-deps = "internal"`
                    Err(E::custom(
                        "the only valid value for `implicit-deps` is `implicit-deps = false`",
                    ))
                }
            }
        }

        deserializer.deserialize_any(ImplicitDepModeVisitor)
    }
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

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::schema::ImplicitDepMode;

    use super::ParsedManifest;

    /// The default value for `implicit-deps` is `true`
    #[test]
    fn parse_implicit_deps() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "test"
            edition = "2024"
            "#,
        )
        .unwrap();

        assert!(manifest.package.implicit_deps == ImplicitDepMode::Enabled);
    }

    /// You can turn implicit deps off
    #[test]
    fn parse_explicit_deps() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "test"
            edition = "2024"
            implicit-deps = false
            "#,
        )
        .unwrap();

        assert!(manifest.package.implicit_deps == ImplicitDepMode::Disabled);
    }

    /// You can ask for internal implicit deps
    #[test]
    fn parse_internal_implicit_deps() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "test"
            edition = "2024"
            implicit-deps = "internal"
            "#,
        )
        .unwrap();

        assert!(manifest.package.implicit_deps == ImplicitDepMode::Testing);
    }

    /// implicit deps can't be a random string
    #[test]
    fn parse_bad_implicit_deps() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"
            implicit-deps = "bogus"
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 5, column 29
          |
        5 |             implicit-deps = "bogus"
          |                             ^^^^^^^
        the only valid value for `implicit-deps` is `implicit-deps = false`
        "###);
    }
}
