// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{ops::Range, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;

// TODO: sensible error type
pub type PackageError = anyhow::Error;

// TODO: sensible error type
pub type PackageResult<T> = anyhow::Result<T>;

// TODO: implement Deref, get deserialization right, etc
// Maybe the right thing is to make the path optional and mutable, then go back and patch it up
// after deserialization? Then the type system doesn't help anymore, but at least we can just
// deserialize immediately without
#[derive(Serialize, Deserialize)]
pub struct Located<T> {
    // TODO: Spanned<T>
    #[serde(flatten)]
    inner: T,

    #[serde(skip)]
    path: Option<PathBuf>,
}
