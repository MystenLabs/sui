// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use colored::Colorize;
use move_package_alt::schema::{Environment, Publication};
use sui_package_alt::SuiFlavor;
use sui_rpc_api::Client;
use sui_types::base_types::ObjectID;

pub mod error;

mod binary;
mod build;
mod compare;
mod onchain;
mod pinning;
mod toolchain_version;

pub use binary::ensure_binary;
pub use error::{AggregateError, Error};

/// Verify that the Move source package at `source_path` compiles to the on-chain package described
/// by `publication`, matching both its module bytecode and its linkage.
///
/// The package is rebuilt with the `sui` binary of the publication's recorded toolchain version, or
/// of `toolchain_override` when one is given (downloaded and cached if necessary), against `env`. The
/// resulting `0x0` root address is rewritten to the publication's original id, and the modules and
/// linkage are compared against the package fetched from the publication's published-at address.
/// `client_config` locates the wallet for releases whose build contacts the network.
pub async fn verify_source(
    source_path: &Path,
    publication: &Publication<SuiFlavor>,
    toolchain_override: Option<String>,
    env: &Environment,
    client: &Client,
    client_config: Option<&Path>,
) -> Result<(), AggregateError> {
    let toolchain = resolve_toolchain(
        publication.metadata.toolchain_version.clone(),
        source_path,
        toolchain_override,
    )?;
    check_toolchain_version(&toolchain)?;
    let binary = ensure_binary(&toolchain)?;

    // Verification is attempted even for packages whose dependencies are not pinned to commit
    // hashes; only if that attempt fails is the lack of pinning reported, since it explains why the
    // rebuild could not reproduce what was published. The package is built against the environment
    // it is being verified on.
    let generated = build::dump(&binary, source_path, env.name(), client_config)
        .map_err(|e| explain_unpinned_dependencies(source_path, e.into()))?;

    let published_at = ObjectID::from_address(publication.addresses.published_at.0);
    let original_id = publication.addresses.original_id.0;
    let onchain = onchain::fetch(client, published_at).await?;

    // The package the source claims to be (its recorded original id) must be the one actually at
    // `published_at`, otherwise a source could be verified against an unrelated on-chain package.
    if onchain.original_id != original_id {
        return Err(Error::OriginalIdMismatch {
            recorded: original_id,
            on_chain: onchain.original_id,
        }
        .into());
    }

    compare::check(client, generated, onchain)
        .await
        .map_err(|e| explain_unpinned_dependencies(source_path, e))
}

/// Determine which toolchain version to rebuild with.
///
/// `recorded` is the version from the package's publication metadata, if any. When it is absent, the
/// legacy `Move.lock` is consulted (older packages record the version only there). `override_` is
/// the user's `--toolchain-version`: it is used when nothing is recorded, and otherwise takes
/// precedence with a warning, so a package whose recorded version cannot be built can still be
/// rebuilt with a working one.
fn resolve_toolchain(
    recorded: Option<String>,
    source_path: &Path,
    override_: Option<String>,
) -> Result<String, Error> {
    let recorded = recorded.or_else(|| toolchain_version::legacy_move_lock_version(source_path));

    match (override_, recorded) {
        (Some(override_), Some(recorded)) if override_ != recorded => {
            eprintln!(
                "{} rebuilding with toolchain {} instead of the recorded {}",
                "WARNING".bold().yellow(),
                override_.yellow(),
                recorded.yellow(),
            );
            Ok(override_)
        }
        (Some(version), _) | (None, Some(version)) => Ok(version),
        (None, None) => Err(Error::ToolchainVersionNotFound),
    }
}

/// Append an explanation for each dependency that is not pinned to a commit hash. Such a package
/// resolves its dependencies to whatever they point at now rather than at publish time, which is
/// the usual reason a rebuild neither compiles nor matches.
fn explain_unpinned_dependencies(source_path: &Path, mut error: AggregateError) -> AggregateError {
    for moving in pinning::moving_revisions(source_path) {
        error.0.push(Error::NonReproducibleDependency {
            dependency: moving.dependency,
            rev: moving.rev,
        });
    }
    error
}

/// Fail up front for the handful of releases known not to work for verification, naming a nearby
/// release that does — so a package recording such a toolchain gets a precise, actionable error
/// rather than an opaque download or build failure. Unparseable versions (e.g. a nightly) are let
/// through to be attempted.
fn check_toolchain_version(version: &str) -> Result<(), Error> {
    let parse = || -> Option<(u32, u32, u32)> {
        let mut parts = version.split('.');
        let mut next = || parts.next()?.parse::<u32>().ok();
        Some((next()?, next()?, next()?))
    };
    let Some(v) = parse() else { return Ok(()) };

    let (reason, alternative) = if v <= (1, 8, 1) {
        // No release publishes a binary for this platform this far back.
        (
            "no release publishes a binary for this platform at v1.8.1 or earlier",
            "1.9.0",
        )
    } else if (v.0, v.1) == (1, 64) {
        // v1.64.x pins a framework revision (protocol 108) that is no longer in the repository.
        (
            "v1.64.x pins a framework revision that is no longer available",
            "1.65.2",
        )
    } else {
        return Ok(());
    };

    Err(Error::UnsupportedToolchain {
        version: version.to_string(),
        reason: reason.to_string(),
        alternative: alternative.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::check_toolchain_version;

    fn suggestion(version: &str) -> Option<String> {
        check_toolchain_version(version)
            .err()
            .map(|e| e.to_string())
    }

    #[test]
    fn flags_known_unusable_versions() {
        assert!(
            suggestion("1.8.1")
                .unwrap()
                .contains("--toolchain-version 1.9.0")
        );
        assert!(
            suggestion("1.5.0")
                .unwrap()
                .contains("--toolchain-version 1.9.0")
        );
        assert!(
            suggestion("1.64.1")
                .unwrap()
                .contains("--toolchain-version 1.65.2")
        );
        assert!(
            suggestion("1.64.2")
                .unwrap()
                .contains("--toolchain-version 1.65.2")
        );
        // Usable and unparseable versions are let through.
        assert!(suggestion("1.65.2").is_none());
        assert!(suggestion("1.50.0").is_none());
        assert!(suggestion("nightly").is_none());
    }
}
