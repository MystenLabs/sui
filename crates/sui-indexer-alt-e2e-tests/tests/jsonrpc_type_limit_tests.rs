// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Context;
use move_core_types::{ident_str, language_storage::StructTag};
use reqwest::Client;
use serde_json::{json, Value};
use simulacrum::Simulacrum;
use sui_indexer_alt::config::{IndexerConfig, PipelineLayer};
use sui_indexer_alt_e2e_tests::{find_address_owned, find_immutable, FullCluster};
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_graphql::config::RpcConfig as GraphQlConfig;
use sui_indexer_alt_jsonrpc::config::{PackageResolverLayer, RpcConfig as JsonRpcConfig};
use sui_move_build::BuildConfig;
use sui_types::{
    base_types::ObjectID,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Transaction, TransactionData},
    Identifier, TypeTag,
};
use tokio_util::sync::CancellationToken;

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// We get a successful response if everything is within limits.
#[tokio::test]
async fn test_within_limits() {
    let mut c = TypeLimitCluster::new(PackageResolverLayer::default()).await;

    let object_id = c.create_deep(3).await;
    c.cluster.create_checkpoint().await;

    let result = c.get_object(object_id).await.unwrap();
    assert!(
        result["result"]["data"]["content"].is_object(),
        "Result: {result:#?}"
    );

    c.cluster.stopped().await;
}

/// If we set a limit on how deeply nested some type arguments can be, then trying to fetch an
/// object whose type is that deeply nested is going to cause problems.
#[tokio::test]
async fn test_type_argument_depth() {
    let mut c = TypeLimitCluster::new(PackageResolverLayer {
        max_type_argument_depth: 3,
        ..Default::default()
    })
    .await;

    let object_id = c.create_deep(4).await;
    c.cluster.create_checkpoint().await;

    let result = c.get_object(object_id).await.unwrap();
    let Some(err) = result["error"]["message"].as_str() else {
        panic!("Expected an error, got: {result:#?}");
    };

    assert!(err.contains("Type parameter nesting exceeded"), "{err}");
    c.cluster.stopped().await;
}

/// There is also a limit on how many type parameters a single type can have.
#[tokio::test]
async fn test_type_argument_width() {
    let mut c = TypeLimitCluster::new(PackageResolverLayer {
        max_type_argument_width: 3,
        ..Default::default()
    })
    .await;

    let object_id = c.create_wide().await;
    c.cluster.create_checkpoint().await;

    let result = c.get_object(object_id).await.unwrap();
    let Some(err) = result["error"]["message"].as_str() else {
        panic!("Expected an error, got: {result:#?}");
    };

    assert!(err.contains("Expected at most 3 type parameters"), "{err}");
    c.cluster.stopped().await;
}

/// This limit controls the number of types that need to be loaded to resolve the layout of a type.
#[tokio::test]
async fn test_type_nodes() {
    let mut c = TypeLimitCluster::new(PackageResolverLayer {
        max_type_nodes: 3,
        ..Default::default()
    })
    .await;

    let object_id = c.create_deep(4).await;
    c.cluster.create_checkpoint().await;

    let result = c.get_object(object_id).await.unwrap();
    let Some(err) = result["error"]["message"].as_str() else {
        panic!("Expected an error, got: {result:#?}");
    };

    assert!(
        err.contains("More than 3 struct definitions required to resolve type"),
        "{err}"
    );

    c.cluster.stopped().await;
}

/// This limit controls the depth of the resulting value layout.
#[tokio::test]
async fn test_value_depth() {
    let mut c = TypeLimitCluster::new(PackageResolverLayer {
        max_move_value_depth: 3,
        ..Default::default()
    })
    .await;

    let object_id = c.create_deep(4).await;
    c.cluster.create_checkpoint().await;

    let result = c.get_object(object_id).await.unwrap();
    let Some(err) = result["error"]["message"].as_str() else {
        panic!("Expected an error, got: {result:#?}");
    };

    assert!(
        err.contains("Type layout nesting exceeded limit of 3"),
        "{err}"
    );

    c.cluster.stopped().await;
}

struct TypeLimitCluster {
    cluster: FullCluster,
    package_id: ObjectID,
    client: Client,
}

impl TypeLimitCluster {
    /// Sets up a full cluster and publishes a test package containing types that can exercise type
    /// resolution limits.
    async fn new(package_resolver: PackageResolverLayer) -> Self {
        // (1) Set-up a cluster that indexes object data and sets the given limits up.
        let mut cluster = FullCluster::new_with_configs(
            Simulacrum::new(),
            IndexerArgs::default(),
            IndexerConfig {
                pipeline: PipelineLayer {
                    cp_sequence_numbers: Some(Default::default()),
                    kv_objects: Some(Default::default()),
                    obj_info: Some(Default::default()),
                    obj_versions: Some(Default::default()),
                    sum_packages: Some(Default::default()),
                    ..Default::default()
                },
                ..IndexerConfig::for_test()
            },
            JsonRpcConfig {
                package_resolver: package_resolver.finish(),
                ..JsonRpcConfig::default()
            },
            GraphQlConfig::default(),
            &prometheus::Registry::new(),
            CancellationToken::new(),
        )
        .await
        .expect("Failed to set-up cluster");

        // (2) Compile the test package.
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.extend(["packages", "type_limits"]);

        let pkg = BuildConfig::new_for_testing()
            .build(&path)
            .expect("Failed to compile package");

        // (3) Create an address and fund it to be able to run transactions
        let (sender, kp, gas) = cluster
            .funded_account(DEFAULT_GAS_BUDGET)
            .expect("Failed to get account");

        // (4) Publish the test package
        let mut builder = ProgrammableTransactionBuilder::new();
        let with_unpublished_deps = false;
        builder.publish_immutable(
            pkg.get_package_bytes(with_unpublished_deps),
            pkg.get_dependency_storage_package_ids(),
        );

        let data = TransactionData::new_programmable(
            sender,
            vec![gas],
            builder.finish(),
            DEFAULT_GAS_BUDGET,
            cluster.reference_gas_price(),
        );

        let (fx, _) = cluster
            .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
            .expect("Publish failed");

        let package_id = find_immutable(&fx).expect("Couldn't find package").0;

        Self {
            cluster,
            package_id,
            client: Client::new(),
        }
    }

    /// Run a transaction on the cluster to create a nested instance of the `DeepX` types from the
    /// test package. The number of nestings is determined by the `depth` parameter, and the exact
    /// types used cycle through `Deep0`, `Deep1`, `Deep2`, and `Deep3` in a round-robin fashion to
    /// cause more types to be loaded.
    pub async fn create_deep(&mut self, depth: usize) -> ObjectID {
        let (sender, kp, gas) = self
            .cluster
            .funded_account(DEFAULT_GAS_BUDGET)
            .expect("Failed to get account");

        let mut builder = ProgrammableTransactionBuilder::new();
        let mut arg = builder.pure(42u64).unwrap();
        let mut typ = TypeTag::U64;

        for d in 0..depth {
            arg = builder.programmable_move_call(
                self.package_id,
                ident_str!("type_limits").to_owned(),
                Identifier::new(format!("deep{}", d % 4)).unwrap(),
                vec![typ.clone()],
                vec![arg],
            );

            typ = TypeTag::Struct(Box::new(StructTag {
                address: self.package_id.into(),
                module: ident_str!("type_limits").to_owned(),
                name: Identifier::new(format!("Deep{}", d % 4)).unwrap(),
                type_params: vec![typ.clone()],
            }));
        }

        builder.transfer_args(sender, vec![arg]);

        let data = TransactionData::new_programmable(
            sender,
            vec![gas],
            builder.finish(),
            DEFAULT_GAS_BUDGET,
            self.cluster.reference_gas_price(),
        );

        let (fx, _) = self
            .cluster
            .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
            .expect("Transaction failed");

        find_address_owned(&fx).unwrap().0
    }

    /// Run a transaction on the cluster to create an instance of the `Wide` type from the test
    /// package. All of the `Wide` type's parameters are instantiated to `u64`.
    pub async fn create_wide(&mut self) -> ObjectID {
        let (sender, kp, gas) = self
            .cluster
            .funded_account(DEFAULT_GAS_BUDGET)
            .expect("Failed to get account");

        let mut builder = ProgrammableTransactionBuilder::new();
        let inner = builder.pure(42u64).unwrap();

        let arg = builder.programmable_move_call(
            self.package_id,
            ident_str!("type_limits").to_owned(),
            ident_str!("wide").to_owned(),
            vec![TypeTag::U64; 4],
            vec![inner; 4],
        );

        builder.transfer_args(sender, vec![arg]);

        let data = TransactionData::new_programmable(
            sender,
            vec![gas],
            builder.finish(),
            DEFAULT_GAS_BUDGET,
            self.cluster.reference_gas_price(),
        );

        let (fx, _) = self
            .cluster
            .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
            .expect("Transaction failed");

        find_address_owned(&fx).unwrap().0
    }

    /// Try and fetch the contents of an object from the cluster's RPC.
    async fn get_object(&self, id: ObjectID) -> anyhow::Result<Value> {
        let query = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sui_getObject",
            "params": [
                id.to_string(),
                {
                    "showContent": true,
                },
            ]
        });

        let response = self
            .client
            .post(self.cluster.jsonrpc_url())
            .json(&query)
            .send()
            .await
            .context("Request to JSON-RPC server failed")?;

        let body: Value = response
            .json()
            .await
            .context("Failed to parse JSON-RPC response")?;

        Ok(body)
    }
}
