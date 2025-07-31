use std::{collections::BTreeMap, path::PathBuf};

use serde::{
    Deserialize, Deserializer,
    de::{self, SeqAccess, Visitor},
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

    #[serde(flatten)]
    pub unrecognized_fields: BTreeMap<String, toml::Value>,
}

/// The `implicit-deps` field of a manifest
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImplicitDepMode {
    /// There is no `implicit-deps` field, or there's `implicit-deps = ["foo", "bar"]`
    Enabled(Option<Vec<String>>),

    /// `implicit-deps = false`
    Disabled,

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
        Self::Enabled(None)
    }
}

impl<'de> Deserialize<'de> for ImplicitDepMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ImplicitDepModeVisitor;
        impl<'de> Visitor<'de> for ImplicitDepModeVisitor {
            type Value = ImplicitDepMode;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                // there's other things you can write, but we won't advertise that
                formatter.write_str("the value false or a vector of names")
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

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(s) = seq.next_element::<String>()? {
                    values.push(s)
                }

                Ok(Self::Value::Enabled(Some(values)))
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

    use crate::schema::{ImplicitDepMode, LocalDepInfo, OnChainDepInfo};

    use super::{
        DefaultDependency, ExternalDependency, ManifestDependencyInfo, ManifestGitDependency,
        ParsedManifest, ReplacementDependency,
    };

    impl ParsedManifest {
        /// (unsafe) convenience method for pulling out a dependency having given `name`
        fn get_dep(&self, name: impl AsRef<str>) -> &DefaultDependency {
            self.dependencies
                .iter()
                .find(|(dep_name, dep)| dep_name.as_ref().as_str() == name.as_ref())
                .unwrap()
                .1
        }

        /// (unsafe) convenience method for pulling out a dep-replacement for `env` having given `name`
        fn get_replacement(
            &self,
            env: impl AsRef<str>,
            name: impl AsRef<str>,
        ) -> &ReplacementDependency {
            self.dep_replacements
                .get(env.as_ref())
                .expect("environment exists")
                .iter()
                .find(|(dep_name, dep)| dep_name.as_ref().as_str() == name.as_ref())
                .unwrap()
                .1
                .as_ref()
        }
    }

    /// (unsafe) convenience methods for casting to particular dependency types
    impl ManifestDependencyInfo {
        fn as_external(&self) -> &ExternalDependency {
            let Self::External(ext) = self else {
                panic!("expected external dependency")
            };
            ext
        }

        fn as_local(&self) -> &LocalDepInfo {
            let Self::Local(loc) = self else {
                panic!("expected local dependency")
            };
            loc
        }

        fn as_git(&self) -> &ManifestGitDependency {
            let Self::Git(git) = self else {
                panic!("expected git dependency")
            };
            git
        }

        fn as_onchain(&self) -> &OnChainDepInfo {
            let Self::OnChain(onchain) = self else {
                panic!("expected onchain dependency")
            };
            onchain
        }
    }

    impl ReplacementDependency {
        /// (unsafe) convenience method for unwrapping the dependency info
        fn info(&self) -> &ManifestDependencyInfo {
            &self.dependency.as_ref().unwrap().dependency_info
        }
    }

    // Smoke tests ///////////////////////////////////////////////////////////////////////

    /// Parsing a basic file using a number of features succeeds
    #[test]
    fn basic() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "example"
            edition = "2024"
            license = "Apache-2.0"
            authors = ["Move Team"]
            flavor = "vanilla"

            [environments]
            mainnet = "35834a8a"
            testnet = "4c78adac"

            [dependencies]
            foo = { git = "https://example.com/foo.git", rev = "releases/v1", rename-from = "Foo", override = true}
            qwer = { r.mvr = "@pkg/qwer" }

            [dep-replacements]
            # used to replace dependencies for specific environments
            mainnet.foo = { git = "https://example.com/foo.git", original-id = "0x6ba0cc1a418ff3bebce0ff9ec3961e6cc794af9bc3a4114fb138d00a4c9274bb", published-at = "0x6ba0cc1a418ff3bebce0ff9ec3961e6cc794af9bc3a4114fb138d00a4c9274bb", use-environment = "mainnet_alpha" }

            [dep-replacements.mainnet.bar]
            git = "https://example.com/bar.git"
            original-id = "0x10775b77a3deea86dd3b4a1dbebd18736f85677535e86db56cdb40c52778da5b"
            published-at = "0x10775b77a3deea86dd3b4a1dbebd18736f85677535e86db56cdb40c52778da5b"
            use-environment = "mainnet_beta"
            "#,
        )
        .unwrap();
    }

    // External resolver formatting //////////////////////////////////////////////////////

    /// Parsing with an external resolver works as expected
    #[test]
    fn parse_basic_external_resolver() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            mock = { r.mock-resolver = { resolved = { local = "."} } }
            "#,
        )
        .unwrap();

        let dep = manifest.get_dep("mock").dependency_info.as_external();

        assert_eq!(dep.resolver, "mock-resolver");
        assert_eq!(
            dep.data,
            toml_edit::de::from_str(r#"resolved = { local = "." }"#).unwrap()
        );
    }

    /// You can only have one external resolver
    #[test]
    fn parse_multiple_external_resolvers() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = { r.mvr = "a", r.ext = "b" }
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 7, column 19
          |
        7 |             foo = { r.mvr = "a", r.ext = "b" }
          |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        Externally resolved dependencies may only have one `r.<resolver>` field
        "###);
    }

    /// `r` fields (for external deps) must be objects
    #[test]
    fn parse_nonobject_external() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = { r = 0 }
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 7, column 19
          |
        7 |             foo = { r = 0 }
          |                   ^^^^^^^^^
        invalid type: integer `0`, expected a map for key `r`
        "###);
    }

    // Implicit dependency parsing ///////////////////////////////////////////////////////

    /// The default value for `implicit-deps` is `Enabled`
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

        assert!(manifest.package.implicit_deps == ImplicitDepMode::Enabled(None));
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

    /// You can define specific implicit deps.
    #[test]
    fn parse_specific_implicit_deps() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
                [package]
                name = "test"
                edition = "2024"
                implicit-deps = ["foo", "bar"]
                "#,
        )
        .unwrap();

        assert!(
            manifest.package.implicit_deps
                == ImplicitDepMode::Enabled(Some(vec!["foo".to_string(), "bar".to_string()]))
        );
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

    // Dependency and dep-replacement parsing ////////////////////////////////////////////

    /// You need the `git` field to have a git dependency
    #[test]
    fn parse_incomplete_dep() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = { rename-from = "Foo", override = true, rev = "releases/v1" }
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @r###"
        TOML parse error at line 7, column 19
          |
        7 |             foo = { rename-from = "Foo", override = true, rev = "releases/v1" }
          |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        Invalid dependency; dependencies must have exactly one of the following fields: `git`, `r.<resolver>`, `local`, or `on-chain`.
        "###);
    }

    #[test]
    fn parse_empty_dep() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = {}
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @r###"
        TOML parse error at line 7, column 19
          |
        7 |             foo = {}
          |                   ^^
        Invalid dependency; dependencies must have exactly one of the following fields: `git`, `r.<resolver>`, `local`, or `on-chain`.
        "###);
    }

    /// You can override the complete dependency location information (e.g. a new `git` field) in a
    /// `dep-replacement`
    #[test]
    fn parse_git_override() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = { git = "foo-default.git", rev = "1234" }

            [dep-replacements]
            # Note: the combined dep here should have no revision; the entire dep is overridden
            mainnet.foo = { git = "foo-replacement.git" }
            "#,
        )
        .unwrap();

        let dep = manifest.get_dep("foo").dependency_info.as_git();
        let replacement = manifest.get_replacement("mainnet", "foo").info().as_git();

        assert_eq!(dep.repo, "foo-default.git");
        assert_eq!(dep.rev, Some("1234".into()));

        assert_eq!(replacement.repo, "foo-replacement.git");
        assert_eq!(replacement.rev, None);
    }

    /// If overriding the address of a dependency, you can't just provide the published-at
    #[test]
    #[ignore] // TODO: this test is currently failing because the extra stuff just gets dropped
    fn parse_published_at_without_original_id() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dep-replacements]
            mainnet.foo = { published-at = "1234" }
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @"TODO");
    }

    /// If overriding the address of a dependency, you can't just provide the original-id
    #[test]
    #[ignore] // TODO: this test is currently failing because the extra stuff just gets dropped
    fn parse_original_id_without_published_at() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dep-replacements]
            mainnet.foo = { original-id = "1234" }
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @"TODO");
    }

    // Basic TOML error messages /////////////////////////////////////////////////////////

    /// Top level fields can't be repeated
    #[test]
    fn parse_duplicate_top_level_field() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "name"
            edition = "2025"

            [package]
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @r###"
        TOML parse error at line 6, column 13
          |
        6 |             [package]
          |             ^
        invalid table header
        duplicate key `package` in document root
        "###);
    }

    /// No unrecognized fields at top level
    #[test]
    fn test_unknown_toplevel_field() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "name"
            edition = "2024"

            [unknown]
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @r###"
        TOML parse error at line 6, column 14
          |
        6 |             [unknown]
          |              ^^^^^^^
        unknown field `unknown`, expected one of `package`, `environments`, `dependencies`, `dep-replacements`
        "###);
    }

    // `package` section parsing /////////////////////////////////////////////////////////

    /// Check that we're parsing the [package] section correctly
    #[test]
    fn test_all_package_fields() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            # non-ignored fields
            name = "name"
            edition = "2024"

            # ignored fields
            flavor = "core"
            license = "license"
            authors = ["some author"]
            other_fields = "fine"

            [environments]
            mainnet = "35834a8a"
            "#,
        )
        .unwrap();

        assert_eq!(manifest.package.name.as_ref().as_str(), "name");
        assert_eq!(manifest.package.edition, "2024");

        let unrecognized = manifest.package.unrecognized_fields.keys();
        assert_eq!(
            unrecognized.collect::<Vec<_>>(),
            ["authors", "flavor", "license", "other_fields"]
        );
    }

    /// Unrecognized fields should produce warnings
    #[test]
    #[ignore] // TODO: we need a way to collect warnings in unit tests
    fn parse_unrecognized_package_fields() {
        // TODO: we're not actually producing these warnings!
        todo!()
    }

    /// [package] must be present
    #[test]
    fn parse_no_package_section() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [dependencies]
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 1, column 1
          |
        1 | 
          | ^
        missing field `package`
        "###);
    }

    /// package.name must be present
    #[test]
    fn parse_no_package_name() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            edition = "2024"
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 2, column 13
          |
        2 |             [package]
          |             ^^^^^^^^^
        missing field `name`
        "###);
    }

    /// package.name must be a string
    #[test]
    fn parse_integer_package_name() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = 1
            edition = "2024"
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 3, column 20
          |
        3 |             name = 1
          |                    ^
        invalid type: integer `1`, expected a string
        "###);
    }

    /// package.name must be nonempty
    #[test]
    fn parse_empty_package_name() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = ""
            edition = "2024"
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 3, column 20
          |
        3 |             name = ""
          |                    ^^
        Invalid identifier ''
        "###);
    }

    /// package.name must be an identifier
    #[test]
    fn parse_nonident_package_name() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "®´∑œ"
            edition = "2024"
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 3, column 20
          |
        3 |             name = "®´∑œ"
          |                    ^^^^^^^^^^^
        Invalid identifier '®´∑œ'
        "###);
    }

    /// package.edition must be present
    #[test]
    fn parse_no_edition() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 2, column 13
          |
        2 |             [package]
          |             ^^^^^^^^^
        missing field `edition`
        "###);
    }

    /// package edition must be recognized
    #[test]
    #[ignore] // TODO: this validation currently doesn't happen during parsing. Should it?
    fn parse_unknown_edition() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "unknown"
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @"");
    }

    /// Environment IDs must be strings
    #[test]
    fn test_invalid_env_id() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "name"
            edition = "2024"

            [environments]
            mainnet = 1234
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 7, column 23
          |
        7 |             mainnet = 1234
          |                       ^^^^
        invalid type: integer `1234`, expected a string
        "###);
    }

    /// Rename-from must be a string
    #[test]
    fn test_invalid_rename_from() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "name"
            edition = "2024"

            [dependencies]
            a = { local = "a", rename-from = { "A" = "B" } }
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @r###"
        TOML parse error at line 7, column 46
          |
        7 |             a = { local = "a", rename-from = { "A" = "B" } }
          |                                              ^^^^^^^^^^^^^
        invalid type: map, expected a string
        "###);
    }

    /// Rename-from must be a valid identifier
    #[test]
    fn test_nonident_rename_from() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "name"
            edition = "2024"

            [dependencies]
            a = { local = "a", rename-from = "0xff" }
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @r###"
        TOML parse error at line 7, column 46
          |
        7 |             a = { local = "a", rename-from = "0xff" }
          |                                              ^^^^^^
        Invalid identifier '0xff'
        "###);
    }

    // Tests to remove? //////////////////////////////////////////////////////////////////

    /// Authors must be an array
    #[test]
    #[ignore] // TODO: do we want to validate `authors` type? we currently don't
    fn test_authors() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "name"
            edition = "2024"
            authors = [1]
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @"TODO");

        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "name"
            edition = "2024"
            authors = "me@mystenlabs.com"
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @"TODO");
    }

    /// You can't add partial dependency information (e.g. just updating the `rev` field) in a
    /// `dep-replacement`
    #[test]
    #[ignore] // TODO: this test is currently failing because the extra stuff just gets dropped
    fn parse_git_partial_replacement() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dep-replacements]
            mainnet.foo = { rev = "foo-replacement.git" }
        "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @"TODO");
    }

    // Unsorted tests ////////////////////////////////////////////////////////////////////

    /// `local` field must be a path
    #[test]
    fn parse_local_integer_path() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            a = { local = 1 }
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @r###"
        TOML parse error at line 7, column 17
          |
        7 |             a = { local = 1 }
          |                 ^^^^^^^^^^^^^
        invalid type: integer `1`, expected path string for key `local`
        "###);
    }

    /// [addresses] is dead ♥
    #[test]
    fn parse_addresses_section() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [addresses]
            legacy = 0x0
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @r###"
        TOML parse error at line 6, column 14
          |
        6 |             [addresses]
          |              ^^^^^^^^^
        unknown field `addresses`, expected one of `package`, `environments`, `dependencies`, `dep-replacements`
        "###);
    }
}
