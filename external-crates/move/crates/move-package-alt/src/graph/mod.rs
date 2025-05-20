use crate::{
    errors::PackageResult,
    package::{Package, PackagePath},
};

struct PackageGraph;

impl PackageGraph {
    /// Try to load a package graph from the lockfile in [path]; check if it is up-to-date (i.e. if
    /// the manifest digests are correct), and if not, rebuild the graph from the manifest
    pub fn load(path: PackagePath) -> PackageResult<Self> {
        todo!()
    }

    pub async fn load_from_manifests(path: PackagePath) -> PackageResult<Self> {
        todo!()
    }

    pub async fn load_from_lockfile(path: PackagePath) -> PackageResult<Self> {
        todo!()
    }
}
