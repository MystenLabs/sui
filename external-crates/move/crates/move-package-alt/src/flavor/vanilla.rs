// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Defines the [Vanilla] implementation of the [MoveFlavor] trait.
//! This implementation supports git, local, and externally resolved dependencies, and stores no
//! additional metadata in the lockfile.

use std::{marker::PhantomData, path::Path};

use serde::Serialize;

use crate::{
    dependency::{ExternalDependency, GitDependency, LocalDependency},
    errors::PackageError,
};

use super::MoveFlavor;

/// The [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports git,
/// local, and externally resolved dependencies, and stores no additional metadata in the lockfile.
pub struct Vanilla;

#[derive(Serialize)]
pub enum ManifestDependency {
    Git(GitDependency),
    External(ExternalDependency),
    Local(LocalDependency),
}

pub enum InternalDependency {
    Git(GitDependency),
    Local(LocalDependency),
}

#[derive(Serialize)]
pub enum PinnedDependnency {
    Git(GitDependency<PinnedDependnency>),
    Local(LocalDependency),
}

// TODO
#[derive(Serialize)]
pub struct PublishedMetadata;

impl MoveFlavor for Vanilla {
    // TODO
    type PublishedMetadata = PublishedMetadata;

    type ManifestDependency = ManifestDependency;

    type InternalDependency = InternalDependency;

    type PinnedDependency = PinnedDependnency;

    type EnvironmentID = String;

    fn implicit_dependencies(&self, id: Self::EnvironmentID) -> Vec<Self::InternalDependency> {
        todo!()
    }

    fn resolve(
        &self,
        dep: &Self::ManifestDependency,
    ) -> crate::errors::PackageResult<&Self::InternalDependency> {
        todo!()
    }

    fn pin(
        &self,
        dep: &Self::InternalDependency,
    ) -> crate::errors::PackageResult<&Self::PinnedDependency> {
        todo!()
    }

    fn fetch(
        &self,
        dep: &Self::PinnedDependency,
    ) -> crate::errors::PackageResult<std::path::PathBuf> {
        todo!()
    }
}

impl TryFrom<(&Path, toml_edit::Value)> for ManifestDependency {
    type Error = PackageError;

    fn try_from(value: (&Path, toml_edit::Value)) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl TryFrom<(&Path, toml_edit::Value)> for PinnedDependnency {
    type Error = PackageError;

    fn try_from(value: (&Path, toml_edit::Value)) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl TryFrom<(&Path, toml_edit::Value)> for PublishedMetadata {
    type Error = PackageError;

    fn try_from(value: (&Path, toml_edit::Value)) -> Result<Self, Self::Error> {
        todo!()
    }
}
