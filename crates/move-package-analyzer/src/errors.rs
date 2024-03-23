// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::ObjectID;
use thiserror::Error;

/// Errors that can occur when running the package analyzer.
/// We tend to panic when running into an error however error as `Result` are
/// still useful.
#[derive(Debug, Error)]
pub enum PackageAnalyzerError {
    #[error("Generic error: `{0}`")]
    GenericError(String),
    #[error("Unexpected directory structure for packages dump: `{0}`")]
    BadDirectoryStructure(String),
    #[error("Error reading from DB: `{0}`")]
    DBReadError(String),
    #[error("Cannot load package `{0}`: `{1}`")]
    BadPackage(ObjectID, String),
    #[error("Cannot load config file `{0}`")]
    BadConfig(String),
}
