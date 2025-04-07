// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods for external dependencies (of the form `{ r.<res> = data }`)

use std::{collections::BTreeMap, fmt::Debug, path::Path};

use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;

use crate::{
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
};

use super::PinnedDependencyInfo;

/// An external dependency has the form `{ r.<res> = "<data>" }`; it is resolved by invoking the
/// binary `<res>` (from the `PATH`), and passing `<data>` on the command line. The binary is
/// expected to output a single resolved dependency on the command line.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExternalDependency {
    /// Should be a table with a single entry; the name of the entry is the resolver binary to run
    /// and the value should be the argument passed to the resolver
    r: toml::Value,

    #[serde(flatten)]
    fields: BTreeMap<String, String>,
}

impl ExternalDependency {
    /// Invoke the external binary and deserialize its output as a dependency, then pin the
    /// dependency.
    fn resolve<F: MoveFlavor>(&self) -> PackageResult<PinnedDependencyInfo<F>> {
        todo!()
    }
}
