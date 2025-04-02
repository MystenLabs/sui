use std::marker::PhantomData;

use crate::flavor::MoveFlavor;

pub struct Package<F: MoveFlavor> {
    flavor_phantom: PhantomData<F>,
}

/*
use lockfile::Publication;
use serde::{Deserialize, Serialize};

use crate::flavor::MoveFlavor;

pub struct Package<F: MoveFlavor> {
    // TODO: maybe hold a lock on the lock file? Maybe not if move-analyzer wants to hold on to a
    // Package long term?
    // TODO: manifest: manifest::Manifest,
    lockfiles: lockfile::Lockfiles<F>,
}

impl<F: MoveFlavor> Package<F> {
    /// Load a package from the manifest and lock files in directory [path].
    /// Makes a best effort to translate old-style packages into the current format,
    ///
    /// Fails if [path] does not exist, or if it doesn't contain a manifest
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        todo!()
    }

    /// The path to the root directory of this package. This path is guaranteed to exist
    /// and contain a manifest file.
    pub fn path(&self) -> &Path {
        todo!()
    }

    /// Save the package into the lockfiles in the given path
    pub fn save(&self) -> anyhow::Result<Self> {
        todo!()
    }

    /// Return the metadata for the most recent published version in the given environemtn
    pub fn published_on(&self, id: EnvironmentName) -> Option<Publication<F>> {
        todo!()
    }

    /// Register a published package on the given chain
    pub fn add_published(&mut self, metadata: Publication<F>) -> anyhow::Result<()> {
        todo!()
    }

    /// Load and return all the transitive dependencies of this package
    pub fn transitive_dependencies(&self) -> Vec<Package<F>> {
        todo!()
    }
}
*/
