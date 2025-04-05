// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods for external dependencies (of the form `{ r.<res> = data }`).
//!
//! External dependencies are resolved in each environment as follows. First, all dependencies are
//! grouped by the resolver name (`<res>`). Then, the binary `<res>` is invoked with the command
//! line `<res> --resolve-deps`; an array of requests is passed to the binary
//! on standard input, and the results are read from standard output.
//!
//! An individual request contains a dependency and an optional network. If the network is not
//! present, the resolver should return the "default" resolution, otherwise it should return the
//! appropriate resolution for the given network.
//!
//! An individual response contains the requested dependency
//!
//! For example, when resolving the following manifest:
//! ```toml
//! [environments]
//! a = "chainID1"
//! b = "chainID2"
//!
//! [dependencies]
//! foo = { r.mvr = "@qux/foo", override = true }
//! bar = { r.mvr = [1,2,3] }
//! xxx = { r.yyy = "zzz" }
//! ```
//!
//! The binaries `mvr` and `xxx` will each be invoked with `--resolve-deps`
//! (in the case of dep-overrides, they may be invoked more than once). The input for `mvr` would
//! be:
//! ```toml
//! flavor = "..."
//!
//! requests = [
//!     { argument = "@qux/foo" },
//!     { argument = "@qux/foo", environment-id = "chainID1" },
//!     { argument = "@qux/foo", environment-id = "chainID2" },
//!     { argument = [1,2,3] },
//!     { argument = [1,2,3], environment-id = "chainID1" },
//!     { argument = [1,2,3], environment-id = "chainID2" },
//! ]
//! ```
//! Note that the generic fields (like override = true) are removed, and that the value for the
//! `r.<res>` field can be an arbitrary value.
//!
//! The resolver will respond with an array containing a response for each request. The
//! from package names to internal dependencies, as well as a "Default" table. The return value can
//! also have arrays of errors and warnings; these should be strings.
//!
//! ```toml
//!
//! responses = [
//!     { argument = "@qux/foo", resolved = { git = "..." } },
//!     { argument = "@qux/foo", environment-id = "chainID1", resolved = { git = "..." }, warnings = ["..."] },
//!     { ..., errors = ["..."] },
//!     ...
//! ]
//! ```

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;

use crate::{
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::PackageName,
};

use super::{DependencySet, ManifestDependencyInfo, PinnedDependencyInfo};

/// An external dependency has the form `{ r.<res> = <data> }`. External
/// dependencies are resolved by external resolvers.
#[derive(Serialize, Deserialize, Clone)]
pub struct ExternalDependency {
    /// Should be a table with a single entry; the name of the entry is the resolver binary to run
    /// and the value will be passed to the resolver
    r: toml::Value,
}

impl ExternalDependency {
    /// Invoke the external binaries and deserialize their outputs as dependencies, then pin the
    /// dependencies.
    pub fn resolve<F: MoveFlavor>(
        deps: &DependencySet<ExternalDependency>,
    ) -> PackageResult<DependencySet<PinnedDependencyInfo<F>>> {
        todo!()
    }
}

/// Types for the wire protocol for external resolvers
mod protocol {
    use serde::{Deserialize, Serialize};

    use crate::{dependency::ManifestDependencyInfo, flavor::MoveFlavor};

    #[derive(Serialize, Deserialize)]
    #[serde(bound = "")]
    #[serde(rename = "kebab-case")]
    pub struct Request<F: MoveFlavor> {
        flavor: String,
        requests: Vec<Query<F>>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(bound = "")]
    pub struct Response<F: MoveFlavor> {
        responses: Vec<QueryResponse<F>>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(bound = "")]
    #[serde(rename = "kebab-case")]
    pub struct Query<F: MoveFlavor> {
        argument: toml::Value,

        #[serde(default)]
        environment_id: Option<F::EnvironmentID>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(bound = "")]
    pub enum Result<F: MoveFlavor> {
        Error(String),
        Success {
            #[serde(default)]
            warnings: Vec<String>,
            resolved: ManifestDependencyInfo<F>,
        },
    }

    #[derive(Serialize, Deserialize)]
    #[serde(bound = "")]
    pub struct QueryResponse<F: MoveFlavor> {
        #[serde(flatten)]
        request: Request<F>,

        #[serde(flatten)]
        result: Result<F>,
    }
}
