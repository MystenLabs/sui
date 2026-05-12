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
        PackageName, ParsedManifest, ParsedPublishedFile, Publication, PublishAddresses,
        PublishedID, RenderToml,
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
    let cache_dir = cache_dir_for(&config.move_home, &config.chain_id, address);
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
fn cache_dir_for(move_home: &Path, chain_id: &str, address: &PublishedID) -> PathBuf {
    move_home
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

    // Serialize everything except dep-replacements. ManifestDependencyInfo's derived
    // Serialize doesn't match its custom Deserialize (the serializer produces tagged enum
    // output like `OnChainAt = { ... }` but the deserializer expects flat keys like
    // `on-chain = "0x..."`). We can't fix Serialize because the digest computation depends
    // on the current (incorrect) format. See DVX-2125 for the proper fix.
    // TODO(DVX-2125): once the digest is decoupled from serialization, use RenderToml.
    let mut doc = toml_edit::ser::to_document(&manifest).expect("can serialize generated manifest");

    // Build dep-replacements manually via toml_edit
    if !data.dependencies.is_empty() {
        let mut env_table = toml_edit::Table::new();
        for (original_id, linked_address) in &data.dependencies {
            let dep_name = format!("onchain_{original_id}");
            let mut dep_table = toml_edit::Table::new();
            dep_table.insert("on-chain", toml_edit::value(linked_address.to_string()));
            dep_table.insert("override", toml_edit::value(true));
            env_table.insert(&dep_name, toml_edit::Item::Table(dep_table));
        }
        let mut replacements = toml_edit::Table::new();
        replacements.insert(ON_CHAIN_ENV_NAME, toml_edit::Item::Table(env_table));
        doc.insert("dep-replacements", toml_edit::Item::Table(replacements));
    }

    fs::write(manifest_path, doc.to_string()).expect("can write generated manifest to cache");
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use insta::assert_snapshot;
    use tempfile::tempdir;
    use test_log::test;

    use super::*;
    use crate::{
        flavor::Vanilla,
        schema::{OriginalID, PublishedID},
    };

    /// Helper to create test on-chain package data.
    fn test_data(deps: Vec<(u16, u16)>) -> OnChainPackageData {
        OnChainPackageData {
            modules: BTreeMap::from([
                ("my_module".to_string(), vec![0xDE, 0xAD]),
                ("other_module".to_string(), vec![0xBE, 0xEF]),
            ]),
            dependencies: deps
                .into_iter()
                .map(|(orig, linked)| (OriginalID::from(orig), PublishedID::from(linked)))
                .collect(),
            original_id: OriginalID::from(0xABCD_u16),
            version: 3,
        }
    }

    /// Writing bytecode creates files at `bytecode/<name>.mv`.
    #[test]
    fn bytecode_files() {
        let dir = tempdir().unwrap();
        let data = test_data(vec![]);

        write_bytecode(dir.path(), &data.modules);

        let my_mod = dir.path().join("bytecode/my_module.mv");
        let other_mod = dir.path().join("bytecode/other_module.mv");
        assert_eq!(fs::read(&my_mod).unwrap(), vec![0xDE, 0xAD]);
        assert_eq!(fs::read(&other_mod).unwrap(), vec![0xBE, 0xEF]);
    }

    /// Generated Move.toml for a package with no linkage table dependencies.
    #[test]
    fn manifest_no_deps() {
        let dir = tempdir().unwrap();
        let manifest_path = dir.path().join("Move.toml");
        let data = test_data(vec![]);

        write_manifest(dir.path(), &manifest_path, "test_chain", &data);

        assert_snapshot!(fs::read_to_string(&manifest_path).unwrap(), @r#"
        package = { name = "self", implicit-dependencies = false }
        environments = { _on_chain = "test_chain" }
        dependencies = {}
        dep-replacements = {}
        "#);
    }

    /// Generated Move.toml includes dep-replacements from the linkage table.
    #[test]
    fn manifest_with_deps() {
        let dir = tempdir().unwrap();
        let manifest_path = dir.path().join("Move.toml");
        let data = test_data(vec![(0x2, 0x2), (0x3, 0x33)]);

        write_manifest(dir.path(), &manifest_path, "test_chain", &data);

        assert_snapshot!(fs::read_to_string(&manifest_path).unwrap(), @r#"
        package = { name = "self", implicit-dependencies = false }
        environments = { _on_chain = "test_chain" }
        dependencies = {}

        [dep-replacements]

        [dep-replacements._on_chain]

        [dep-replacements._on_chain.onchain_0x0000000000000000000000000000000000000000000000000000000000000002]
        on-chain = "0x0000000000000000000000000000000000000000000000000000000000000002"
        override = true

        [dep-replacements._on_chain.onchain_0x0000000000000000000000000000000000000000000000000000000000000003]
        on-chain = "0x0000000000000000000000000000000000000000000000000000000000000033"
        override = true
        "#);
    }

    /// Generated Move.toml round-trips through parse.
    #[test]
    fn manifest_round_trips() {
        let dir = tempdir().unwrap();
        let manifest_path = dir.path().join("Move.toml");
        let data = test_data(vec![(0x2, 0x2)]);

        write_manifest(dir.path(), &manifest_path, "test_chain", &data);

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
        let data = test_data(vec![]);

        write_published::<Vanilla>(dir.path(), &address, "test_chain", &data);

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

    /// Rename-from check is skipped for on-chain deps.
    #[test(tokio::test)]
    async fn rename_from_skipped_for_on_chain() {
        use crate::flavor::vanilla::DEFAULT_ENV_NAME;
        use crate::test_utils::graph_builder::TestPackageGraph;

        // "my_dep" won't match the generated package name "self", but the
        // rename-from check should be skipped for on-chain deps.
        let scenario = TestPackageGraph::new(["root"])
            .add_on_chain_dep("root", "my_dep", "true", |d| d)
            .add_on_chain_dep(
                "root",
                "my_dep",
                "0x0000000000000000000000000000000000000000000000000000000000000001",
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
        use crate::flavor::vanilla::DEFAULT_ENV_NAME;
        use crate::test_utils::graph_builder::TestPackageGraph;

        let addr_a = PublishedID::from(0xA_u16);
        let addr_b = PublishedID::from(0xB_u16);

        let scenario = TestPackageGraph::new(["root"])
            .add_on_chain_dep("root", "dep_a", "true", |d| d)
            .add_on_chain_dep("root", "dep_a", &addr_a.to_string(), |d| {
                d.in_env(DEFAULT_ENV_NAME)
            })
            .add_on_chain_pkg(addr_a, |pkg| {
                pkg.dep(OriginalID::from(0xB_u16), addr_b.clone())
            })
            .add_on_chain_pkg(addr_b, |pkg| pkg)
            .build();

        // Should succeed — both packages fetched and loaded
        let root = scenario.root_package("root").await;
        assert!(root.packages().len() >= 3, "expected root + A + B");
    }

    /// An on-chain dep's linkage table references the same address as a local dep
    /// that is published at that address. This creates a duplicate in the graph.
    /// TODO: deduplication should resolve this.
    #[test(tokio::test)]
    async fn on_chain_overlaps_with_local_dep() {
        use crate::flavor::vanilla::DEFAULT_ENV_NAME;
        use crate::test_utils::graph_builder::TestPackageGraph;

        let addr_a = PublishedID::from(0xA_u16);
        let addr_shared = PublishedID::from(0xCC_u16);

        // local_dep is published at 0xCC — same address as A's linkage entry
        let scenario = TestPackageGraph::new(["root"])
            .add_published("local_dep", OriginalID::from(0xCC_u16), addr_shared.clone())
            .add_deps([("root", "local_dep")])
            .add_on_chain_dep("root", "dep_a", "true", |d| d)
            .add_on_chain_dep("root", "dep_a", &addr_a.to_string(), |d| {
                d.in_env(DEFAULT_ENV_NAME)
            })
            .add_on_chain_pkg(addr_a, |pkg| {
                pkg.dep(OriginalID::from(0xCC_u16), addr_shared.clone())
            })
            .add_on_chain_pkg(addr_shared, |pkg| pkg)
            .build();

        // This currently loads both as separate packages (no deduplication).
        // TODO: when deduplication is implemented, this should resolve to a
        // single package and this test should be updated.
        let result = scenario.try_root_package("root", |cfg| cfg).await;
        match result {
            Ok(root) => {
                // Both the local dep and on-chain fetched copy are in the graph
                assert!(
                    root.packages().len() >= 4,
                    "expected root + local_dep + on-chain A + on-chain CC, got {}",
                    root.packages().len()
                );
            }
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    !msg.contains("rename-from"),
                    "unexpected rename-from error: {msg}"
                );
            }
        }
    }
}
