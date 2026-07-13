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

    #[error("Failed to download sui toolchain version {version}: {message}")]
    BinaryDownload { version: String, message: String },

    #[error("The build command failed.\nCommand: {command}\n--- stderr ---\n{stderr}")]
    BuildSubprocess { command: String, stderr: String },

    // Advice appended to the errors above (see `toolchain_suggestion`).
    #[error("{0}")]
    ToolchainSuggestion(String),

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

/// Advice to append to a toolchain download or build failure. A full sweep of mainnet releases found
/// a small, fixed set that cannot be used for verification; those get a targeted hint naming a nearby
/// working release, and anything else gets the generic advice to try another version.
pub(crate) fn toolchain_suggestion(version: &str) -> String {
    let parsed = || -> Option<(u32, u32, u32)> {
        let mut parts = version.split('.');
        let mut next = || parts.next()?.parse::<u32>().ok();
        Some((next()?, next()?, next()?))
    };

    match parsed() {
        // No release publishes a binary for this platform at v1.8.1 or earlier.
        Some(v) if v <= (1, 8, 1) => {
            "No sui release publishes a binary for this platform at v1.8.1 or earlier. \
             Pass --toolchain-version with v1.9.0 or later."
        }
        // v1.64.1 pins a framework revision that no longer exists.
        Some((1, 64, 1)) => {
            "sui v1.64.1 pins a framework revision that is no longer available. Compiler output \
             rarely changes between releases, so pass --toolchain-version with an adjacent release \
             such as 1.63.4 or 1.65.2."
        }
        _ => {
            "Pass --toolchain-version to rebuild with a different toolchain; compiler output rarely \
             changes between releases, so an adjacent one usually works."
        }
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::toolchain_suggestion;

    #[test]
    fn targets_known_unusable_versions() {
        // No binary at v1.8.1 and earlier.
        assert!(toolchain_suggestion("1.8.1").contains("v1.9.0 or later"));
        assert!(toolchain_suggestion("1.5.0").contains("v1.9.0 or later"));
        // v1.64.1's framework is gone; adjacent releases are named.
        assert!(toolchain_suggestion("1.64.1").contains("1.63.4 or 1.65.2"));
        // Nearby-but-fine and unparseable versions get the generic advice.
        assert!(toolchain_suggestion("1.64.2").contains("adjacent one usually works"));
        assert!(toolchain_suggestion("1.50.0").contains("adjacent one usually works"));
        assert!(toolchain_suggestion("nightly").contains("adjacent one usually works"));
    }
}
