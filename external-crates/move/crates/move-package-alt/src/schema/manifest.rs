use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use codespan_reporting::diagnostic::{Diagnostic, Label};
use serde::{Deserialize, Deserializer, de};
use serde_spanned::Spanned;
use sha2::{Digest as ShaDigest, Sha256};
use thiserror::Error;

use super::{
    EnvironmentName, LocalDepInfo, OnChainDepInfo, PackageName, PublishAddresses, ResolverName,
};

use crate::errors::{FileHandle, Location};

const ALLOWED_EDITIONS: &[&str] = &["2025", "2024", "2024.beta", "legacy"];

/// The on-chain identifier for an environment (such as a chain ID); these are bound to environment
/// names in the `[environments]` table of the manifest
pub type EnvironmentID = String;

pub type ManifestResult<T> = Result<T, ManifestError>;
pub type Digest = String;

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
pub struct PackageMetadata {
    pub name: Spanned<PackageName>,
    pub edition: String,
    #[serde(skip)]
    pub digest: Digest,
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

#[derive(Error, Debug)]
#[error("{kind}")]
pub struct ManifestError {
    pub kind: Box<ManifestErrorKind>,
    location: ErrorLocation,
}

#[derive(Debug)]
enum ErrorLocation {
    WholeFile(PathBuf),
    AtLoc(Location),
}

#[derive(Error, Debug)]
pub enum ManifestErrorKind {
    #[error("package name cannot be empty")]
    EmptyPackageName,
    #[error("unsupported edition '{edition}', expected one of '{valid}'")]
    InvalidEdition { edition: String, valid: String },
    #[error("externally resolved dependencies must have exactly one resolver field")]
    BadExternalDependency,
    #[error(
        "dep-replacements.mainnet is invalid because mainnet is not in the [environments] table"
    )]
    MissingEnvironment { env: EnvironmentName },
    #[error(
        // TODO: add a suggested environment (needs to be part of the flavor)
        "you must define at least one environment in the [environments] section of `Move.toml`."
    )]
    NoEnvironments,
    #[error("{}", .0.message())]
    ParseError(#[from] toml_edit::de::Error),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

impl ParsedManifest {
    pub fn read_from_file(file_handle: FileHandle) -> ManifestResult<Self> {
        let mut parsed: ParsedManifest = toml_edit::de::from_str(file_handle.source())
            .map_err(ManifestError::from_toml(file_handle))?;
        parsed.set_digest(&file_handle);

        parsed.validate_manifest(file_handle)?;

        Ok(parsed)
    }

    /// Set the digest of the manifest to a SHA256 hash of the file contents.
    fn set_digest(&mut self, file_id: &FileHandle) {
        self.package.digest = format!("{:X}", Sha256::digest(file_id.source().as_ref()));
    }

    /// The combined entries of the `[dependencies]` and `[dep-replacements]` sections for this
    /// manifest
    pub fn dependencies(&self) -> BTreeMap<PackageName, DefaultDependency> {
        self.dependencies
            .iter()
            .map(|(name, dep)| (name.as_ref().clone(), dep.clone()))
            .collect()
    }

    pub fn dep_replacements(
        &self,
    ) -> &BTreeMap<EnvironmentName, BTreeMap<PackageName, Spanned<ReplacementDependency>>> {
        &self.dep_replacements
    }

    pub fn metadata(&self) -> PackageMetadata {
        self.package.clone()
    }

    /// The entries from the `[environments]` section
    pub fn environments(&self) -> BTreeMap<EnvironmentName, EnvironmentID> {
        self.environments
            .iter()
            .map(|(name, id)| (name.as_ref().clone(), id.as_ref().clone()))
            .collect()
    }

    /// The name declared in the `[package]` section
    pub fn package_name(&self) -> &PackageName {
        self.package.name.as_ref()
    }

    /// A digest of the file, suitable for detecting changes
    pub fn digest(&self) -> &Digest {
        &self.package.digest
    }

    /// Validate the manifest contents, after deserialization.
    ///
    // TODO: add more validation
    fn validate_manifest(&self, handle: FileHandle) -> ManifestResult<()> {
        // Are there any environments?
        if self.environments().is_empty() {
            return Err(ManifestError::with_file(handle.path())(
                ManifestErrorKind::NoEnvironments,
            ));
        }

        // Do all dep-replacements have valid environments?
        for (env, entries) in self.dep_replacements.iter() {
            if !self.environments().contains_key(env) {
                let span = entries
                    .first_key_value()
                    .expect("dep-replacements.<env> only exists if it has a dep")
                    .1
                    .span();

                let loc = Location::new(handle, span);

                return Err(ManifestError::with_span(&loc)(
                    ManifestErrorKind::MissingEnvironment { env: env.clone() },
                ));
            }
        }

        Ok(())
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

impl ManifestError {
    pub(crate) fn with_file<T: Into<ManifestErrorKind>>(
        path: impl AsRef<Path>,
    ) -> impl Fn(T) -> Self {
        move |e| ManifestError {
            kind: Box::new(e.into()),
            location: ErrorLocation::WholeFile(path.as_ref().to_path_buf()),
        }
    }

    fn with_span<T: Into<ManifestErrorKind>>(loc: &Location) -> impl Fn(T) -> Self {
        move |e| ManifestError {
            kind: Box::new(e.into()),
            location: ErrorLocation::AtLoc(loc.clone()),
        }
    }

    fn from_toml(file: FileHandle) -> impl Fn(toml_edit::de::Error) -> Self {
        move |e| {
            let location = e
                .span()
                .map(|span| ErrorLocation::AtLoc(Location::new(file, span)))
                .unwrap_or(ErrorLocation::WholeFile(file.path().to_path_buf()));
            ManifestError {
                kind: Box::new(e.into()),
                location,
            }
        }
    }

    /// Convert this error into a codespan Diagnostic
    pub fn to_diagnostic(&self) -> Diagnostic<FileHandle> {
        match &self.location {
            ErrorLocation::WholeFile(path) => {
                Diagnostic::error().with_message(format!("Error while loading `{path:?}`: {self}"))
            }
            ErrorLocation::AtLoc(loc) => Diagnostic::error()
                .with_message(format!("Error while loading `{:?}`", loc.file()))
                .with_labels(vec![Label::primary(loc.file(), loc.span().clone())])
                .with_notes(vec![self.to_string()]),
        }
    }
}
