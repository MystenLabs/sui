// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::{self, Debug, Display, Formatter},
    path::Path,
};

use derive_where::derive_where;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::{
    dependency::DependencySet,
    dependency::{DependencySet, UnpinnedDependencyInfo},
    errors::{FileHandle, Located, ManifestError, ManifestErrorKind, PackageResult, TheFile},
    errors::{FileHandle, Located, ManifestError, ManifestErrorKind, PackageResult, with_file},
    flavor::{MoveFlavor, Vanilla},
    schema::{self, Address, EnvironmentName, ManifestDependency, PackageName},
};

use super::*;
use sha2::{Digest as ShaDigest, Sha256};

// TODO: replace this with something more strongly typed
type Digest = String;

pub struct Manifest {
    inner: schema::Manifest,
    file_id: FileHandle,
}

/// The result of merging the information from the `[dependencies]` section with the
/// `[dep-replacements]` section, specialized to a particular environment. This encapsulates the
/// complete information about the dependency contained in the manifest.
pub struct CombinedDependency {
    dep_info: ManifestDependency,

    use_environment: EnvironmentName,

    is_override: bool,

    published_at: Option<Address>,
}

impl Manifest {
    /// Read the manifest file at the given path, returning a [`Manifest`].
    // TODO: PackagePath?
    pub fn read_from_file(path: impl AsRef<Path>) -> PackageResult<Self> {
        debug!("Reading manifest from {:?}", path.as_ref());

        let (inner, file_id) = TheFile::with_file(&path, toml_edit::de::from_str::<Self>)?;

        Ok(Self { inner, file_id })
    }

    fn write_template(path: impl AsRef<Path>, name: &PackageName) -> PackageResult<()> {
        std::fs::write(
            path,
            r###"
            "###,
        )?;

        Ok(())
    }

    /// Return the dependency set of this manifest, including replacements.
    pub fn dependencies(&self) -> DependencySet<UnpinnedDependencyInfo<F>> {
        let mut deps = DependencySet::new();

        for (name, dep) in &self.dependencies {
            deps.insert(None, name.clone(), dep.dependency_info.clone());
        }

        for (env, replacements) in &self.dep_replacements {
            for (name, dep) in replacements {
                if let Some(dep) = &dep.dependency {
                    deps.insert(Some(env.clone()), name.clone(), dep.dependency_info.clone());
                }
            }
        }
        deps
    }

    pub fn environments(&self) -> &BTreeMap<EnvironmentName, F::EnvironmentID> {
        &self.environments
    }
}

/// Compute a digest of this input data using SHA-256.
pub fn digest(data: &[u8]) -> Digest {
    format!("{:X}", Sha256::digest(data))
}
