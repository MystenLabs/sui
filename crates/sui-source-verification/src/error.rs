// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;
use sui_types::base_types::ObjectID;

/// One or more [`Error`]s from a verification run. Comparison errors are collected so that all
/// mismatches are reported together; fatal setup errors are reported singly.
#[derive(Debug, thiserror::Error)]
pub struct AggregateError(pub(crate) Vec<Error>);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    //
    // Fatal setup errors (abort before comparison)
    //
    #[error(
        "The package records no toolchain version. \
         Pass --toolchain-version to specify the version to rebuild it with."
    )]
    ToolchainVersionNotFound,

    #[error(
        "The recorded toolchain sui {version} cannot verify this package: {reason}. \
         Passing --toolchain-version {alternative} may work, since compiler output rarely changes \
         between adjacent releases."
    )]
    UnsupportedToolchain {
        version: String,
        reason: String,
        alternative: String,
    },

    #[error(
        "Failed to download sui toolchain version {version}: {message}\n\
         Pass --toolchain-version to rebuild with a different toolchain."
    )]
    BinaryDownload { version: String, message: String },

    #[error(
        "The build command failed.\nCommand: {command}\n--- stderr ---\n{stderr}\n\
         Pass --toolchain-version to rebuild with a different toolchain."
    )]
    BuildSubprocess { command: String, stderr: String },

    #[error("Could not parse the output of the build command: {message}")]
    BuildOutputParse { message: String },

    #[error("Could not read the on-chain package: {0}")]
    PackageReadFailure(String),

    #[error("Object at {0} is a Move object, not a package")]
    ObjectFoundWhenPackageExpected(ObjectID),

    #[error("On-chain package {0} is empty")]
    EmptyOnChainPackage(AccountAddress),

    #[error("Could not deserialize on-chain module {address}::{module}")]
    OnChainModuleDeserialization {
        address: AccountAddress,
        module: Symbol,
    },

    #[error(
        "The on-chain package's original id ({on_chain}) does not match the one the source records \
         ({recorded}); the package at the recorded address is not the one the source describes."
    )]
    OriginalIdMismatch {
        recorded: AccountAddress,
        on_chain: AccountAddress,
    },

    //
    // Comparison errors (aggregated)
    //
    #[error("Invalid module {name}: {message}")]
    InvalidModule { name: String, message: String },

    #[error("Module {module} bytecode does not match its on-chain version")]
    ModuleBytecodeMismatch { module: Symbol },

    #[error("Module {module} is produced by the source but is not present on-chain")]
    SourceModuleNotOnChain { module: Symbol },

    #[error("Module {module} is present on-chain but is not produced by the source")]
    OnChainModuleNotInSource { module: Symbol },

    #[error(
        "Dependency {original} is linked at a different version: on-chain {on_chain}, source {in_source}"
    )]
    LinkageVersionMismatch {
        original: AccountAddress,
        on_chain: AccountAddress,
        in_source: AccountAddress,
    },

    #[error("Source depends on package {0} which could not be found on-chain")]
    SourceDependencyNotOnChain(AccountAddress),

    #[error(
        "Dependency `{dependency}` is pinned to `{rev}`, which is not a commit hash, so its \
         contents can change over time. This package cannot be rebuilt reproducibly; pin its \
         dependencies to commit hashes."
    )]
    NonReproducibleDependency { dependency: String, rev: String },
}

impl fmt::Display for AggregateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Self(errors) = self;
        match &errors[..] {
            [] => unreachable!("Aggregate error with no errors"),
            [error] => write!(f, "{}", error),
            errors => {
                writeln!(f, "Multiple source verification errors found:")?;
                for error in errors {
                    write!(f, "\n- {}", error)?;
                }
                Ok(())
            }
        }
    }
}

impl From<Error> for AggregateError {
    fn from(error: Error) -> Self {
        Self(vec![error])
    }
}
