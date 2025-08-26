use std::path::Path;

use crate::{
    compatibility::{legacy::LegacyEnvironment, legacy_parser::ParsedLegacyPackage},
    errors::PackageResult,
    flavor::MoveFlavor,
    schema::{ParsedLockfile, ParsedManifest, PublicationFile},
};

use super::paths::PackagePath;

#[derive(Debug)]
pub struct PackageFiles<F: MoveFlavor> {
    pub lockfile: Lockfile,
    pub manifest: Manifest,
    pub published: Option<PublicationFile<F>>,
    pub publocal: Option<PublicationFile<F>>,
}

#[derive(Debug)]
pub enum Lockfile {
    None,
    Legacy(LegacyEnvironment),
    Modern(ParsedLockfile),
}

#[derive(Debug)]
pub enum Manifest {
    Legacy(ParsedLegacyPackage),
    Modern(ParsedManifest),
}

impl<F: MoveFlavor> PackageFiles<F> {
    /// Reads the package-related files from `path`; determines whether they are modern or legacy
    /// files, and returns them.
    pub async fn load(path: &PackagePath) -> PackageResult<Self> {
        Ok(Self {
            lockfile: load_lockfile(path).await?,
            manifest: load_manifest(path).await?,
            published: load_pubfile(&path.publications_path()).await?,
            publocal: load_pubfile(&path.publications_local_path()).await?,
        })
    }
}

/// Read the lockfile from `path` and determine whether it is modern or legacy
async fn load_lockfile(path: &PackagePath) -> PackageResult<Lockfile> {
    todo!()
}

/// Read the manifest from `path` and determine whether it is modern or legacy
async fn load_manifest(path: &PackagePath) -> PackageResult<Manifest> {
    todo!()
}

/// Read the pubfile from the file located at `path`
async fn load_pubfile<F: MoveFlavor>(file: &Path) -> PackageResult<Option<PublicationFile<F>>> {
    todo!()
}
