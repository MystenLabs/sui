// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, time::Duration};

use anyhow::{ensure, Context as _};
use move_core_types::ident_str;
use reqwest::Client;
use serde_json::{json, Value};
use simulacrum::Simulacrum;
use sui_indexer_alt_e2e_tests::{find, FullCluster, OffchainClusterConfig};
use sui_indexer_alt_graphql::config::RpcConfig;
use sui_indexer_alt_jsonrpc::config::NameServiceConfig;
use sui_move_build::BuildConfig;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    effects::TransactionEffectsAPI,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{ObjectArg, Transaction, TransactionData},
};
use tokio_util::sync::CancellationToken;

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// Tests happy path for SuiNS resolution (forward lookup).
macro_rules! assert_resolved {
    ($target:expr, $resp:expr) => {
        let resp = $resp;
        let address = resp["data"]["suinsName"]["address"]
            .as_str()
            .expect("result should be string");

        assert_eq!(
            $target,
            address.parse().expect("failed to parse result address"),
            "Expected successful response from GraphQL, got {resp:#?}",
        );
    };
}

/// Tests SuiNS resolution response when the domain doesn't exist or has expired.
macro_rules! assert_not_resolved {
    ($resp:expr) => {
        let resp = $resp;
        assert!(
            resp["data"]["suinsName"].is_null(),
            "Expected null result for expired/invalid domain, got {resp:#?}"
        );
    };
}

/// Tests reverse resolution (address to name).
macro_rules! assert_reverse {
    ($target:expr, $resp:expr) => {
        let resp = $resp;
        let name = resp["data"]["address"]["defaultSuinsName"]
            .as_str()
            .expect("defaultSuinsName should be a string");

        assert_eq!($target, name, "Expected name {}, got {resp:#?}", $target);
    };
}

/// Tests when reverse resolution returns no result.
macro_rules! assert_no_reverse {
    ($resp:expr) => {
        let resp = $resp;
        assert!(
            resp["data"]["address"]["defaultSuinsName"].is_null(),
            "Expected null for defaultSuinsName, got {resp:#?}",
        );
    };
}

/// Test resolving a simple domain name, using both formats.
#[tokio::test]
async fn test_resolve_domain() {
    let mut c = SuiNSCluster::new().await;

    let nft = ObjectID::random();
    let target = SuiAddress::random_for_testing_only();
    c.add_domain(nft, &["sui", "foo"], Some(target), 1000)
        .await
        .expect("Failed to add domain");

    c.cluster.create_checkpoint().await;

    assert_resolved!(target, c.resolve_address("foo.sui").await.unwrap());
    assert_resolved!(target, c.resolve_address("@foo").await.unwrap());
    assert_reverse!("foo.sui", c.resolve_name(target).await.unwrap());

    c.cluster.stopped().await;
}

/// If a domain name exists but has no target, we can't resolve it, but it's not an error.
#[tokio::test]
async fn test_resolve_domain_no_target() {
    let mut c = SuiNSCluster::new().await;

    let nft = ObjectID::random();
    c.add_domain(nft, &["sui", "foo"], None, 1000)
        .await
        .expect("Failed to add domain");

    c.cluster.create_checkpoint().await;

    let resp = c.resolve_address("foo.sui").await.unwrap();
    assert!(resp["data"]["suinsName"].is_null());
    assert!(resp["errors"].is_null());

    c.cluster.stopped().await;
}

/// Set-up a domain with an expiry, and confirm that it exists, then advance time on-chain until it
/// expires and confirm that the GraphQL no longer resolves the domain.
#[tokio::test]
async fn test_resolve_domain_expiry() {
    let mut c = SuiNSCluster::new().await;

    let nft = ObjectID::random();
    let target = SuiAddress::random_for_testing_only();
    let expiry_ms = 1000;
    c.add_domain(nft, &["sui", "foo"], Some(target), expiry_ms)
        .await
        .expect("Failed to add domain");

    c.cluster.create_checkpoint().await;

    assert_resolved!(target, c.resolve_address("foo.sui").await.unwrap());
    assert_reverse!("foo.sui", c.resolve_name(target).await.unwrap());

    // Simulacrum's clock starts at 1, so if we advance by the expiry time, we will go past it.
    c.cluster.advance_clock(Duration::from_millis(expiry_ms));
    c.cluster.create_checkpoint().await;

    assert_not_resolved!(c.resolve_address("foo.sui").await.unwrap());
    assert_no_reverse!(c.resolve_name(target).await.unwrap());

    c.cluster.stopped().await;
}

#[tokio::test]
async fn test_resolve_nonexistent_domain() {
    let mut c = SuiNSCluster::new().await;
    c.cluster.create_checkpoint().await;

    assert_not_resolved!(c.resolve_address("foo.sui").await.unwrap());

    c.cluster.stopped().await;
}

/// Test resolving a valid sub-domain (which requires both the sub-domain and its parent to exist
/// in the registry).
#[tokio::test]
async fn test_resolve_subdomain() {
    let mut c = SuiNSCluster::new().await;

    let nft = ObjectID::random();
    let target = SuiAddress::random_for_testing_only();

    c.add_domain(nft, &["sui", "foo"], None, 1000)
        .await
        .expect("Failed to add parent domain");

    c.add_domain(nft, &["sui", "foo", "bar"], Some(target), 0)
        .await
        .expect("Failed to add subdomain");

    c.cluster.create_checkpoint().await;

    assert_resolved!(target, c.resolve_address("bar.foo.sui").await.unwrap());
    assert_resolved!(target, c.resolve_address("bar@foo").await.unwrap());
    assert_reverse!("bar.foo.sui", c.resolve_name(target).await.unwrap());

    c.cluster.stopped().await;
}

/// Like the parent domain case, but a sub-domain's expiry is controlled by its parent's expiry
#[tokio::test]
async fn test_resolve_subdomain_parent_expiry() {
    let mut c = SuiNSCluster::new().await;

    let nft = ObjectID::random();
    let target = SuiAddress::random_for_testing_only();
    let expiry_ms = 1000;

    c.add_domain(nft, &["sui", "foo"], None, expiry_ms)
        .await
        .expect("Failed to add parent domain");

    c.add_domain(nft, &["sui", "foo", "bar"], Some(target), 0)
        .await
        .expect("Failed to add subdomain");

    c.cluster.create_checkpoint().await;

    assert_resolved!(target, c.resolve_address("bar.foo.sui").await.unwrap());
    assert_reverse!("bar.foo.sui", c.resolve_name(target).await.unwrap());

    c.cluster.advance_clock(Duration::from_millis(expiry_ms));
    c.cluster.create_checkpoint().await;

    assert_not_resolved!(c.resolve_address("bar.foo.sui").await.unwrap());
    assert_no_reverse!(c.resolve_name(target).await.unwrap());

    c.cluster.stopped().await;
}

/// A sub-domain that has its own expiry, in addition to (and before) the parent's expiry.
#[tokio::test]
async fn test_resolve_subdomain_expiry() {
    let mut c = SuiNSCluster::new().await;

    let parent_nft = ObjectID::random();
    let nft = ObjectID::random();
    let target = SuiAddress::random_for_testing_only();
    let parent_expiry_ms = 10000;
    let expiry_ms = 1000;

    c.add_domain(parent_nft, &["sui", "foo"], None, parent_expiry_ms)
        .await
        .expect("Failed to add parent domain");

    c.add_domain(nft, &["sui", "foo", "bar"], Some(target), expiry_ms)
        .await
        .expect("Failed to add subdomain");

    c.cluster.create_checkpoint().await;

    assert_resolved!(target, c.resolve_address("bar.foo.sui").await.unwrap());
    assert_reverse!("bar.foo.sui", c.resolve_name(target).await.unwrap());

    c.cluster.advance_clock(Duration::from_millis(expiry_ms));
    c.cluster.create_checkpoint().await;

    assert_not_resolved!(c.resolve_address("bar.foo.sui").await.unwrap());
    assert_no_reverse!(c.resolve_name(target).await.unwrap());

    c.cluster.stopped().await;
}

/// A sub-domain where the parent domain's NFT is different from the sub-domain's NFT, is
/// considered expired -- its parent has been bought by someone else.
#[tokio::test]
async fn test_resolve_subdomain_bad_parent() {
    let mut c = SuiNSCluster::new().await;

    let nft0 = ObjectID::random();
    let nft1 = ObjectID::random();
    assert_ne!(nft0, nft1, "NFTs should be different");

    let target = SuiAddress::random_for_testing_only();

    c.add_domain(nft0, &["sui", "foo"], None, 1000)
        .await
        .expect("Failed to add parent domain");

    c.add_domain(nft1, &["sui", "foo", "bar"], Some(target), 0)
        .await
        .expect("Failed to add subdomain");

    c.cluster.create_checkpoint().await;

    assert_not_resolved!(c.resolve_address("bar.foo.sui").await.unwrap());
    assert_no_reverse!(c.resolve_name(target).await.unwrap());

    c.cluster.stopped().await;
}

/// The parent domain record does not exist, so the sub-domain is considered expired.
#[tokio::test]
async fn test_resolve_subdomain_no_parent() {
    let mut c = SuiNSCluster::new().await;

    let nft = ObjectID::random();
    let target = SuiAddress::random_for_testing_only();

    c.add_domain(nft, &["sui", "foo", "bar"], Some(target), 0)
        .await
        .expect("Failed to add subdomain");

    c.cluster.create_checkpoint().await;

    assert_not_resolved!(c.resolve_address("bar.foo.sui").await.unwrap());
    assert_no_reverse!(c.resolve_name(target).await.unwrap());

    c.cluster.stopped().await;
}

struct SuiNSCluster {
    cluster: FullCluster,
    config: NameServiceConfig,
    forward_registry: ObjectArg,
    reverse_registry: ObjectArg,
    client: Client,
}

impl SuiNSCluster {
    /// Sets up a full cluster with a mock SuiNS package and registries. GraphQL is configured to read
    /// from these packages and registries to resolve SuiNS names.
    ///
    /// Set-up transactions are run using a burner address that is funded by requesting gas from
    /// the executor.
    async fn new() -> Self {
        // (1) Spin up the simulator to run transactions.
        let mut sim = Simulacrum::new();

        // (2) Compile the mock SuiNS package.
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.extend(["packages", "suins"]);

        let pkg = BuildConfig::new_for_testing()
            .build(&path)
            .expect("Failed to compile package");

        // (3) Create an address and fund it to be able to run transactions.
        let (sender, kp, gas) = sim
            .funded_account(DEFAULT_GAS_BUDGET * 3)
            .expect("Failed to get account");

        // (4) Publish the mock SuiNS package.
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
            sim.reference_gas_price(),
        );

        let (fx, _) = sim
            .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
            .expect("Publish failed");

        let package_address = find::immutable(&fx).expect("Couldn't find package").0;

        // (5) Initialize the forward registry.
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .move_call(
                package_address,
                ident_str!("suins").to_owned(),
                ident_str!("share_forward_registry").to_owned(),
                vec![],
                vec![],
            )
            .unwrap();

        let data = TransactionData::new_programmable(
            sender,
            vec![fx.gas_object().0],
            builder.finish(),
            DEFAULT_GAS_BUDGET,
            sim.reference_gas_price(),
        );

        let (fx, _) = sim
            .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
            .expect("Forward registry initialization failed");

        let registry_id = find::shared(&fx).expect("Couldn't find forward registry").0;
        let forward_registry = ObjectArg::SharedObject {
            id: registry_id,
            initial_shared_version: fx.lamport_version(),
            mutable: true,
        };

        // (6) Initialize the reverse registry.
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .move_call(
                package_address,
                ident_str!("suins").to_owned(),
                ident_str!("share_reverse_registry").to_owned(),
                vec![],
                vec![],
            )
            .unwrap();

        let data = TransactionData::new_programmable(
            sender,
            vec![fx.gas_object().0],
            builder.finish(),
            DEFAULT_GAS_BUDGET,
            sim.reference_gas_price(),
        );

        let (fx, _) = sim
            .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
            .expect("Reverse registry initialization failed");

        let reverse_registry_id = find::shared(&fx).expect("Couldn't find reverse registry").0;
        let reverse_registry = ObjectArg::SharedObject {
            id: reverse_registry_id,
            initial_shared_version: fx.lamport_version(),
            mutable: true,
        };

        // (7) Configure the GraphQL RPC to read from the mock SuiNS package.
        let config = NameServiceConfig {
            package_address: package_address.into(),
            registry_id,
            reverse_registry_id,
        };

        // Note: We need to configure both JSON-RPC and GraphQL with the same name service config
        let graphql_config = RpcConfig {
            name_service: config.clone(),
            ..Default::default()
        };

        // (8) Spin up the rest of the cluster.
        let cluster = FullCluster::new_with_configs(
            sim,
            OffchainClusterConfig {
                graphql_config,
                ..Default::default()
            },
            &prometheus::Registry::new(),
            CancellationToken::new(),
        )
        .await
        .expect("Failed to set-up cluster");

        Self {
            cluster,
            config,
            forward_registry,
            reverse_registry,
            client: Client::new(),
        }
    }

    /// Introduce a new domain to the registry (and the reverse registry, if it has a target
    /// address).
    ///
    /// Transactions are run using a burner address that is funded by requesting gas from the
    /// executor.
    async fn add_domain(
        &mut self,
        nft: ObjectID,
        labels: &[&str],
        target: Option<SuiAddress>,
        expiration_timestamp_ms: u64,
    ) -> anyhow::Result<()> {
        let (sender, kp, gas) = self
            .cluster
            .funded_account(DEFAULT_GAS_BUDGET)
            .expect("failed to get account");

        let mut builder = ProgrammableTransactionBuilder::new();

        let forward_registry = builder.obj(self.forward_registry)?;
        let reverse_registry = builder.obj(self.reverse_registry)?;
        let nft_id = builder.pure(nft)?;
        let labels = builder.pure(labels)?;
        let target = builder.pure(target)?;
        let expiration_timestamp_ms = builder.pure(expiration_timestamp_ms)?;

        builder.programmable_move_call(
            self.config.package_address.into(),
            ident_str!("suins").to_owned(),
            ident_str!("add_domain").to_owned(),
            vec![],
            vec![
                forward_registry,
                reverse_registry,
                nft_id,
                labels,
                target,
                expiration_timestamp_ms,
            ],
        );

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
            .expect("Failed to execute add domain transaction");

        ensure!(fx.status().is_ok(), "add domain transaction failed");

        Ok(())
    }

    /// Send a GraphQL request to the cluster to resolve the given SuiNS name.
    async fn resolve_address(&self, name: &str) -> anyhow::Result<Value> {
        let query = r#"
            query($address: String!) {
                suinsName(address: $address) {
                    address
                }
            }
        "#;

        let variables = json!({
            "address": name,
        });

        let response = self
            .client
            .post(self.cluster.graphql_url())
            .json(&json!({
                "query": query,
                "variables": variables,
            }))
            .send()
            .await
            .context("Request to GraphQL server failed")?;

        let body: Value = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        Ok(body)
    }

    /// Send a GraphQL request to the cluster to resolve the SuiNS name for a given address.
    async fn resolve_name(&self, addr: SuiAddress) -> anyhow::Result<Value> {
        let query = r#"
            query($address: SuiAddress!) {
                address(address: $address) {
                    defaultSuinsName
                }
            }
        "#;

        let variables = json!({
            "address": addr.to_string(),
        });

        let response = self
            .client
            .post(self.cluster.graphql_url())
            .json(&json!({
                "query": query,
                "variables": variables,
            }))
            .send()
            .await
            .context("Request to GraphQL server failed")?;

        let body: Value = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        Ok(body)
    }
}
