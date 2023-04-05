// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_package::{
    resolution::{
        dependency_cache::DependencyCache, dependency_graph as DG, resolution_graph as RG,
    },
    source_package::manifest_parser as MP,
    BuildConfig,
};
use std::{collections::BTreeMap, path::PathBuf};
use tempfile::tempdir;

#[test]
fn test_additonal_addresses() {
    let path: PathBuf = [
        "tests",
        "test_sources",
        "basic_no_deps_address_not_assigned_with_dev_assignment",
    ]
    .into_iter()
    .collect();

    let pm = MP::parse_move_manifest_from_file(&path).unwrap();

    let mut dependency_cache = DependencyCache::new(/* skip_fetch_latest_git_deps */ true);
    let mut sink = std::io::sink();
    let dg = DG::DependencyGraph::new(&pm, path, &mut dependency_cache, &mut sink).unwrap();

    assert!(RG::ResolvedGraph::resolve(
        dg.clone(),
        BuildConfig {
            install_dir: Some(tempdir().unwrap().path().to_path_buf()),
            additional_named_addresses: BTreeMap::from([(
                "A".to_string(),
                AccountAddress::from_hex_literal("0x1").unwrap()
            )]),
            ..Default::default()
        },
        &mut dependency_cache,
        &mut sink,
    )
    .is_ok());

    assert!(RG::ResolvedGraph::resolve(
        dg,
        BuildConfig {
            install_dir: Some(tempdir().unwrap().path().to_path_buf()),
            ..Default::default()
        },
        &mut dependency_cache,
        &mut sink,
    )
    .is_err());
}

#[test]
fn test_additonal_addresses_already_assigned_same_value() {
    let path: PathBuf = ["tests", "test_sources", "basic_no_deps_address_assigned"]
        .into_iter()
        .collect();

    let pm = MP::parse_move_manifest_from_file(&path).unwrap();

    let mut dependency_cache = DependencyCache::new(/* skip_fetch_latest_git_deps */ true);
    let mut sink = std::io::sink();
    let dg = DG::DependencyGraph::new(&pm, path, &mut dependency_cache, &mut sink).unwrap();

    assert!(RG::ResolvedGraph::resolve(
        dg,
        BuildConfig {
            install_dir: Some(tempdir().unwrap().path().to_path_buf()),
            additional_named_addresses: BTreeMap::from([(
                "A".to_string(),
                AccountAddress::from_hex_literal("0x0").unwrap()
            )]),
            ..Default::default()
        },
        &mut dependency_cache,
        &mut sink,
    )
    .is_ok());
}

#[test]
fn test_additonal_addresses_already_assigned_different_value() {
    let path: PathBuf = ["tests", "test_sources", "basic_no_deps_address_assigned"]
        .into_iter()
        .collect();

    let pm = MP::parse_move_manifest_from_file(&path).unwrap();

    let mut dependency_cache = DependencyCache::new(/* skip_fetch_latest_git_deps */ true);
    let mut sink = std::io::sink();
    let dg = DG::DependencyGraph::new(&pm, path, &mut dependency_cache, &mut sink).unwrap();

    assert!(RG::ResolvedGraph::resolve(
        dg,
        BuildConfig {
            install_dir: Some(tempdir().unwrap().path().to_path_buf()),
            additional_named_addresses: BTreeMap::from([(
                "A".to_string(),
                AccountAddress::from_hex_literal("0x1").unwrap()
            )]),
            ..Default::default()
        },
        &mut dependency_cache,
        &mut sink,
    )
    .is_err());
}
