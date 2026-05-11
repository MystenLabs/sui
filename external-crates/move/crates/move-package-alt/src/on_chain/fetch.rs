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
//!   dep-replacements added incrementally as the package is used in different environments
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
/// my_env = "some_chain_id"
///
/// [dep-replacements.my_env.onchain_0x0000...0002]
/// on-chain = "0x0000...0002"
/// override = true
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
            .map_err(|source| OnChainError::Fetch {
                address: address.clone(),
                source,
            })?;
        write_bytecode(&cache_dir, &data.modules);
        Some(data)
    };

    update_manifest(&cache_dir, env, &data);
    update_published::<F>(&cache_dir, address, env, &data);

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

/// Generate or update the `Move.toml` in the cache directory. Adds the current
/// environment and dep-replacements for on-chain dependencies from the linkage table.
/// If the manifest already exists and contains this environment, it is not modified.
fn update_manifest(cache_dir: &Path, env: &Environment, data: &Option<OnChainPackageData>) {
    let manifest_path = cache_dir.join(SourcePackageLayout::Manifest.location_str());

    let mut manifest = load_or_create_manifest(cache_dir, &manifest_path);

    // Skip if this environment is already present
    if manifest
        .environments
        .keys()
        .any(|k| k.as_ref() == env.name())
    {
        return;
    }

    // Add environment
    manifest.environments.insert(
        Spanned::new(0..1, env.name().to_string()),
        Spanned::new(0..1, env.id().to_string()),
    );

    // Add dep-replacements from linkage table
    if let Some(data) = data {
        let env_replacements = dep_replacements_for(&data.dependencies);
        manifest
            .dep_replacements
            .insert(env.name().to_string(), env_replacements);
    }

    let doc = toml_edit::ser::to_document(&manifest).expect("can serialize generated manifest");
    fs::write(&manifest_path, doc.to_string()).expect("can write generated manifest to cache");
}

/// Load an existing `ParsedManifest` from disk, or create a new one for an on-chain package.
fn load_or_create_manifest(cache_dir: &Path, manifest_path: &Path) -> ParsedManifest {
    if manifest_path.exists() {
        let content = fs::read_to_string(manifest_path).expect("can read cached manifest");
        toml_edit::de::from_str(&content).expect("can parse cached manifest")
    } else {
        fs::create_dir_all(cache_dir).expect("can create on-chain package cache directory");
        new_manifest()
    }
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

/// Generate or update `Published.toml` in the cache directory. Adds a publication entry
/// for the current environment if not already present.
fn update_published<F: MoveFlavor>(
    cache_dir: &Path,
    address: &PublishedID,
    env: &Environment,
    data: &Option<OnChainPackageData>,
) {
    let pub_path = cache_dir.join("Published.toml");

    let mut pubfile: ParsedPublishedFile<F> = if pub_path.exists() {
        let content = fs::read_to_string(&pub_path).expect("can read cached Published.toml");
        toml_edit::de::from_str(&content).expect("can parse cached Published.toml")
    } else {
        ParsedPublishedFile::default()
    };

    // Skip if this environment is already present
    if pubfile.published.contains_key(env.name()) {
        return;
    }

    // We need the on-chain data to create a publication entry
    let Some(data) = data else {
        return;
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
    fs::write(&pub_path, rendered).expect("can write Published.toml to cache");
}
