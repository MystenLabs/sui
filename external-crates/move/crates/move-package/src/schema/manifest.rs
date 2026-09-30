use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
    str::FromStr,
};

use anyhow::ensure;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeMap};
use serde_spanned::Spanned;

use move_compiler::{
    diagnostics::{
        codes::DiagnosticsID,
        filter::{FilterKind, FilterName, FilterPrefix, resolve_filter_names},
    },
    editions::Edition,
    shared::known_attributes::DiagnosticAttribute,
};

use crate::compatibility::legacy::LegacyData;

use super::{
    EnvironmentName, LocalDepInfo, OnChainAddress, OnChainPlaceholder, PackageName,
    PublishAddresses, ResolverName,
};

/// The on-chain identifier for an environment (such as a chain ID); these are bound to environment
/// names in the `[environments]` table of the manifest
pub type EnvironmentID = String;

/// The name of a mode
pub type ModeName = String;

/// The identifier for a system dependency (in `{system = "dep_id"}` dependencies
pub type SystemDepName = String;

pub type ConfigFilters = BTreeMap<String, LintLevel>;
pub type KnownDiagnosticFilters = Vec<(FilterPrefix, Vec<(FilterName, Vec<DiagnosticsID>)>)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticProfile {
    Build,
    Test,
}

impl DiagnosticProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Test => "test",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "build" => Some(Self::Build),
            "test" => Some(Self::Test),
            _ => None,
        }
    }
}

impl Serialize for DiagnosticProfile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticFilterEntries {
    pub filters: ConfigFilters,
    pub profile_filters: BTreeMap<DiagnosticProfile, ConfigFilters>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticConfigObject {
    pub warning_filters: DiagnosticFilterEntries,
    pub lint_filters: DiagnosticFilterEntries,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DiagnosticConfigError {
    UnknownWarning(FilterName),
    UnknownLint(FilterName),
    LintConfiguredAsWarning(FilterName),
    WarningConfiguredAsLint(FilterName),
}

impl fmt::Display for DiagnosticConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownWarning(name) => {
                write!(f, "unknown warning filter '{name}' in Move.toml")
            }
            Self::UnknownLint(name) => {
                write!(f, "unknown warning filter 'lint({name})' in Move.toml")
            }
            Self::LintConfiguredAsWarning(name) => {
                write!(
                    f,
                    "lint '{name}' must be configured under [lints], not [warnings]"
                )
            }
            Self::WarningConfiguredAsLint(name) => {
                write!(
                    f,
                    "compiler warning '{name}' must be configured under [warnings], not [lints]"
                )
            }
        }
    }
}

impl std::error::Error for DiagnosticConfigError {}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LintLevel {
    Allow,
    Warn,
    Deny,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DiagnosticFilterEntry {
    Level(LintLevel),
    Mode(ConfigFilters),
}

impl<'de> Deserialize<'de> for DiagnosticFilterEntries {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries = BTreeMap::<String, DiagnosticFilterEntry>::deserialize(deserializer)?;
        let mut filters = BTreeMap::new();
        let mut profile_filters = BTreeMap::new();
        for (name, entry) in entries {
            match entry {
                DiagnosticFilterEntry::Level(level) => {
                    filters.insert(name, level);
                }
                DiagnosticFilterEntry::Mode(config) => {
                    let profile = DiagnosticProfile::from_name(&name).ok_or_else(|| {
                        de::Error::custom(format!(
                            "unknown diagnostic profile '{name}', expected 'build' or 'test'"
                        ))
                    })?;
                    profile_filters.insert(profile, config);
                }
            }
        }
        Ok(Self {
            filters,
            profile_filters,
        })
    }
}

impl Serialize for DiagnosticFilterEntries {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map =
            serializer.serialize_map(Some(self.filters.len() + self.profile_filters.len()))?;
        for (name, level) in &self.filters {
            map.serialize_entry(name, level)?;
        }
        for (profile, config) in &self.profile_filters {
            map.serialize_entry(profile.as_str(), config)?;
        }
        map.end()
    }
}

impl DiagnosticFilterEntries {
    pub fn filters_no_profile(&self) -> impl Iterator<Item = (&str, LintLevel)> + '_ {
        self.filters
            .iter()
            .map(|(name, level)| (name.as_str(), *level))
    }

    pub fn filters(
        &self,
        profile: DiagnosticProfile,
    ) -> impl Iterator<Item = (&str, LintLevel)> + '_ {
        let profile_filters = self.profile_filters.get(&profile);
        self.filters_no_profile()
            .filter(move |(name, _)| {
                !profile_filters.is_some_and(|filters| filters.contains_key(*name))
            })
            .chain(
                profile_filters
                    .into_iter()
                    .flat_map(|filters| filters.iter())
                    .map(|(name, level)| (name.as_str(), *level)),
            )
    }

    fn configured_filters(
        &self,
        prefix: FilterPrefix,
        profile: DiagnosticProfile,
    ) -> impl Iterator<Item = (FilterPrefix, FilterName, FilterKind)> + '_ {
        self.filters(profile)
            .map(move |(name, level)| (prefix, name.into(), level.filter_kind()))
    }

    fn all_configured_filters(
        &self,
        prefix: FilterPrefix,
    ) -> impl Iterator<Item = (FilterPrefix, FilterName, FilterKind)> + '_ {
        self.filters
            .iter()
            .chain(
                self.profile_filters
                    .values()
                    .flat_map(|profile| profile.iter()),
            )
            .map(move |(name, level)| (prefix, name.as_str().into(), level.filter_kind()))
    }
}

impl DiagnosticConfigObject {
    pub fn configured_filters(
        &self,
        profile: DiagnosticProfile,
    ) -> impl Iterator<Item = (FilterPrefix, FilterName, FilterKind)> + '_ {
        self.warning_filters
            .configured_filters(None, profile)
            .chain(
                self.lint_filters
                    .configured_filters(Some(DiagnosticAttribute::LINT_SYMBOL), profile),
            )
    }

    pub fn enabled_lints(&self, profile: DiagnosticProfile) -> BTreeSet<FilterName> {
        self.lint_filters
            .filters(profile)
            .filter(|(_, level)| *level != LintLevel::Allow)
            .map(|(name, _)| name.into())
            .collect()
    }

    pub fn allowed_lints(&self, profile: DiagnosticProfile) -> BTreeSet<FilterName> {
        self.lint_filters
            .filters(profile)
            .filter(|(_, level)| *level == LintLevel::Allow)
            .map(|(name, _)| name.into())
            .collect()
    }

    pub fn validate(&self, known: &KnownDiagnosticFilters) -> Result<(), DiagnosticConfigError> {
        let warnings = self.warning_filters.all_configured_filters(None);
        if let Err((_, name)) = resolve_filter_names(warnings, known.iter().cloned()) {
            let is_lint = resolve_filter_names(
                [(
                    Some(DiagnosticAttribute::LINT_SYMBOL),
                    name,
                    FilterKind::Warn,
                )],
                known.iter().cloned(),
            )
            .is_ok();
            return Err(if is_lint {
                DiagnosticConfigError::LintConfiguredAsWarning(name)
            } else {
                DiagnosticConfigError::UnknownWarning(name)
            });
        }

        let lints = self
            .lint_filters
            .all_configured_filters(Some(DiagnosticAttribute::LINT_SYMBOL));
        if let Err((_, name)) = resolve_filter_names(lints, known.iter().cloned()) {
            let is_warning =
                resolve_filter_names([(None, name, FilterKind::Warn)], known.iter().cloned())
                    .is_ok();
            return Err(if is_warning {
                DiagnosticConfigError::WarningConfiguredAsLint(name)
            } else {
                DiagnosticConfigError::UnknownLint(name)
            });
        }

        Ok(())
    }
}

impl LintLevel {
    fn filter_kind(self) -> FilterKind {
        match self {
            Self::Allow => FilterKind::Allow,
            Self::Warn => FilterKind::Warn,
            Self::Deny => FilterKind::Deny,
        }
    }
}

// Note: [Manifest] objects should not be mutated or serialized; they are user-defined files so
// tools that write them should use [toml_edit] to set / preserve the formatting. However, we do
// implement [Serialize] and provide [render_as_toml], primarily for generating tests
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ParsedManifest {
    pub package: PackageMetadata,

    #[serde(default)]
    pub warnings: DiagnosticFilterEntries,

    #[serde(default)]
    pub lints: DiagnosticFilterEntries,

    #[serde(default)]
    pub environments: BTreeMap<Spanned<EnvironmentName>, Spanned<EnvironmentID>>,

    #[serde(default)]
    pub dependencies: BTreeMap<Spanned<PackageName>, DefaultDependency>,

    /// Replace dependencies for the given environment.
    #[serde(default)]
    pub dep_replacements:
        BTreeMap<EnvironmentName, BTreeMap<PackageName, Spanned<ReplacementDependency>>>,

    /// Additional information that we may need when we handle legacy packages. This data is only
    /// populated by the legacy parser
    #[serde(skip)]
    pub legacy_data: Option<LegacyData>,
}

/// The `[package]` section of a manifest
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct PackageMetadata {
    pub name: Spanned<PackageName>,

    #[serde(default, deserialize_with = "from_str_option")]
    pub edition: Option<Edition>,

    #[serde(default = "return_true")]
    pub implicit_dependencies: bool,

    #[serde(flatten)]
    pub unrecognized_fields: BTreeMap<String, toml::Value>,
}

fn return_true() -> bool {
    true
}

/// An entry in the `[dependencies]` section of a manifest
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct DefaultDependency {
    #[serde(flatten)]
    pub dependency_info: ManifestDependencyInfo,

    #[serde(rename = "override", default)]
    pub is_override: bool,

    #[serde(default)]
    pub rename_from: Option<PackageName>,

    #[serde(default)]
    pub modes: Option<Vec<ModeName>>,
}

/// An entry in the `[dep-replacements]` section of a manifest
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
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
#[derive(Debug, Clone, Serialize)]
pub enum ManifestDependencyInfo {
    Git(ManifestGitDependency),
    External(ExternalDependency),
    Local(LocalDepInfo),
    OnChainPlaceholder(OnChainPlaceholder),
    OnChain(OnChainAddress),
    System(SystemDependency),
}

/// An external dependency has the form `{ r.<res> = <data> }`. External
/// dependencies are resolved by external resolvers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(try_from = "RField", into = "RField")]
pub struct ExternalDependency {
    /// The `<res>` in `{ r.<res> = <data> }`
    pub resolver: ResolverName,

    /// the `<data>` in `{ r.<res> = <data> }`
    pub data: toml::Value,
}

/// A `{git = "..."}` dependency in a manifest
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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

/// A `{system = "..."}` dependency in a manifest
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SystemDependency {
    pub system: SystemDepName,
}

/// Convenience type for serializing/deserializing external deps
#[derive(Serialize, Deserialize)]
struct RField {
    r: BTreeMap<ResolverName, toml::Value>,
}

impl ReplacementDependency {
    /// Convenience method for creating a `{ system = <name>, override = true }` dep
    pub fn override_system_dep(name: &str) -> ReplacementDependency {
        ReplacementDependency {
            dependency: Some(DefaultDependency {
                dependency_info: ManifestDependencyInfo::System(SystemDependency {
                    system: name.into(),
                }),
                is_override: true,
                rename_from: None,
                modes: None,
            }),
            addresses: None,
            use_environment: None,
        }
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
            } else if tbl.contains_key("system") {
                let dep = SystemDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(ManifestDependencyInfo::System(dep))
            } else if tbl.contains_key("r") {
                let dep = ExternalDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(ManifestDependencyInfo::External(dep))
            } else if tbl.contains_key("local") {
                let dep = LocalDepInfo::deserialize(data).map_err(de::Error::custom)?;
                Ok(ManifestDependencyInfo::Local(dep))
            } else if tbl.contains_key("on-chain") {
                match &tbl["on-chain"] {
                    toml::Value::Boolean(_) => OnChainPlaceholder::deserialize(data)
                        .map(ManifestDependencyInfo::OnChainPlaceholder)
                        .map_err(de::Error::custom),
                    toml::Value::String(_) => OnChainAddress::deserialize(data)
                        .map(ManifestDependencyInfo::OnChain)
                        .map_err(de::Error::custom),
                    _ => Err(de::Error::custom(
                        "on-chain must be `true` (in [dependencies]) or a hex address string \
                         like \"0x...\" (in [dep-replacements])",
                    )),
                }
            } else {
                Err(de::Error::custom(
                    "Invalid dependency; dependencies must have exactly one of the following fields: `system`, `git`, `r.<resolver>`, `local`, or `on-chain`.",
                ))
            }
        } else {
            Err(de::Error::custom("Dependency must be a table"))
        }
    }
}

impl TryFrom<RField> for ExternalDependency {
    type Error = anyhow::Error;

    /// Convert from [RField] (`{r.<res> = <data>}`) to [ExternalDependency] (`{ res, data }`)
    fn try_from(value: RField) -> Result<Self, Self::Error> {
        ensure!(
            value.r.len() == 1,
            "Externally resolved dependencies may only have one `r.<resolver>` field"
        );

        let (resolver, data) = value
            .r
            .into_iter()
            .next()
            .expect("iterator of length 1 structure is nonempty");

        Ok(Self { resolver, data })
    }
}

impl From<ExternalDependency> for RField {
    fn from(value: ExternalDependency) -> Self {
        Self {
            r: BTreeMap::from([(value.resolver, value.data)]),
        }
    }
}

fn from_str_option<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: FromStr,
    T::Err: std::fmt::Display,
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(s) => T::from_str(&s).map(Some).map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::{
        ConfigFilters, DefaultDependency, DiagnosticConfigError, DiagnosticConfigObject,
        DiagnosticFilterEntries, DiagnosticProfile, ExternalDependency, LintLevel,
        ManifestDependencyInfo, ManifestGitDependency, ParsedManifest, ReplacementDependency,
    };
    use move_compiler::{diagnostics::filter::FilterName, editions::Edition, linters};
    use std::{
        collections::{BTreeMap, BTreeSet},
        str::FromStr,
    };

    impl ParsedManifest {
        /// (unsafe) convenience method for pulling out a dependency having given `name`
        fn get_dep(&self, name: impl AsRef<str>) -> &DefaultDependency {
            self.dependencies
                .iter()
                .find(|(dep_name, _)| dep_name.as_ref().as_str() == name.as_ref())
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
                .find(|(dep_name, _)| dep_name.as_ref().as_str() == name.as_ref())
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

        fn as_git(&self) -> &ManifestGitDependency {
            let Self::Git(git) = self else {
                panic!("expected git dependency")
            };
            git
        }
    }

    impl ReplacementDependency {
        /// (unsafe) convenience method for unwrapping the dependency info
        fn info(&self) -> &ManifestDependencyInfo {
            &self.dependency.as_ref().unwrap().dependency_info
        }
    }

    // Smoke tests ///////////////////////////////////////////////////////////////////////

    #[test]
    fn lint_levels() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "example"

            [warnings]
            unused = "deny"

            [lints]
            abort_without_constant = "allow"
            "#,
        )
        .unwrap();

        assert_eq!(
            manifest.warnings.filters.get("unused"),
            Some(&LintLevel::Deny)
        );
        assert_eq!(
            manifest.lints.filters.get("abort_without_constant"),
            Some(&LintLevel::Allow)
        );
    }

    #[test]
    fn lint_levels_for_profiles() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "example"

            [lints]
            all = "deny"
            shared_object_derp = "warn"
            build = { all = "allow", shared_object_derp = "allow" }

            [lints.test]
            shared_object_derp = "deny"
            "#,
        )
        .unwrap();

        assert_eq!(manifest.lints.filters.get("all"), Some(&LintLevel::Deny));
        assert_eq!(
            manifest.lints.profile_filters[&DiagnosticProfile::Build].get("shared_object_derp"),
            Some(&LintLevel::Allow)
        );
        assert_eq!(
            manifest.lints.profile_filters[&DiagnosticProfile::Test].get("shared_object_derp"),
            Some(&LintLevel::Deny)
        );
    }

    #[test]
    fn profile_filters_override_base_filters() {
        let entries = DiagnosticFilterEntries {
            filters: ConfigFilters::from([
                ("all".to_string(), LintLevel::Deny),
                ("base_only".to_string(), LintLevel::Warn),
                ("overridden".to_string(), LintLevel::Warn),
            ]),
            profile_filters: BTreeMap::from([
                (
                    DiagnosticProfile::Build,
                    ConfigFilters::from([
                        ("all".to_string(), LintLevel::Allow),
                        ("overridden".to_string(), LintLevel::Allow),
                    ]),
                ),
                (
                    DiagnosticProfile::Test,
                    ConfigFilters::from([("overridden".to_string(), LintLevel::Deny)]),
                ),
            ]),
        };

        assert_eq!(
            entries.filters_no_profile().collect::<BTreeMap<_, _>>(),
            BTreeMap::from([
                ("all", LintLevel::Deny),
                ("base_only", LintLevel::Warn),
                ("overridden", LintLevel::Warn),
            ])
        );
        assert_eq!(
            entries
                .filters(DiagnosticProfile::Build)
                .collect::<BTreeMap<_, _>>(),
            BTreeMap::from([
                ("all", LintLevel::Allow),
                ("base_only", LintLevel::Warn),
                ("overridden", LintLevel::Allow),
            ])
        );
        assert_eq!(
            entries
                .filters(DiagnosticProfile::Test)
                .collect::<BTreeMap<_, _>>(),
            BTreeMap::from([
                ("all", LintLevel::Deny),
                ("base_only", LintLevel::Warn),
                ("overridden", LintLevel::Deny),
            ])
        );
    }

    #[test]
    fn allowed_and_enabled_lints_use_effective_profile() {
        let diagnostics = DiagnosticConfigObject {
            warning_filters: DiagnosticFilterEntries::default(),
            lint_filters: DiagnosticFilterEntries {
                filters: ConfigFilters::from([
                    ("all".to_string(), LintLevel::Deny),
                    ("abort_without_constant".to_string(), LintLevel::Warn),
                ]),
                profile_filters: BTreeMap::from([(
                    DiagnosticProfile::Build,
                    ConfigFilters::from([
                        ("all".to_string(), LintLevel::Allow),
                        ("abort_without_constant".to_string(), LintLevel::Deny),
                    ]),
                )]),
            },
        };

        assert_eq!(
            diagnostics.allowed_lints(DiagnosticProfile::Build),
            BTreeSet::from([FilterName::from("all")])
        );
        assert_eq!(
            diagnostics.enabled_lints(DiagnosticProfile::Build),
            BTreeSet::from([FilterName::from("abort_without_constant")])
        );
        assert!(
            diagnostics
                .allowed_lints(DiagnosticProfile::Test)
                .is_empty()
        );
        assert_eq!(
            diagnostics.enabled_lints(DiagnosticProfile::Test),
            BTreeSet::from([
                FilterName::from("all"),
                FilterName::from("abort_without_constant"),
            ])
        );
    }

    #[test]
    fn unknown_diagnostic_profiles_are_rejected() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "example"

            [lints.spec]
            all = "deny"
            "#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("unknown diagnostic profile 'spec', expected 'build' or 'test'")
        );
    }

    #[test]
    fn diagnostic_validation_errors_are_consistent() {
        let known = vec![linters::known_filters()];
        let check = |warning: Option<&str>, lint: Option<&str>| {
            DiagnosticConfigObject {
                warning_filters: DiagnosticFilterEntries {
                    filters: warning
                        .map(|name| ConfigFilters::from([(name.to_string(), LintLevel::Warn)]))
                        .unwrap_or_default(),
                    profile_filters: BTreeMap::new(),
                },
                lint_filters: DiagnosticFilterEntries {
                    filters: lint
                        .map(|name| ConfigFilters::from([(name.to_string(), LintLevel::Warn)]))
                        .unwrap_or_default(),
                    profile_filters: BTreeMap::new(),
                },
            }
            .validate(&known)
            .unwrap_err()
        };

        assert_eq!(
            check(Some("abort_without_constant"), None),
            DiagnosticConfigError::LintConfiguredAsWarning(FilterName::from(
                "abort_without_constant",
            ))
        );
        assert_eq!(
            check(None, Some("unused_variable")),
            DiagnosticConfigError::WarningConfiguredAsLint(FilterName::from("unused_variable"))
        );
        assert_eq!(
            check(Some("not_a_warning"), None),
            DiagnosticConfigError::UnknownWarning(FilterName::from("not_a_warning"))
        );
        assert_eq!(
            check(None, Some("not_a_lint")),
            DiagnosticConfigError::UnknownLint(FilterName::from("not_a_lint"))
        );
    }

    /// Parsing a basic file using a number of features succeeds
    #[test]
    fn basic() {
        let _: ParsedManifest = toml_edit::de::from_str(
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
            tester = { local = "../tester", modes = ["test"] }
            system = { system = "foo" }

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

        assert_eq!(dep.resolver.to_string(), "mock-resolver");
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

    /// external resolver names can't contain invalid characters (DVX-2019)
    #[test]
    fn parse_malformed_external_resolver() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"

            [dependencies]
            foo = { r."foo//bar" = "foo" }
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 6, column 19
          |
        6 |             foo = { r."foo//bar" = "foo" }
          |                   ^^^^^^^^^^^^^^^^^^^^^^^^
        invalid character in external resolver name `foo//bar` for key `r`
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

    /// The default value for `implicit-dependencies` is `Enabled`
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

        assert!(manifest.package.implicit_dependencies);
    }

    /// You can turn implicit deps off
    #[test]
    fn parse_explicit_deps() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "test"
            edition = "2024"
            implicit-dependencies = false
            "#,
        )
        .unwrap();

        assert!(!manifest.package.implicit_dependencies);
    }

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
        Invalid dependency; dependencies must have exactly one of the following fields: `system`, `git`, `r.<resolver>`, `local`, or `on-chain`.
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
        Invalid dependency; dependencies must have exactly one of the following fields: `system`, `git`, `r.<resolver>`, `local`, or `on-chain`.
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
            edition = "2024"

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

        assert_snapshot!(error, @r"
        TOML parse error at line 6, column 14
          |
        6 |             [unknown]
          |              ^^^^^^^
        unknown field `unknown`, expected one of `package`, `warnings`, `lints`, `environments`, `dependencies`, `dep-replacements`
        ");
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
        assert_eq!(
            manifest.package.edition,
            Some(Edition::from_str("2024").unwrap())
        );

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

    /// package.edition not allowed
    #[test]
    fn parse_unsupported_edition() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2025"
            "#,
        )
        .unwrap_err()
        .to_string();
        assert_snapshot!(error, @r###"
        TOML parse error at line 4, column 23
          |
        4 |             edition = "2025"
          |                       ^^^^^^
        Unsupported edition "2025". Current supported editions include: "legacy", "2024.alpha", "2024.beta", and "2024"
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
    #[ignore] // TODO: pkg-alt this test is currently failing because the extra stuff just gets dropped
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

    // On-chain dependency parsing ///////////////////////////////////////////////////

    /// Parsing `on-chain = true` in [dependencies] succeeds
    #[test]
    fn parse_on_chain_flag() {
        let manifest: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = { on-chain = true }
            "#,
        )
        .unwrap();

        assert!(matches!(
            manifest.get_dep("foo").dependency_info,
            ManifestDependencyInfo::OnChainPlaceholder(_)
        ));
    }

    /// Parsing `on-chain = "0x1234"` in [dep-replacements] succeeds
    #[test]
    fn parse_on_chain_address_in_replacement() {
        let _: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = { on-chain = true }

            [dep-replacements]
            mainnet.foo = { on-chain = "0x0000000000000000000000000000000000000000000000000000000000000001" }
            "#,
        )
        .unwrap();
    }

    /// Parsing `on-chain = false` is rejected
    #[test]
    fn parse_on_chain_false() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = { on-chain = false }
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @r###"
        TOML parse error at line 7, column 19
          |
        7 |             foo = { on-chain = false }
          |                   ^^^^^^^^^^^^^^^^^^^^
        Expected the constant `true` for key `on-chain`
        "###);
    }

    /// Parsing `on-chain = 42` is rejected
    #[test]
    fn parse_on_chain_integer() {
        let error = toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = { on-chain = 42 }
            "#,
        )
        .unwrap_err()
        .to_string();

        assert_snapshot!(error, @r###"
        TOML parse error at line 7, column 19
          |
        7 |             foo = { on-chain = 42 }
          |                   ^^^^^^^^^^^^^^^^^
        on-chain must be `true` (in [dependencies]) or a hex address string like "0x..." (in [dep-replacements])
        "###);
    }

    /// Parsing a short hex address like `on-chain = "0x1"` succeeds (addresses are zero-padded)
    #[test]
    fn parse_on_chain_short_address() {
        let _: ParsedManifest = toml_edit::de::from_str(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = { on-chain = true }

            [dep-replacements]
            mainnet.foo = { on-chain = "0x1" }
            "#,
        )
        .unwrap();
    }

    /// Parsing an address longer than 32 bytes should be a parse error.
    // TODO(DVX-2143): currently silently drops the invalid address due to serde flatten+default
    #[test]
    #[ignore]
    fn parse_on_chain_too_long_address() {
        toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = { on-chain = true }

            [dep-replacements]
            mainnet.foo = { on-chain = "0x00000000000000000000000000000000000000000000000000000000000000000001" }
            "#,
        )
        .unwrap_err();
    }

    /// Parsing an invalid hex string should be a parse error.
    // TODO(DVX-2143): currently silently drops the invalid address due to serde flatten+default
    #[test]
    #[ignore]
    fn parse_on_chain_invalid_hex() {
        toml_edit::de::from_str::<ParsedManifest>(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dependencies]
            foo = { on-chain = true }

            [dep-replacements]
            mainnet.foo = { on-chain = "0x0000q" }
            "#,
        )
        .unwrap_err();
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

        assert_snapshot!(error, @r"
        TOML parse error at line 6, column 14
          |
        6 |             [addresses]
          |              ^^^^^^^^^
        unknown field `addresses`, expected one of `package`, `warnings`, `lints`, `environments`, `dependencies`, `dep-replacements`
        ");
    }
}
