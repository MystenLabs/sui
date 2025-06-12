// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::{self, Debug, Display, Formatter},
    ops::Range,
    path::{Path, PathBuf},
};

use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFiles,
    term::{
        self,
        termcolor::{ColorChoice, StandardStream},
    },
};

use derive_where::derive_where;
use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;
use thiserror::Error;
use tracing::{debug, info};

use crate::{
    dependency::{Dependency, Parsed, unpinned::UnpinnedDependency},
    errors::{FileHandle, Located, Location, PackageResult, TheFile},
    schema::{
        self, Address, DefaultDependency, EnvironmentID, EnvironmentName, PackageName,
        ReplacementDependency,
    },
};

use super::{paths::PackagePath, *};
use sha2::{Digest as ShaDigest, Sha256};

// TODO: replace this with something more strongly typed
type Digest = String;

#[derive(Error, Debug)]
#[error("Error in {:?}: {kind}", handle.path())]
pub struct ManifestError {
    pub kind: ManifestErrorKind,
    pub handle: FileHandle,
    pub span: Option<Range<usize>>,
}

#[derive(Error, Debug)]
pub enum ManifestErrorKind {
    #[error("package name cannot be empty")]
    EmptyPackageName,
    #[error("unsupported edition '{edition}', expected one of '{valid}'")]
    InvalidEdition { edition: String, valid: String },
    #[error("externally resolved dependencies must have exactly one resolver field")]
    BadExternalDependency,

    #[error(transparent)]
    ParseError(#[from] toml_edit::TomlError),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

pub type ManifestResult<T> = Result<T, ManifestError>;

/// The in-memory representation of a manifest file
#[derive(Debug)]
pub struct Manifest {
    metadata: schema::PackageMetadata,
    environments: BTreeMap<EnvironmentName, EnvironmentID>,

    // invariant: `dependencies` contains an entry for every environment
    dependencies: BTreeMap<EnvironmentName, BTreeMap<PackageName, Dependency<Parsed>>>,

    /// The file that this manifest was parsed from
    file_id: FileHandle,

    /// The SHA-256 hash of the manifest file
    digest: Digest,
}

impl Manifest {
    /// Read the manifest file in `path`, returning a [`Manifest`].
    // TODO: PackageResult is probably wrong here
    pub fn load(path: PackagePath) -> PackageResult<Self> {
        debug!("Reading manifest from {:?}", path.path());

        let file_id = FileHandle::new(path.manifest_path())?;
        let manifest = toml_edit::de::from_str::<schema::ParsedManifest>(file_id.source());

        let schema::ParsedManifest {
            package,
            environments,
            dependencies,
            mut dep_replacements,
        } = manifest?;

        // merge dependencies
        let mut deps_for_envs: BTreeMap<EnvironmentName, _> = BTreeMap::new();

        for (env, env_id) in environments.iter() {
            let full_deps_for_env = map_zip(
                dependencies.clone(),
                dep_replacements.remove(env.as_ref()).unwrap_or_default(),
                |_, default, replacement| {
                    combine_deps(file_id, env, env_id.as_ref(), default, replacement)
                },
            )?;

            deps_for_envs.insert(env.as_ref().clone(), full_deps_for_env);
        }

        Ok(Self {
            metadata: package,
            environments: environments
                .into_iter()
                .map(|(k, v)| (k.into_inner(), v.into_inner()))
                .collect(),
            dependencies: deps_for_envs,
            file_id,
            digest: format!("{:X}", Sha256::digest(file_id.source().as_bytes())),
        })
    }

    /// Output a default manifest for a package named `name` to the file identified by `[path]`
    // TODO: maybe this belongs closer to the new command?
    pub fn write_template(path: impl AsRef<Path>, name: &PackageName) -> PackageResult<()> {
        std::fs::write(
            path,
            r###"
            "###,
        )?;

        Ok(())
    }

    /// Return the package name
    pub fn package_name(&self) -> &PackageName {
        self.metadata.name.as_ref()
    }

    /// Return the set of dependencies for the environment `env` (constructed by merging the
    /// `[dependencies]` and the `[dep-replacements]` sections
    pub fn deps_for_env(
        &self,
        env: EnvironmentName,
    ) -> Option<&BTreeMap<PackageName, Dependency<Parsed>>> {
        self.dependencies.get(&env)
    }

    /// Return the `[environments]` table
    pub fn environments(&self) -> &BTreeMap<EnvironmentName, EnvironmentID> {
        &self.environments
    }

    /// The SHA 256 Digest of the manifest file
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
}

/// Produce a new map `m` containing the union of the keys of `m1` and `m2`, with `m[k]` given by
/// `f(m1.get(k), m2.get(k))`
///
/// `f(_, None, None)` is never called
///
/// Example:
/// ```
/// fn main() {
///     let m1 = BTreeMap::from([("a", 1), ("b", 2)]);
///     let m2 = BTreeMap::from([("b", 2), ("c", 3)]);
///
///     let zipped = map_zip(m1, m2, |_k, v1, v2| v1.unwrap_or_default() + v2.unwrap_or_default());
///
///     let expected = BTreeMap::from([("a", 1), ("b", 4), ("c", 3)]);
///
///     assert_eq!(zipped, expected);
/// }
/// ```
fn map_zip<K: Ord, V1, V2, V, E, F: Fn(&K, Option<V1>, Option<V2>) -> Result<V, E>>(
    mut m1: BTreeMap<K, V1>,
    mut m2: BTreeMap<K, V2>,
    f: F,
) -> Result<BTreeMap<K, V>, E> {
    let mut result: BTreeMap<K, V> = BTreeMap::new();

    for (k, v1) in m1.into_iter() {
        let v = f(&k, Some(v1), m2.remove(&k))?;
        result.insert(k, v);
    }

    for (k, v2) in m2.into_iter() {
        let v = f(&k, None, Some(v2))?;
        result.insert(k, v);
    }

    Ok(result)
}

/// Helper function to combine the default dep and replacement for a given package and environment.
fn combine_deps(
    file_id: FileHandle,
    env: &Spanned<EnvironmentName>,
    source_env: &EnvironmentID,
    default: Option<Spanned<DefaultDependency>>,
    replacement: Option<Spanned<ReplacementDependency>>,
) -> ManifestResult<Dependency<Parsed>> {
    let env = env.as_ref().clone();

    match (default, replacement) {
        (Some(default), None) => Ok(Dependency::from_default(
            file_id,
            env,
            source_env.clone(),
            default.into_inner(),
        )),
        (None, Some(replacement)) => {
            Dependency::from_replacement(file_id, env, source_env.clone(), replacement.into_inner())
        }
        (Some(default), Some(replacement)) => Dependency::from_default_with_replacement(
            file_id,
            env,
            source_env.clone(),
            default.into_inner(),
            replacement.into_inner(),
        ),
        (None, None) => panic!("map_zip never calls f(None,None)"),
    }
}

impl ManifestError {
    /// Convert this error into a codespan Diagnostic
    pub fn to_diagnostic(&self) -> Diagnostic<usize> {
        let (file_id, span) = self.span_info();
        Diagnostic::error()
            .with_message(self.kind.to_string())
            .with_labels(vec![Label::primary(file_id, span.unwrap_or_default())])
    }

    /// Get the file ID and span for this error
    fn span_info(&self) -> (usize, Option<Range<usize>>) {
        let mut files = SimpleFiles::new();
        let file_id = files.add(self.handle.path().to_string_lossy(), self.handle.source());
        (file_id, self.span.clone())
    }

    /// Emit this error to stderr
    pub fn emit(&self) -> Result<(), codespan_reporting::files::Error> {
        let mut files = SimpleFiles::new();
        let file_id = files.add(self.handle.path().to_string_lossy(), self.handle.source());

        let writer = StandardStream::stderr(ColorChoice::Always);
        let config = term::Config {
            display_style: term::DisplayStyle::Rich,
            chars: term::Chars::ascii(),
            ..Default::default()
        };

        let diagnostic = self.to_diagnostic();
        let e = term::emit(&mut writer.lock(), &config, &files, &diagnostic);
        e
    }
}
