// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fetches on-chain packages and generates synthetic manifests and publication files.
//!
//! On-chain packages are cached at `~/.move/on-chain/<chain_id>/<address>/`. The cache
//! contains:
//! - `bytecode/<module_name>.mv` — serialized `CompiledModule` bytecode (the same format
//!   the compiler writes to `build/<pkg>/bytecode_modules/`; we use a separate directory
//!   to avoid clobbering the original bytecode if the package is recompiled)
//! - `Move.toml` — a generated manifest with `name = "self"` and a single fixed
//!   environment (`_on_chain = "<chain_id>"`)
//! - `Published.toml` — publication metadata for the `_on_chain` environment
//!
//! We use a fixed environment name ([`ON_CHAIN_ENV_NAME`]) because environment names are
//! per-package: different packages in the graph may use different names for the same
//! network. On-chain packages don't need environment flexibility since they're tied to a
//! specific chain. The parent package's `use_environment` is set to this fixed name during
//! combining (see [`crate::dependency::combine`]).

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde_spanned::Spanned;

use crate::{
    flavor::{MoveFlavor, OnChainPackageData},
    package::{
        layout::SourcePackageLayout, package_loader::PackageConfig, package_lock::PackageSystemLock,
    },
    schema::{
        DefaultDependency, ManifestDependencyInfo, OnChainAddress, PackageName, ParsedManifest,
        ParsedPublishedFile, Publication, PublishAddresses, PublishedID, RenderToml,
        ReplacementDependency,
    },
};

use super::{OnChainError, OnChainResult};

/// The package name used for all generated on-chain package manifests.
const ON_CHAIN_PACKAGE_NAME: &str = "self";

/// The fixed environment name used in generated on-chain package manifests. On-chain
/// packages are tied to a specific chain, so they use a single environment rather than
/// mirroring the caller's environment name.
pub(crate) const ON_CHAIN_ENV_NAME: &str = "_on_chain";

/// Fetch an on-chain package, writing bytecode and generating manifest/publication files
/// in the cache directory. If the cache already exists, returns immediately.
///
/// The generated `Move.toml` looks like:
/// ```toml
/// [package]
/// name = "self"
/// implicit-dependencies = false
///
/// [environments]
/// _on_chain = "<chain_id>"
///
/// [dep-replacements._on_chain.onchain_0x0000...0002]
/// on-chain = "0x0000...0002"
/// override = true
/// ```
///
/// # Known limitation: system packages
///
/// The linkage table entries are currently all turned into on-chain dep-replacements. This
/// includes entries for system packages (e.g. `0x1`, `0x2` on Sui), which are mutable and
/// should instead be resolved as system deps. Since almost all on-chain packages depend on
/// system packages, this means the generated manifest will redundantly fetch system packages
/// on-chain rather than using the source versions already in the graph.
///
/// The general problem is that a package may be reachable both as an on-chain dep (from the
/// linkage table) and as a source dep (from system deps or the user's manifest). These need
/// to be deduplicated, likely in a post-processing step after the full graph is built.
// TODO: filter system packages from the linkage table or deduplicate in the graph builder.
pub(crate) async fn fetch_onchain<F: MoveFlavor>(
    address: &PublishedID,
    config: &PackageConfig<F>,
) -> OnChainResult<PathBuf> {
    let cache_dir = cache_dir_for(&config.chain_id, address);
    let manifest_path = cache_dir.join(SourcePackageLayout::Manifest.location_str());

    let _lock = PackageSystemLock::new_for_onchain(&config.chain_id, address)?;

    // Skip if the cache is already populated
    if manifest_path.exists() {
        return Ok(cache_dir);
    }

    let data = config
        .flavor
        .fetch_onchain_package(address)
        .await
        .map_err(|source| OnChainError::Fetch {
            address: address.clone(),
            source,
        })?;

    write_bytecode(&cache_dir, &data.modules);
    write_manifest(&cache_dir, &manifest_path, &config.chain_id, &data);
    write_published::<F>(&cache_dir, address, &config.chain_id, &data);

    Ok(cache_dir)
}

/// Return the cache directory for an on-chain package.
fn cache_dir_for(chain_id: &str, address: &PublishedID) -> PathBuf {
    PathBuf::from(move_command_line_common::env::MOVE_HOME.as_str())
        .join("on-chain")
        .join(chain_id)
        .join(address.to_string())
}

/// Write serialized `CompiledModule` bytecode to `<cache_dir>/bytecode/<name>.mv`.
fn write_bytecode(cache_dir: &Path, modules: &BTreeMap<String, Vec<u8>>) {
    let bytecode_dir = cache_dir.join("bytecode");
    fs::create_dir_all(&bytecode_dir).expect("can create on-chain package cache directory");

    for (name, bytes) in modules {
        let path = bytecode_dir.join(format!("{name}.mv"));
        fs::write(&path, bytes).expect("can write module bytecode to cache");
    }
}

/// Generate the `Move.toml` for an on-chain package.
fn write_manifest(
    cache_dir: &Path,
    manifest_path: &Path,
    chain_id: &str,
    data: &OnChainPackageData,
) {
    fs::create_dir_all(cache_dir).expect("can create on-chain package cache directory");

    let mut manifest = new_manifest();

    manifest.environments.insert(
        Spanned::new(0..1, ON_CHAIN_ENV_NAME.to_string()),
        Spanned::new(0..1, chain_id.to_string()),
    );

    let env_replacements = dep_replacements_for(&data.dependencies);
    if !env_replacements.is_empty() {
        manifest
            .dep_replacements
            .insert(ON_CHAIN_ENV_NAME.to_string(), env_replacements);
    }

    let doc = toml_edit::ser::to_document(&manifest).expect("can serialize generated manifest");
    fs::write(manifest_path, doc.to_string()).expect("can write generated manifest to cache");
}

/// Build dep-replacement entries from a linkage table.
fn dep_replacements_for(
    dependencies: &BTreeMap<crate::schema::OriginalID, PublishedID>,
) -> BTreeMap<PackageName, Spanned<ReplacementDependency>> {
    dependencies
        .iter()
        .map(|(original_id, linked_address)| {
            let dep_name =
                PackageName::new(format!("onchain_{original_id}")).expect("valid identifier");
            (
                dep_name,
                Spanned::new(0..1, on_chain_replacement(linked_address)),
            )
        })
        .collect()
}

/// Create a `ReplacementDependency` for an on-chain address.
fn on_chain_replacement(address: &PublishedID) -> ReplacementDependency {
    ReplacementDependency {
        dependency: Some(DefaultDependency {
            dependency_info: ManifestDependencyInfo::OnChainAt(OnChainAddress {
                on_chain: address.clone(),
            }),
            is_override: true,
            rename_from: None,
            modes: None,
        }),
        addresses: None,
        use_environment: None,
    }
}

/// Create a new empty `ParsedManifest` for an on-chain package.
fn new_manifest() -> ParsedManifest {
    ParsedManifest {
        package: crate::schema::PackageMetadata {
            name: Spanned::new(
                0..1,
                PackageName::new(ON_CHAIN_PACKAGE_NAME).expect("valid identifier"),
            ),
            edition: None,
            implicit_dependencies: false,
            unrecognized_fields: BTreeMap::new(),
        },
        environments: BTreeMap::new(),
        dependencies: BTreeMap::new(),
        dep_replacements: BTreeMap::new(),
        legacy_data: None,
    }
}

/// Generate `Published.toml` for an on-chain package.
fn write_published<F: MoveFlavor>(
    cache_dir: &Path,
    address: &PublishedID,
    chain_id: &str,
    data: &OnChainPackageData,
) {
    let pub_path = cache_dir.join("Published.toml");

    let mut pubfile = ParsedPublishedFile::<F>::default();

    pubfile.published.insert(
        ON_CHAIN_ENV_NAME.to_string(),
        Publication {
            chain_id: chain_id.to_string(),
            addresses: PublishAddresses {
                published_at: address.clone(),
                original_id: data.original_id.clone(),
            },
            version: data.version,
            metadata: F::PublishedMetadata::default(),
        },
    );

    let rendered = pubfile.render_as_toml();
    fs::write(&pub_path, rendered).expect("can write Published.toml to cache");
}
