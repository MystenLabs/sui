// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fetches on-chain packages and generates synthetic manifests and publication files.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, anyhow};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::{
    flavor::{MoveFlavor, OnChainPackageData},
    package::{package_loader::PackageConfig, package_lock::PackageSystemLock},
    schema::{
        Environment, OriginalID, ParsedPublishedFile, Publication, PublishAddresses, PublishedID,
        RenderToml,
    },
};

/// Fetch an on-chain package and generate its manifest and publication files. Returns the path
/// to the generated package directory in the cache.
pub(crate) async fn fetch_onchain<F: MoveFlavor>(
    address: &PublishedID,
    env: &Environment,
    config: &PackageConfig<F>,
) -> anyhow::Result<PathBuf> {
    let cache_dir = cache_dir_for(&config.chain_id, address);

    let _lock = PackageSystemLock::new_for_onchain(&config.chain_id, address)
        .map_err(|e| anyhow!("failed to acquire lock for on-chain package {address}: {e}"))?;

    let data = config.flavor.fetch_onchain_package(address).await?;

    write_bytecode(&cache_dir, &data.modules)?;
    update_manifest(&cache_dir, env, &data.dependencies)?;
    update_published::<F>(&cache_dir, address, env, &data)?;

    Ok(cache_dir)
}

/// Return the cache directory for an on-chain package.
fn cache_dir_for(chain_id: &str, address: &PublishedID) -> PathBuf {
    PathBuf::from(move_command_line_common::env::MOVE_HOME.as_str())
        .join("on-chain")
        .join(chain_id)
        .join(address.to_string())
}

/// Write module bytecode files to `<cache_dir>/bytecode/<name>.mv`.
fn write_bytecode(cache_dir: &Path, modules: &BTreeMap<String, Vec<u8>>) -> anyhow::Result<()> {
    let bytecode_dir = cache_dir.join("bytecode");
    fs::create_dir_all(&bytecode_dir)
        .with_context(|| format!("creating bytecode dir: {}", bytecode_dir.display()))?;

    for (name, bytes) in modules {
        let path = bytecode_dir.join(format!("{name}.mv"));
        fs::write(&path, bytes)
            .with_context(|| format!("writing module bytecode: {}", path.display()))?;
    }

    Ok(())
}

/// Generate or update `Move.toml` in the cache directory. Adds the environment and
/// dep-replacements for on-chain dependencies from the linkage table.
fn update_manifest(
    cache_dir: &Path,
    env: &Environment,
    dependencies: &BTreeMap<OriginalID, PublishedID>,
) -> anyhow::Result<()> {
    let manifest_path = cache_dir.join("Move.toml");

    let mut doc = if manifest_path.exists() {
        let content = fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        content
            .parse::<DocumentMut>()
            .with_context(|| format!("parsing {}", manifest_path.display()))?
    } else {
        fs::create_dir_all(cache_dir)
            .with_context(|| format!("creating cache dir: {}", cache_dir.display()))?;
        let mut doc = DocumentMut::new();
        let mut package = Table::new();
        package.insert("name", value("self"));
        doc.insert("package", Item::Table(package));
        doc
    };

    // Add environment entry
    if !doc.contains_key("environments") {
        doc.insert("environments", Item::Table(Table::new()));
    }
    doc["environments"][env.name()] = value(env.id());

    // Add dep-replacements for this environment
    if !dependencies.is_empty() {
        if !doc.contains_key("dep-replacements") {
            doc.insert("dep-replacements", Item::Table(Table::new()));
        }
        let replacements = &mut doc["dep-replacements"];

        let mut env_table = Table::new();
        for (original_id, linked_address) in dependencies {
            let dep_name = format!("onchain_{original_id}");
            let mut dep_table = Table::new();
            dep_table.insert("on-chain", value(linked_address.to_string()));
            env_table.insert(&dep_name, Item::Table(dep_table));
        }
        replacements[env.name()] = Item::Table(env_table);
    }

    fs::write(&manifest_path, doc.to_string())
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    Ok(())
}

/// Generate or update `Published.toml` in the cache directory.
fn update_published<F: MoveFlavor>(
    cache_dir: &Path,
    address: &PublishedID,
    env: &Environment,
    data: &OnChainPackageData,
) -> anyhow::Result<()> {
    let pub_path = cache_dir.join("Published.toml");

    let mut pubfile: ParsedPublishedFile<F> = if pub_path.exists() {
        let content = fs::read_to_string(&pub_path)
            .with_context(|| format!("reading {}", pub_path.display()))?;
        toml_edit::de::from_str(&content)
            .with_context(|| format!("parsing {}", pub_path.display()))?
    } else {
        ParsedPublishedFile::default()
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
    fs::write(&pub_path, rendered).with_context(|| format!("writing {}", pub_path.display()))?;

    Ok(())
}
