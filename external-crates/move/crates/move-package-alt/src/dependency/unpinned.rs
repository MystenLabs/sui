use std::collections::BTreeMap;

use move_package::source_package::parsed_manifest::OnChainInfo;
use serde::Deserialize;
use thiserror::Error;
use tracing::debug;

use crate::{
    dependency::{
        external,
        git::{self, UnpinnedGitDependency},
    },
    errors::{FileHandle, Location, PackageResult},
    git::errors::GitError,
    package::{PackagePath, manifest::ManifestResult},
    schema::{
        Address, DefaultDependency, EnvironmentID, EnvironmentName, ExternalDependency,
        LocalDependency, LockfileDependencyInfo, ManifestDependencyInfo, ManifestGitDependency,
        ReplacementDependency, ResolverDependencyInfo,
    },
};

use super::{DependencySet, external::ResolverError, pinned::PinnedDependency};

pub type PinResult<T> = Result<T, PinError>;

impl Dependency<Parsed> {}
