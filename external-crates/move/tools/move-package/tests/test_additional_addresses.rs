// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_package::{
    resolution::{dependency_graph as DG, resolution_graph as RG},
    source_package::{manifest_parser as MP, parsed_manifest as PM},
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

    let mut dep_graph_builder = DG::DependencyGraphBuilder::new(
        /* skip_fetch_latest_git_deps */ true,
        std::io::sink(),
    );
    let dg = dep_graph_builder
        .new_graph(&PM::DependencyKind::default(), &pm, path, None, None)
        .unwrap();

    let DG::DependencyGraphBuilder {
        mut dependency_cache,
        mut progress_output,
        ..
    } = dep_graph_builder;

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
        &mut progress_output,
    )
    .is_ok());

    assert!(RG::ResolvedGraph::resolve(
        dg,
        BuildConfig {
            install_dir: Some(tempdir().unwrap().path().to_path_buf()),
            ..Default::default()
        },
        &mut dependency_cache,
        &mut progress_output,
    )
    .is_err());
}

#[test]
fn test_additonal_addresses_already_assigned_same_value() {
    let path: PathBuf = ["tests", "test_sources", "basic_no_deps_address_assigned"]
        .into_iter()
        .collect();

    let pm = MP::parse_move_manifest_from_file(&path).unwrap();

    let mut dep_graph_builder = DG::DependencyGraphBuilder::new(
        /* skip_fetch_latest_git_deps */ true,
        std::io::sink(),
    );
    let dg = dep_graph_builder
        .new_graph(&PM::DependencyKind::default(), &pm, path, None, None)
        .unwrap();

    let DG::DependencyGraphBuilder {
        mut dependency_cache,
        mut progress_output,
        ..
    } = dep_graph_builder;

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
        &mut progress_output,
    )
    .is_ok());
}

#[test]
fn test_additonal_addresses_already_assigned_different_value() {
    let path: PathBuf = ["tests", "test_sources", "basic_no_deps_address_assigned"]
        .into_iter()
        .collect();

    let pm = MP::parse_move_manifest_from_file(&path).unwrap();

    let mut dep_graph_builder = DG::DependencyGraphBuilder::new(
        /* skip_fetch_latest_git_deps */ true,
        std::io::sink(),
    );
    let dg = dep_graph_builder
        .new_graph(&PM::DependencyKind::default(), &pm, path, None, None)
        .unwrap();

    let DG::DependencyGraphBuilder {
        mut dependency_cache,
        mut progress_output,
        ..
    } = dep_graph_builder;

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
        &mut progress_output,
    )
    .is_err());
}
