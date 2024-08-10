// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::benchmark_context::BenchmarkContext;
use crate::mock_account::Account;
use crate::tx_generator::TxGenerator;
use move_package::source_package::manifest_parser::parse_move_manifest_from_file;
use move_symbol_pool::Symbol;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use sui_move_build::{BuildConfig, CompiledPackage};
use sui_test_transaction_builder::{PublishData, TestTransactionBuilder};
use sui_types::base_types::ObjectID;
use sui_types::transaction::{Transaction, DEFAULT_VALIDATOR_GAS_PRICE};
use tracing::info;

pub struct PackagePublishTxGenerator {
    compiled_package: CompiledPackage,
}

impl PackagePublishTxGenerator {
    pub async fn new(ctx: &mut BenchmarkContext, manifest_path: PathBuf) -> Self {
        let manifest = load_manifest_json(&manifest_path);
        let dir = manifest_path.parent().unwrap();
        let PackageDependencyManifest {
            dependencies,
            root_package,
        } = manifest;
        let mut dep_map = BTreeMap::new();
        for dependency in dependencies {
            let Package {
                name,
                path,
                is_source_code,
            } = dependency;

            info!("Publishing dependent package {}", name);
            let target_path = dir.join(&path);
            let module_bytes = if is_source_code {
                let compiled_package = BuildConfig::new_for_testing_replace_addresses(vec![(
                    name.clone(),
                    ObjectID::ZERO,
                )])
                .build(&target_path)
                .unwrap();
                compiled_package.get_package_bytes(false)
            } else {
                let toml = parse_move_manifest_from_file(&target_path.join("Move.toml")).unwrap();
                let package_name = toml.package.name.as_str();
                let module_dir = target_path
                    .join("build")
                    .join(package_name)
                    .join("bytecode_modules");
                let mut all_bytes = Vec::new();
                info!("Loading module bytes from {:?}", module_dir);
                for entry in fs::read_dir(module_dir).unwrap() {
                    let entry = entry.unwrap();
                    let file_path = entry.path();
                    if file_path.extension().and_then(|s| s.to_str()) == Some("mv") {
                        let contents = fs::read(file_path).unwrap();
                        all_bytes.push(contents);
                    }
                }
                all_bytes
            };
            let package_id = ctx
                .publish_package(PublishData::ModuleBytes(module_bytes))
                .await
                .0;
            info!("Published dependent package {}", package_id);
            dep_map.insert(Symbol::from(name), package_id);
        }

        let Package {
            name,
            path,
            is_source_code,
        } = root_package;

        info!("Compiling root package {}", name);
        assert!(
            is_source_code,
            "Only support building root package from source code"
        );

        let target_path = dir.join(path);
        let published_deps = dep_map.clone();

        dep_map.insert(Symbol::from(name), ObjectID::ZERO);
        let mut compiled_package = BuildConfig::new_for_testing_replace_addresses(
            dep_map.into_iter().map(|(k, v)| (k.to_string(), v)),
        )
        .build(&target_path)
        .unwrap();

        compiled_package.dependency_ids.published = published_deps;
        Self { compiled_package }
    }
}

impl TxGenerator for PackagePublishTxGenerator {
    fn generate_tx(&self, account: Account) -> Transaction {
        TestTransactionBuilder::new(
            account.sender,
            account.gas_objects[0],
            DEFAULT_VALIDATOR_GAS_PRICE,
        )
        .publish_with_data(PublishData::CompiledPackage(self.compiled_package.clone()))
        .build_and_sign(account.keypair.as_ref())
    }

    fn name(&self) -> &'static str {
        "PackagePublishTxGenerator"
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct PackageDependencyManifest {
    dependencies: Vec<Package>,
    root_package: Package,
}

#[derive(Serialize, Deserialize, Debug)]
struct Package {
    name: String,
    path: PathBuf,
    is_source_code: bool,
}

fn load_manifest_json(file_path: &PathBuf) -> PackageDependencyManifest {
    let data = fs::read_to_string(file_path)
        .unwrap_or_else(|_| panic!("Unable to read file at: {:?}", file_path));
    let parsed_data: PackageDependencyManifest = serde_json::from_str(&data)
        .unwrap_or_else(|_| panic!("Unable to parse json from file at: {:?}", file_path));

    parsed_data
}
