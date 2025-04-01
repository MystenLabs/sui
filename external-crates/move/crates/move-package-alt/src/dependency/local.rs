// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Types and methods related to local dependencies (of the form `{ local = "<path>" }`)

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;

use crate::errors::Located;

#[derive(Serialize, Deserialize)]
pub struct LocalDependency {
    /// The path on the filesystem, relative to the location of the containing file (which is
    /// stored in the `Located` wrapper)
    path: Located<PathBuf>,
}

impl TryFrom<(&Path, toml_edit::Value)> for LocalDependency {
    type Error = anyhow::Error; // TODO

    fn try_from(value: (&Path, toml_edit::Value)) -> Result<Self, Self::Error> {
        // TODO: just deserialize
        todo!()
    }
}
