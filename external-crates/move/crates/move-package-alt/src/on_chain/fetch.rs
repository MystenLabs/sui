// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fetches on-chain packages and generates synthetic manifests and publication files.
//!
//! On-chain packages are cached at `~/.move/on-chain/<chain_id>/<address>/`. The cache
//! contains:
//! - `bytecode/<module_name>.mv` — serialized `CompiledModule` bytecode (the same format
//!   the compiler writes to `build/<pkg>/bytecode_modules/`; we use a separate directory
//!   to avoid clobbering the original bytecode if the package is recompiled)
//! - `Move.toml` — a generated manifest with `name = "_onchain_package"` and a single fixed
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

use anyhow::Context;
use serde_spanned::Spanned;

use crate::{
    flavor::{MoveFlavor, OnChainPackageData},
    package::{
        layout::SourcePackageLayout, package_loader::PackageConfig, package_lock::PackageSystemLock,
    },
    schema::{
        PackageName, ParsedManifest, ParsedPublishedFile, Publication, PublishAddresses,
        PublishedID, RenderToml,
    },
};

use super::{OnChainError, OnChainResult};

/// The package name used for all generated on-chain package manifests.
const ON_CHAIN_PACKAGE_NAME: &str = "_onchain_package";

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
/// name = "_onchain_package"
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
/// The flavor is responsible for classifying each linkage table entry — for example,
/// `SuiFlavor` returns system deps for known system packages and on-chain deps for the rest.
pub(crate) async fn fetch_onchain<F: MoveFlavor>(
    address: &PublishedID,
    config: &PackageConfig<F>,
) -> OnChainResult<PathBuf> {
    let cache_dir = cache_dir_for(&config.chain_id, address);
    let published_path = cache_dir.join("Published.toml");

    let _lock = PackageSystemLock::new_for_onchain(&config.chain_id, address)?;

    // Published.toml is the last file written, so its existence indicates a complete cache.
    if published_path.exists() {
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

    // Write cache files, cleaning up on I/O failure to avoid a partially-populated cache.
    let result = write_cache::<F>(&cache_dir, &config.chain_id, address, &data);
    if result.is_err() {
        let _ = fs::remove_dir_all(&cache_dir);
    }
    result.map_err(|source| OnChainError::Fetch {
        address: address.clone(),
        source,
    })?;

    Ok(cache_dir)
}

/// Return the cache directory for an on-chain package.
// TODO(DVX-2127): use config.move_home instead of hardcoded MOVE_HOME
fn cache_dir_for(chain_id: &str, address: &PublishedID) -> PathBuf {
    PathBuf::from(move_command_line_common::env::MOVE_HOME.as_str())
        .join("on-chain")
        .join(chain_id)
        .join(address.to_string())
}

/// Write all cache files for an on-chain package. Published.toml is written last so that
/// its presence indicates a complete cache.
fn write_cache<F: MoveFlavor>(
    cache_dir: &Path,
    chain_id: &str,
    address: &PublishedID,
    data: &OnChainPackageData,
) -> anyhow::Result<()> {
    write_bytecode(cache_dir, &data.modules)?;
    write_source(cache_dir);
    write_manifest(cache_dir, chain_id, data)?;
    write_published::<F>(cache_dir, address, chain_id, data)?;
    Ok(())
}

/// Write serialized `CompiledModule` bytecode to `<cache_dir>/bytecode/<name>.mv`.
fn write_bytecode(cache_dir: &Path, modules: &BTreeMap<String, Vec<u8>>) -> anyhow::Result<()> {
    let bytecode_dir = cache_dir.join("bytecode");
    fs::create_dir_all(&bytecode_dir)
        .with_context(|| format!("creating cache directory {}", bytecode_dir.display()))?;

    for (name, bytes) in modules {
        let path = bytecode_dir.join(format!("{name}.mv"));
        fs::write(&path, bytes)
            .with_context(|| format!("writing module bytecode to {}", path.display()))?;
    }
    Ok(())
}

/// Generate stub Move source files from bytecode. Currently a no-op; will be implemented
/// when stub generation (PR #26555) is integrated.
// TODO: implement stub source generation from bytecode
fn write_source(_cache_dir: &Path) {}

/// Generate the `Move.toml` for an on-chain package. The dependencies are written as
/// dep-replacements exactly as the flavor provided them.
fn write_manifest(
    cache_dir: &Path,
    chain_id: &str,
    data: &OnChainPackageData,
) -> anyhow::Result<()> {
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("creating cache directory {}", cache_dir.display()))?;

    let manifest_path = cache_dir.join(SourcePackageLayout::Manifest.location_str());

    let mut manifest = new_manifest();
    manifest.environments.insert(
        Spanned::new(0..1, ON_CHAIN_ENV_NAME.to_string()),
        Spanned::new(0..1, chain_id.to_string()),
    );

    if !data.dependencies.is_empty() {
        let env_replacements = data
            .dependencies
            .iter()
            .map(|(name, replacement)| (name.clone(), Spanned::new(0..1, replacement.clone())))
            .collect();
        manifest
            .dep_replacements
            .insert(ON_CHAIN_ENV_NAME.to_string(), env_replacements);
    }

    fs::write(&manifest_path, manifest.render_as_toml())
        .with_context(|| format!("writing manifest to {}", manifest_path.display()))?;
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

/// Generate `Published.toml` for an on-chain package. This is written last so that its
/// presence indicates a complete cache.
fn write_published<F: MoveFlavor>(
    cache_dir: &Path,
    address: &PublishedID,
    chain_id: &str,
    data: &OnChainPackageData,
) -> anyhow::Result<()> {
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
    fs::write(&pub_path, rendered)
        .with_context(|| format!("writing Published.toml to {}", pub_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use insta::assert_snapshot;
    use tempfile::tempdir;
    use test_log::test;

    use super::*;
    use crate::{
        flavor::{Vanilla, vanilla::DEFAULT_ENV_NAME},
        schema::{OriginalID, PublishedID, ReplacementDependency},
        test_utils::graph_builder::TestPackageGraph,
    };

    /// Helper to create test on-chain package data with on-chain dep-replacements.
    fn test_data(deps: &[(&str, &str)]) -> OnChainPackageData {
        OnChainPackageData {
            modules: BTreeMap::from([
                ("my_module".to_string(), vec![0xDE, 0xAD]),
                ("other_module".to_string(), vec![0xBE, 0xEF]),
            ]),
            dependencies: deps
                .iter()
                .map(|(name, toml_str)| {
                    let name = PackageName::new(*name).expect("valid identifier");
                    let dep: ReplacementDependency = toml::from_str(toml_str).expect("valid TOML");
                    (name, dep)
                })
                .collect(),
            original_id: OriginalID::from(0xABCD_u16),
            version: 3,
        }
    }

    /// Writing bytecode creates files at `bytecode/<name>.mv`.
    #[test]
    fn bytecode_files() {
        let dir = tempdir().unwrap();
        let data = test_data(&[]);

        write_bytecode(dir.path(), &data.modules).unwrap();

        let my_mod = dir.path().join("bytecode/my_module.mv");
        let other_mod = dir.path().join("bytecode/other_module.mv");
        assert_eq!(fs::read(&my_mod).unwrap(), vec![0xDE, 0xAD]);
        assert_eq!(fs::read(&other_mod).unwrap(), vec![0xBE, 0xEF]);
    }

    /// Generated Move.toml for a package with no linkage table dependencies.
    #[test]
    fn manifest_no_deps() {
        let dir = tempdir().unwrap();
        let data = test_data(&[]);

        write_manifest(dir.path(), "test_chain", &data).unwrap();

        let manifest_path = dir.path().join("Move.toml");
        assert_snapshot!(fs::read_to_string(&manifest_path).unwrap(), @r#"
        package = { name = "_onchain_package", implicit-dependencies = false }
        environments = { _on_chain = "test_chain" }
        dependencies = {}
        dep-replacements = {}
        "#);
    }

    /// Generated Move.toml includes dep-replacements from the linkage table.
    #[test]
    fn manifest_with_deps() {
        let dir = tempdir().unwrap();
        let data = test_data(&[
            ("dep_a", "on-chain = \"0x2\""),
            ("dep_b", "on-chain = \"0x33\""),
        ]);

        write_manifest(dir.path(), "test_chain", &data).unwrap();

        let manifest_path = dir.path().join("Move.toml");
        assert_snapshot!(fs::read_to_string(&manifest_path).unwrap(), @r#"
        package = { name = "_onchain_package", implicit-dependencies = false }
        environments = { _on_chain = "test_chain" }
        dependencies = {}

        [dep-replacements]

        [dep-replacements._on_chain]
        onchain_0x0000000000000000000000000000000000000000000000000000000000000002 = { on-chain = "0x0000000000000000000000000000000000000000000000000000000000000002", override = true }
        onchain_0x0000000000000000000000000000000000000000000000000000000000000003 = { on-chain = "0x0000000000000000000000000000000000000000000000000000000000000033", override = true }
        "#);
    }

    /// Generated Move.toml round-trips through parse.
    #[test]
    fn manifest_round_trips() {
        let dir = tempdir().unwrap();
        let data = test_data(&[("dep_a", "on-chain = \"0x2\"")]);

        write_manifest(dir.path(), "test_chain", &data).unwrap();

        let manifest_path = dir.path().join("Move.toml");
        let content = fs::read_to_string(&manifest_path).unwrap();
        let parsed: ParsedManifest = toml_edit::de::from_str(&content).unwrap();

        assert_eq!(parsed.package.name.as_ref().as_str(), ON_CHAIN_PACKAGE_NAME);
        assert!(!parsed.package.implicit_dependencies);
        assert!(
            parsed
                .environments
                .keys()
                .any(|k| k.as_ref() == ON_CHAIN_ENV_NAME)
        );
    }

    /// Generated Published.toml has the expected structure.
    #[test]
    fn published_toml() {
        let dir = tempdir().unwrap();
        let address = PublishedID::from(0x1234_u16);
        let data = test_data(&[]);

        write_published::<Vanilla>(dir.path(), &address, "test_chain", &data).unwrap();

        assert_snapshot!(fs::read_to_string(dir.path().join("Published.toml")).unwrap(), @r#"
        # Generated by Move
        # This file contains metadata about published versions of this package in different environments
        # This file SHOULD be committed to source control

        [published._on_chain]
        chain-id = "test_chain"
        published-at = "0x0000000000000000000000000000000000000000000000000000000000001234"
        original-id = "0x000000000000000000000000000000000000000000000000000000000000abcd"
        version = 3
        "#);
    }

    // NOTE: each test must use unique on-chain addresses to avoid cache collisions
    // when tests run in parallel, since the cache is shared at ~/.move/on-chain/.
    // TODO(DVX-2127): thread move_home through the cache to enable per-test isolation.

    /// Rename-from check is skipped for on-chain deps.
    #[test(tokio::test)]
    async fn rename_from_skipped_for_on_chain() {
        // "my_dep" won't match the generated package name "self", but the
        // rename-from check should be skipped for on-chain deps.
        let scenario = TestPackageGraph::new(["root"])
            .add_on_chain_dep("root", "my_dep", "true", |d| d)
            .add_on_chain_dep(
                "root",
                "my_dep",
                &PublishedID::from(0x1001_u16).to_string(),
                |d| d.in_env(DEFAULT_ENV_NAME),
            )
            .build();

        let err = scenario.root_package_err("root").await;
        assert!(
            !err.contains("rename-from"),
            "should not get rename-from error for on-chain deps, got: {err}"
        );
    }

    /// An on-chain package whose linkage table points to another on-chain package.
    /// Both should be fetched and loaded.
    #[test(tokio::test)]
    async fn transitive_on_chain_dep() {
        let scenario = TestPackageGraph::new(["root"])
            .add_on_chain_dep("root", "dep_a", "true", |d| d)
            .add_on_chain_dep(
                "root",
                "dep_a",
                &PublishedID::from(0x2001_u16).to_string(),
                |d| d.in_env(DEFAULT_ENV_NAME),
            )
            .add_on_chain_pkg(PublishedID::from(0x2001_u16), |pkg| {
                pkg.dep("dep_b", "on-chain = \"0x2002\"")
            })
            .add_on_chain_pkg(PublishedID::from(0x2002_u16), |pkg| pkg)
            .build();

        let root = scenario.root_package("root").await;
        assert!(root.packages().len() >= 3, "expected root + A + B");
    }

    /// When an on-chain dep's linkage table references the same address as a local dep,
    /// they should be deduplicated to a single package in the graph.
    #[test(tokio::test)]
    #[ignore] // TODO(DVX-2126): requires deduplication of on-chain and source deps
    async fn on_chain_overlaps_with_local_dep() {
        let scenario = TestPackageGraph::new(["root"])
            .add_published(
                "local_dep",
                OriginalID::from(0x3002_u16),
                PublishedID::from(0x3002_u16),
            )
            .add_deps([("root", "local_dep")])
            .add_on_chain_dep("root", "dep_a", "true", |d| d)
            .add_on_chain_dep(
                "root",
                "dep_a",
                &PublishedID::from(0x3001_u16).to_string(),
                |d| d.in_env(DEFAULT_ENV_NAME),
            )
            .add_on_chain_pkg(PublishedID::from(0x3001_u16), |pkg| {
                pkg.dep("local_dep", "on-chain = \"0x3002\"")
            })
            .add_on_chain_pkg(PublishedID::from(0x3002_u16), |pkg| pkg)
            .build();

        let root = scenario.root_package("root").await;
        // After deduplication: root + local_dep + on-chain A = 3 packages
        // (local_dep and on-chain 0x3002 should be the same node)
        assert_eq!(
            root.packages().len(),
            3,
            "expected root + local_dep + on-chain A (deduplicated), got {}",
            root.packages().len()
        );
    }
}
