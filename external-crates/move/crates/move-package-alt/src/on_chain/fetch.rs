// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fetches on-chain packages and generates synthetic manifests and publication files.
//!
//! On-chain packages are cached at `~/.move/on-chain/<chain_id>/<address>/`. The cache
//! contains:
//! - `bytecode/<module_name>.mv` — serialized `CompiledModule` bytecode (the same format
//!   the compiler writes to `build/<pkg>/bytecode_modules/`; we use a separate directory
//!   to avoid clobbering the original bytecode if the package is recompiled)
//! - `Move.toml` — a generated manifest with `name = "self"`, with environments and
//!   dep-replacements added incrementally as the package is used from different contexts
//! - `Published.toml` — publication metadata, also updated incrementally per environment

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
        DefaultDependency, Environment, ManifestDependencyInfo, OnChainAddress, PackageName,
        ParsedManifest, ParsedPublishedFile, Publication, PublishAddresses, PublishedID,
        RenderToml, ReplacementDependency,
    },
};

use super::{OnChainError, OnChainResult};

/// The package name used for all generated on-chain package manifests.
const ON_CHAIN_PACKAGE_NAME: &str = "self";

/// Fetch an on-chain package, writing bytecode and generating manifest/publication files
/// in the cache directory. If the bytecode is already cached, skips the network fetch but
/// still ensures the manifest and Published.toml include the current environment.
///
/// The generated `Move.toml` looks like:
/// ```toml
/// [package]
/// name = "self"
///
/// [environments]
/// mainnet = "35834a8a"
///
/// [dep-replacements.mainnet.onchain_0x0000...0002]
/// on-chain = "0x0000...0002"
/// ```
///
/// Environments and dep-replacements are added incrementally: fetching the same package
/// for a second environment adds a new entry without disturbing existing ones. This does
/// not trigger repinning for other environments because the manifest digest is computed
/// per-environment from the `CombinedDependency` list.
pub(crate) async fn fetch_onchain<F: MoveFlavor>(
    address: &PublishedID,
    env: &Environment,
    config: &PackageConfig<F>,
) -> OnChainResult<PathBuf> {
    let cache_dir = cache_dir_for(&config.chain_id, address);
    let bytecode_dir = cache_dir.join("bytecode");

    let _lock = PackageSystemLock::new_for_onchain(&config.chain_id, address)?;

    // Skip network fetch if bytecode is already cached
    let data = if bytecode_dir.exists() && fs::read_dir(&bytecode_dir).is_ok_and(|d| d.count() > 0)
    {
        None
    } else {
        let data = config
            .flavor
            .fetch_onchain_package(address)
            .await
            .map_err(|source| OnChainError::FetchFailed {
                address: address.clone(),
                source,
            })?;
        write_bytecode(&cache_dir, &data.modules)?;
        Some(data)
    };

    update_manifest(&cache_dir, address, env, data.as_ref())?;
    update_published::<F>(&cache_dir, address, env, data.as_ref())?;

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
fn write_bytecode(cache_dir: &Path, modules: &BTreeMap<String, Vec<u8>>) -> OnChainResult<()> {
    let bytecode_dir = cache_dir.join("bytecode");
    fs::create_dir_all(&bytecode_dir).map_err(|source| OnChainError::CacheWriteFailed {
        path: bytecode_dir.clone(),
        source,
    })?;

    for (name, bytes) in modules {
        let path = bytecode_dir.join(format!("{name}.mv"));
        fs::write(&path, bytes).map_err(|source| OnChainError::CacheWriteFailed {
            path: path.clone(),
            source,
        })?;
    }

    Ok(())
}

/// Generate or update the `Move.toml` in the cache directory. Adds the current
/// environment and dep-replacements for on-chain dependencies from the linkage table.
/// If the manifest already exists and contains this environment, it is not modified.
fn update_manifest(
    cache_dir: &Path,
    address: &PublishedID,
    env: &Environment,
    data: Option<&OnChainPackageData>,
) -> OnChainResult<()> {
    let manifest_path = cache_dir.join(SourcePackageLayout::Manifest.location_str());

    let mut manifest: ParsedManifest = if manifest_path.exists() {
        let content = fs::read_to_string(&manifest_path).map_err(|source| {
            OnChainError::CacheWriteFailed {
                path: manifest_path.clone(),
                source,
            }
        })?;
        toml_edit::de::from_str(&content).map_err(|source| OnChainError::ManifestParseFailed {
            path: manifest_path.clone(),
            source,
        })?
    } else {
        fs::create_dir_all(cache_dir).map_err(|source| OnChainError::CacheWriteFailed {
            path: cache_dir.to_path_buf(),
            source,
        })?;
        new_manifest()
    };

    // Check if this environment is already present
    let env_already_present = manifest
        .environments
        .keys()
        .any(|k| k.as_ref() == env.name());

    if env_already_present {
        return Ok(());
    }

    // Add environment
    manifest.environments.insert(
        Spanned::new(0..1, env.name().to_string()),
        Spanned::new(0..1, env.id().to_string()),
    );

    // Add dep-replacements from linkage table
    if let Some(data) = data {
        let mut env_replacements = BTreeMap::new();
        for (original_id, linked_address) in &data.dependencies {
            let dep_name =
                PackageName::new(format!("onchain_{original_id}")).expect("valid identifier");
            let replacement = ReplacementDependency {
                dependency: Some(DefaultDependency {
                    dependency_info: ManifestDependencyInfo::OnChainAt(OnChainAddress {
                        on_chain: linked_address.clone(),
                    }),
                    is_override: false,
                    rename_from: None,
                    modes: None,
                }),
                addresses: None,
                use_environment: None,
            };
            env_replacements.insert(dep_name, Spanned::new(0..1, replacement));
        }
        manifest
            .dep_replacements
            .insert(env.name().to_string(), env_replacements);
    }

    // Serialize and write
    let doc = toml_edit::ser::to_document(&manifest).map_err(|source| {
        OnChainError::ManifestSerializeFailed {
            address: address.clone(),
            source,
        }
    })?;
    fs::write(&manifest_path, doc.to_string()).map_err(|source| {
        OnChainError::CacheWriteFailed {
            path: manifest_path,
            source,
        }
    })?;

    Ok(())
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

/// Generate or update `Published.toml` in the cache directory. Adds a publication entry
/// for the current environment if not already present.
fn update_published<F: MoveFlavor>(
    cache_dir: &Path,
    address: &PublishedID,
    env: &Environment,
    data: Option<&OnChainPackageData>,
) -> OnChainResult<()> {
    let pub_path = cache_dir.join("Published.toml");

    let mut pubfile: ParsedPublishedFile<F> = if pub_path.exists() {
        let content =
            fs::read_to_string(&pub_path).map_err(|source| OnChainError::CacheWriteFailed {
                path: pub_path.clone(),
                source,
            })?;
        toml_edit::de::from_str(&content).map_err(|source| OnChainError::ManifestParseFailed {
            path: pub_path.clone(),
            source,
        })?
    } else {
        ParsedPublishedFile::default()
    };

    // Skip if this environment is already present
    if pubfile.published.contains_key(env.name()) {
        return Ok(());
    }

    // We need the on-chain data to create a publication entry
    let Some(data) = data else {
        return Ok(());
    };

    pubfile.published.insert(
        env.name().to_string(),
        Publication {
            chain_id: env.id().to_string(),
            addresses: PublishAddresses {
                published_at: address.clone(),
                original_id: data.original_id.clone(),
            },
            version: data.version,
            metadata: F::PublishedMetadata::default(),
        },
    );

    let rendered = pubfile.render_as_toml();
    fs::write(&pub_path, rendered).map_err(|source| OnChainError::CacheWriteFailed {
        path: pub_path,
        source,
    })?;

    Ok(())
}
