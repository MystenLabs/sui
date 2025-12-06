// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as IoWrite;
use std::net::SocketAddr;
use std::{fmt::Write, fs::read_dir, path::PathBuf, str, thread, time::Duration};

use std::env;
#[cfg(not(msim))]
use std::str::FromStr;

use expect_test::expect;
use fastcrypto::encoding::{Base64, Encoding};
use move_package_alt_compilation::build_config::BuildConfig as MoveBuildConfig;
use serde_json::json;
use sui::client_commands::{
    GasDataArgs, PaymentArgs, PublishArgs, TestPublishArgs, TxProcessingArgs,
};
use sui::client_ptb::ptb::PTB;
use sui::sui_commands::RpcArgs;
use sui_keys::key_identity::KeyIdentity;
use sui_protocol_config::ProtocolConfig;
use sui_sdk::SuiClient;
use sui_test_transaction_builder::batch_make_transfer_transactions;
use sui_types::object::Owner;
use sui_types::transaction::{
    TEST_ONLY_GAS_UNIT_FOR_GENERIC, TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
    TEST_ONLY_GAS_UNIT_FOR_PUBLISH, TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN,
    TEST_ONLY_GAS_UNIT_FOR_TRANSFER, TransactionData, TransactionDataAPI,
};
use tokio::time::sleep;

use move_package_alt::schema::{Environment, ParsedPublishedFile};
use mysten_common::random_util::TempDir;
use mysten_common::tempdir;
use std::fs::OpenOptions;
use std::path::Path;
use std::{fs, io};
use sui::{
    client_commands::{
        SuiClientCommandResult, SuiClientCommands, SwitchResponse, estimate_gas_budget,
    },
    sui_commands::{SuiCommand, parse_host_port},
};
use sui_config::{
    PersistedConfig, SUI_CLIENT_CONFIG, SUI_FULLNODE_CONFIG, SUI_GENESIS_FILENAME,
    SUI_KEYSTORE_ALIASES_FILENAME, SUI_KEYSTORE_FILENAME, SUI_NETWORK_CONFIG,
};
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    OwnedObjectRef, SuiExecutionStatus, SuiObjectData, SuiObjectDataFilter, SuiObjectDataOptions,
    SuiObjectResponse, SuiObjectResponseQuery, SuiRawData, SuiTransactionBlockDataAPI,
    SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI,
};
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_move_build::BuildConfig;
use sui_package_alt::SuiFlavor;
use sui_sdk::sui_client_config::SuiClientConfig;
use sui_sdk::wallet_context::WalletContext;
use sui_swarm_config::genesis_config::{AccountConfig, GenesisConfig};
use sui_swarm_config::network_config::NetworkConfig;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    Ed25519SuiSignature, Secp256k1SuiSignature, SignatureScheme, SuiKeyPair, SuiSignatureInner,
};
use sui_types::error::SuiObjectResponseError;
use sui_types::move_package::{MovePackage, UpgradeInfo};
use sui_types::{base_types::ObjectID, crypto::get_key_pair, gas_coin::GasCoin};
use test_cluster::{TestCluster, TestClusterBuilder};

const TEST_DATA_DIR: &str = "tests/data/";

struct TreeShakingTest {
    test_cluster: TestCluster,
    client: SuiClient,
    rgp: u64,
    gas_obj_id: ObjectID,
    temp_dir: TempDir,
}

impl TreeShakingTest {
    /// Creates a new TreeShakingTest by copying `tests/data/tree_shaking` into a temporary
    /// directory. and setting up a test cluster
    async fn new() -> Result<Self, anyhow::Error> {
        let mut test_cluster = TestClusterBuilder::new().build().await;
        let rgp = test_cluster.get_reference_gas_price().await;
        let address = test_cluster.get_address_0();
        let context = &mut test_cluster.wallet;
        let client = context.get_client().await?;

        let object_refs = client
            .read_api()
            .get_owned_objects(
                address,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::new()
                        .with_type()
                        .with_owner()
                        .with_previous_transaction(),
                )),
                None,
                None,
            )
            .await?
            .data;

        let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

        // Setup temp directory with test data
        let temp_dir = tempfile::Builder::new().prefix("tree_shaking").tempdir()?;
        std::fs::create_dir_all(temp_dir.path()).unwrap();
        let tests_dir = PathBuf::from(TEST_DATA_DIR);
        let framework_pkgs = PathBuf::from("../sui-framework/packages");
        copy_dir_all(tests_dir, temp_dir.path())?;
        copy_dir_all(framework_pkgs, temp_dir.path().join("system-packages"))?;

        Ok(Self {
            test_cluster,
            client,
            rgp,
            gas_obj_id,
            temp_dir,
        })
    }

    /// Produce a published file in `{package_path}/Move.toml` containing `published_at` and
    /// `upgrade_cap` and additional metadata from `self`.
    async fn create_published_file(
        &self,
        package_path: &Path,
        published_at_id: &ObjectID,
        upgrade_cap: &ObjectID,
    ) -> Result<(), anyhow::Error> {
        let chain_id = self.client.read_api().get_chain_identifier().await?;
        let content = format!(
            r#"# Generated by Move
# This file contains metadata about published versions of this package in different environments
# This file SHOULD be committed to source control

[published.localnet]
chain-id = "{}"
published-at = "{}"
original-id = "{}"
version = 1
toolchain-version = "{}"
build-config = {{ flavor = "sui", edition = "2024" }}
upgrade-capability = "{}""#,
            chain_id,
            published_at_id,
            published_at_id,
            env!("CARGO_PKG_VERSION"),
            upgrade_cap
        );

        std::fs::write(package_path.join("Published.toml"), content)?;

        Ok(())
    }

    fn package_path(&self, name: &str) -> PathBuf {
        self.temp_dir.path().join("tree_shaking").join(name)
    }

    fn ephemeral_path(&self) -> PathBuf {
        self.temp_dir.path().join("Pub.localnet.toml")
    }

    /// Publishes the package named `package_name` in ephemeral mode, and adds the package to the
    /// ephemeral publication file.
    async fn test_publish_package(
        &mut self,
        package_name: &str,
        with_unpublished_dependencies: bool,
    ) -> Result<(ObjectID, ObjectID), anyhow::Error> {
        let pubfile = self.ephemeral_path();

        let result = test_publish_package(
            self.package_path(package_name),
            self.test_cluster.wallet_mut(),
            self.rgp,
            self.gas_obj_id,
            with_unpublished_dependencies,
            Some(pubfile.clone()),
        )
        .await?;

        // TODO: this is a little nasty
        // replace `{root = true}` with `{local = "../{package_name}"}` in the ephemeral file
        let file_contents = std::fs::read_to_string(&pubfile)?;
        let file_contents = file_contents.replace(
            "{ root = true }",
            &format!(r#"{{ local = "../{package_name}" }}"#),
        );
        std::fs::write(&pubfile, file_contents)?;

        Ok(result)
    }

    /// Publishes the package in normal mode. It needs a `localnet = "<chain_id>"` in the Move.toml
    /// file
    async fn publish_package(
        &mut self,
        package_name: &str,
        with_unpublished_dependencies: bool,
    ) -> Result<(ObjectID, ObjectID), anyhow::Error> {
        publish_package(
            self.package_path(package_name),
            self.test_cluster.wallet_mut(),
            self.rgp,
            self.gas_obj_id,
            with_unpublished_dependencies,
        )
        .await
    }

    async fn publish_package_without_tree_shaking(
        &mut self,
        package_name: &str,
        environment: &Environment,
    ) -> (ObjectID, ObjectID) {
        let package_path = self.package_path(package_name);

        let mut build_config = BuildConfig::new_for_testing();
        build_config.config.environment = Some(environment.name.clone());
        build_config.environment = environment.clone();
        let compiled_package = build_config.build_async(&package_path).await.unwrap();

        let context = self.test_cluster.wallet_mut();

        let all_module_bytes =
            compiled_package.get_package_bytes(/* with_unpublished_deps */ false);
        let dependencies = compiled_package.get_dependency_storage_package_ids();
        let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
        let gas_price = context.get_reference_gas_price().await.unwrap();
        let tx_data = TransactionData::new_module(
            sender,
            gas_object,
            all_module_bytes,
            dependencies,
            self.rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
            gas_price,
        );
        let tx = context.sign_transaction(&tx_data).await;

        let response = self
            .test_cluster
            .wallet_mut()
            .execute_transaction_may_fail(tx.clone())
            .await
            .unwrap();

        (
            response.get_new_package_obj().unwrap().0,
            response.get_new_package_upgrade_cap().unwrap().0,
        )
    }

    async fn upgrade_package(
        &mut self,
        package_name: &str,
        upgrade_capability: ObjectID,
    ) -> Result<ObjectID, anyhow::Error> {
        let mut build_config = BuildConfig::new_for_testing().config;
        build_config.lock_file = Some(self.package_path(package_name).join("Move.lock"));
        let resp = SuiClientCommands::Upgrade {
            package_path: self.package_path(package_name),
            upgrade_capability: Some(upgrade_capability),
            build_config,
            skip_dependency_verification: false,
            verify_deps: false,
            skip_verify_compatibility: false,
            with_unpublished_dependencies: false,
            payment: PaymentArgs {
                gas: vec![self.gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(self.rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        }
        .execute(self.test_cluster.wallet_mut())
        .await?;

        let SuiClientCommandResult::TransactionBlock(publish_response) = resp else {
            unreachable!("Invalid response");
        };

        let SuiTransactionBlockEffects::V1(effects) = publish_response.clone().effects.unwrap();
        assert!(effects.status.is_ok());

        let package_a_v1 = effects
            .created()
            .iter()
            .find(|refe| matches!(refe.owner, Owner::Immutable))
            .unwrap();
        Ok(package_a_v1.object_id())
    }

    async fn fetch_linkage_table(&self, pkg: ObjectID) -> BTreeMap<ObjectID, UpgradeInfo> {
        let move_pkg = fetch_move_packages(&self.client, vec![pkg]).await;
        move_pkg.first().unwrap().linkage_table().clone()
    }
}

/// Publishes a package in ephemeral mode and returns the package object id and the upgrade
/// capability object id.
/// Note that this sets the `Move.lock` file to be written to the root of the package path.
async fn test_publish_package(
    package_path: PathBuf,
    context: &mut WalletContext,
    rgp: u64,
    gas_obj_id: ObjectID,
    with_unpublished_dependencies: bool,
    pubfile: Option<PathBuf>,
) -> Result<(ObjectID, ObjectID), anyhow::Error> {
    let mut build_config = BuildConfig::new_for_testing().config;
    let move_lock_path = package_path.clone().join("Move.lock");
    build_config.lock_file = Some(move_lock_path.clone());

    let pubfile_path = pubfile.unwrap_or(package_path.join("localnet.toml"));
    let resp = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path: package_path.clone(),
            build_config: build_config.clone(),
            skip_dependency_verification: false,
            verify_deps: false,
            with_unpublished_dependencies,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(pubfile_path),
    })
    .execute(context)
    .await?;

    let SuiClientCommandResult::TransactionBlock(publish_response) = resp else {
        unreachable!("Invalid response");
    };

    let SuiTransactionBlockEffects::V1(effects) = publish_response.clone().effects.unwrap();

    assert!(effects.status.is_ok());
    let package_a = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::Immutable))
        .unwrap();
    let cap = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::AddressOwner(_)))
        .unwrap();

    Ok((package_a.reference.object_id, cap.reference.object_id))
}

async fn publish_package(
    package_path: PathBuf,
    context: &mut WalletContext,
    rgp: u64,
    gas_obj_id: ObjectID,
    with_unpublished_dependencies: bool,
) -> Result<(ObjectID, ObjectID), anyhow::Error> {
    let mut build_config = BuildConfig::new_for_testing().config;
    let move_lock_path = package_path.clone().join("Move.lock");
    build_config.lock_file = Some(move_lock_path.clone());

    let resp = SuiClientCommands::Publish(PublishArgs {
        package_path: package_path.clone(),
        build_config: build_config.clone(),
        skip_dependency_verification: false,
        verify_deps: false,
        with_unpublished_dependencies,
        payment: PaymentArgs {
            gas: vec![gas_obj_id],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    })
    .execute(context)
    .await?;

    let SuiClientCommandResult::TransactionBlock(publish_response) = resp else {
        unreachable!("Invalid response");
    };

    let SuiTransactionBlockEffects::V1(effects) = publish_response.clone().effects.unwrap();

    assert!(effects.status.is_ok());
    let package_a = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::Immutable))
        .unwrap();
    let cap = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::AddressOwner(_)))
        .unwrap();

    Ok((package_a.reference.object_id, cap.reference.object_id))
}

// Recursively copy a directory and all its contents
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Fetch move packages based on the provided package IDs.
pub async fn fetch_move_packages(
    client: &SuiClient,
    package_ids: Vec<ObjectID>,
) -> Vec<MovePackage> {
    let objects = client
        .read_api()
        .multi_get_object_with_options(package_ids, SuiObjectDataOptions::bcs_lossless())
        .await
        .unwrap();

    objects
        .into_iter()
        .map(|o| {
            let o = o.into_object().unwrap();
            let Some(SuiRawData::Package(p)) = o.bcs else {
                panic!("Expected package");
            };
            p.to_move_package(u64::MAX /* safe as this pkg comes from the network */)
                .unwrap()
        })
        .collect()
}

#[sim_test]
async fn test_genesis() -> Result<(), anyhow::Error> {
    let temp_dir = tempfile::tempdir()?;
    let working_dir = temp_dir.path();
    let config = working_dir.join(SUI_NETWORK_CONFIG);

    // Start network without authorities
    let start = SuiCommand::Start {
        data_ingestion_dir: None,
        config_dir: Some(config),
        force_regenesis: false,
        with_faucet: None,
        fullnode_rpc_port: 9000,
        epoch_duration_ms: None,
        no_full_node: false,
        committee_size: None,
        rpc_args: RpcArgs::for_testing(),
    }
    .execute()
    .await;
    assert!(matches!(start, Err(..)));
    // Genesis
    SuiCommand::Genesis {
        working_dir: Some(working_dir.to_path_buf()),
        write_config: None,
        force: false,
        from_config: None,
        epoch_duration_ms: None,
        benchmark_ips: None,
        with_faucet: false,
        committee_size: None,
    }
    .execute()
    .await?;

    // Get all the new file names
    let files = read_dir(working_dir)?
        .flat_map(|r| r.map(|file| file.file_name().to_str().unwrap().to_owned()))
        .collect::<Vec<_>>();

    assert_eq!(7, files.len());
    assert!(files.contains(&SUI_CLIENT_CONFIG.to_string()));
    assert!(files.contains(&SUI_NETWORK_CONFIG.to_string()));
    assert!(files.contains(&SUI_FULLNODE_CONFIG.to_string()));
    assert!(files.contains(&SUI_GENESIS_FILENAME.to_string()));
    assert!(files.contains(&SUI_KEYSTORE_FILENAME.to_string()));
    assert!(files.contains(&SUI_KEYSTORE_ALIASES_FILENAME.to_string()));

    // Check network config
    let network_conf =
        PersistedConfig::<NetworkConfig>::read(&working_dir.join(SUI_NETWORK_CONFIG))?;
    assert_eq!(1, network_conf.validator_configs().len());

    // Check wallet config
    let wallet_conf =
        PersistedConfig::<SuiClientConfig>::read(&working_dir.join(SUI_CLIENT_CONFIG))?;

    assert!(!wallet_conf.envs.is_empty());

    assert_eq!(5, wallet_conf.keystore.addresses().len());

    // Genesis 2nd time should fail
    let result = SuiCommand::Genesis {
        working_dir: Some(working_dir.to_path_buf()),
        write_config: None,
        force: false,
        from_config: None,
        epoch_duration_ms: None,
        benchmark_ips: None,
        with_faucet: false,
        committee_size: None,
    }
    .execute()
    .await;
    assert!(matches!(result, Err(..)));

    temp_dir.close()?;
    Ok(())
}

#[tokio::test]
async fn test_addresses_command() -> Result<(), anyhow::Error> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut context = test_cluster.wallet;

    // Add 3 accounts
    for _ in 0..3 {
        context
            .config
            .keystore
            .import(None, SuiKeyPair::Ed25519(get_key_pair().1))
            .await?;
    }

    // Print all addresses
    SuiClientCommands::Addresses {
        sort_by_alias: true,
    }
    .execute(&mut context)
    .await
    .unwrap()
    .print(true);

    Ok(())
}

#[sim_test]
async fn test_objects_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let alias = context.config.keystore.get_alias(&address).unwrap();
    // Print objects owned by `address`
    SuiClientCommands::Objects {
        address: Some(KeyIdentity::Address(address)),
    }
    .execute(context)
    .await?
    .print(true);
    // Print objects owned by `address`, passing its alias
    SuiClientCommands::Objects {
        address: Some(KeyIdentity::Alias(alias)),
    }
    .execute(context)
    .await?
    .print(true);
    let client = context.get_client().await?;
    let _object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?;

    Ok(())
}

#[sim_test]
async fn test_ptb_publish_and_complex_arg_resolution() -> Result<(), anyhow::Error> {
    // Publish the package
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let chain_id = client.read_api().get_chain_identifier().await.unwrap();
    let (_tmp, pkg_path) =
        create_temp_dir_with_framework_packages("ptb_complex_args_test_functions", Some(chain_id))?;

    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path: pkg_path.clone(),
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies: false,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await;

    let resp = resp?;
    // Print it out to CLI/logs
    resp.print(true);

    let SuiClientCommandResult::TransactionBlock(response) = resp else {
        unreachable!("Invalid response");
    };

    let SuiTransactionBlockEffects::V1(effects) = response.effects.unwrap();

    assert!(effects.status.is_ok());
    assert_eq!(effects.gas_object().object_id(), gas_obj_id);
    let package = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::Immutable))
        .unwrap();
    let package_id_str = package.reference.object_id;

    let start_call_result = SuiClientCommands::Call {
        package: package.reference.object_id,
        module: "test_module".to_string(),
        function: "new_shared".to_string(),
        type_args: vec![],
        args: vec![],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let shared_id_str =
        if let SuiClientCommandResult::TransactionBlock(response) = start_call_result {
            response.effects.unwrap().created().to_vec()[0]
                .reference
                .object_id
                .to_string()
        } else {
            unreachable!("Invalid response");
        };

    let complex_ptb_string = format!(
        r#"
         --assign p @{package_id_str}
         --assign s @{shared_id_str}
         # Use the shared object by immutable reference first
         --move-call "p::test_module::use_immut" s
         # Now use mutably -- we need to update the mutability of the object
         --move-call "p::test_module::use_mut" s
         # Make sure we handle different more complex pure arguments
         --move-call "p::test_module::use_ascii_string" "'foo bar baz'"
         --move-call "p::test_module::use_utf8_string" "'foo †††˚˚¬¬'"
         --gas-budget 100000000
        "#
    );

    let args = shlex::split(&complex_ptb_string).unwrap();
    sui::client_ptb::ptb::PTB { args: args.clone() }
        .execute(context)
        .await?;

    let delete_object_ptb_string = format!(
        r#"
         --assign p @{package_id_str}
         --assign s @{shared_id_str}
         # Use the shared object by immutable reference first
         --move-call "p::test_module::use_immut" s
         --move-call "p::test_module::delete_shared_object" s
         --gas-budget 100000000
        "#
    );

    let args = shlex::split(&delete_object_ptb_string).unwrap();
    sui::client_ptb::ptb::PTB { args: args.clone() }
        .execute(context)
        .await?;

    Ok(())
}

#[sim_test]
async fn test_ptb_publish() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;

    let chain_id = client.read_api().get_chain_identifier().await.unwrap();
    let (_tmp, pkg_path) = create_temp_dir_with_framework_packages("ptb_publish", Some(chain_id))?;

    let publish_ptb_string = format!(
        r#"
         --move-call sui::tx_context::sender
         --assign sender
         --publish {}
         --assign upgrade_cap
         --transfer-objects "[upgrade_cap]" sender
         --gas-budget 50000000
        "#,
        pkg_path.display()
    );
    let args = shlex::split(&publish_ptb_string).unwrap();
    let res = sui::client_ptb::ptb::PTB { args: args.clone() }
        .execute(context)
        .await;

    res.unwrap();
    Ok(())
}

#[sim_test]
async fn test_custom_genesis() -> Result<(), anyhow::Error> {
    // Create and save genesis config file
    // Create 4 authorities, 1 account with 1 gas object with custom id

    let mut config = GenesisConfig::for_local_testing();
    config.accounts.clear();
    config.accounts.push(AccountConfig {
        address: None,
        gas_amounts: vec![500],
    });
    let mut cluster = TestClusterBuilder::new()
        .set_genesis_config(config)
        .build()
        .await;
    let address = cluster.get_address_0();
    let context = cluster.wallet_mut();

    assert_eq!(1, context.config.keystore.addresses().len());

    // Print objects owned by `address`
    SuiClientCommands::Objects {
        address: Some(KeyIdentity::Address(address)),
    }
    .execute(context)
    .await?
    .print(true);

    Ok(())
}

#[sim_test]
async fn test_object_info_get_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;

    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;

    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Check log output contains all object ids.
    let object_id = object_refs.first().unwrap().object().unwrap().object_id;

    SuiClientCommands::Object {
        id: object_id,
        bcs: false,
    }
    .execute(context)
    .await?
    .print(true);

    SuiClientCommands::Object {
        id: object_id,
        bcs: true,
    }
    .execute(context)
    .await?
    .print(true);

    Ok(())
}

#[sim_test]
async fn test_gas_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let alias = context.config.keystore.get_alias(&address).unwrap();

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content(),
            )),
            None,
            None,
        )
        .await?;

    let object_id = object_refs
        .data
        .first()
        .unwrap()
        .object()
        .unwrap()
        .object_id;
    let object_to_send = object_refs.data.get(1).unwrap().object().unwrap().object_id;

    SuiClientCommands::Gas {
        address: Some(KeyIdentity::Address(address)),
    }
    .execute(context)
    .await?
    .print(true);

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send an object
    SuiClientCommands::Transfer {
        to: KeyIdentity::Address(SuiAddress::random_for_testing_only()),
        object_id: object_to_send,
        payment: PaymentArgs {
            gas: vec![object_id],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    // Fetch gas again, and use the alias instead of the address
    SuiClientCommands::Gas {
        address: Some(KeyIdentity::Alias(alias)),
    }
    .execute(context)
    .await?
    .print(true);

    Ok(())
}

#[sim_test]
async fn test_move_call_args_linter_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address1 = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let address2 = SuiAddress::random_for_testing_only();

    let client = context.get_client().await?;
    // publish the object basics package
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address1,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content(),
            )),
            None,
            None,
        )
        .await?
        .data;
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("move_call_args_linter");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies: false,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await?;

    let package = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert!(
            response.status_ok().unwrap(),
            "Command failed: {:?}",
            response
        );
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        response
            .effects
            .unwrap()
            .created()
            .iter()
            .find(
                |OwnedObjectRef {
                     owner,
                     reference: _,
                 }| matches!(owner, Owner::Immutable),
            )
            .unwrap()
            .reference
            .object_id
    } else {
        unreachable!("Invalid response");
    };

    // Print objects owned by `address1`
    SuiClientCommands::Objects {
        address: Some(KeyIdentity::Address(address1)),
    }
    .execute(context)
    .await?
    .print(true);
    tokio::time::sleep(Duration::from_millis(2000)).await;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address1,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Create an object for address1 using Move call

    // Certain prep work
    // Get a gas object
    let coins: Vec<_> = object_refs
        .iter()
        .filter(|object_ref| object_ref.object().unwrap().is_gas_coin())
        .collect();
    let gas = coins.first().unwrap().object()?.object_id;
    let obj = coins.get(1).unwrap().object()?.object_id;

    // Create the args
    let args = vec![
        SuiJsonValue::new(json!("123"))?,
        SuiJsonValue::new(json!(address1))?,
    ];

    // Test case with no gas specified
    let resp = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "create".to_string(),
        type_args: vec![],
        args,
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;
    resp.print(true);

    // Get the created object
    let created_obj: ObjectID = if let SuiClientCommandResult::TransactionBlock(resp) = resp {
        resp.effects
            .unwrap()
            .created()
            .first()
            .unwrap()
            .reference
            .object_id
    } else {
        panic!();
    };

    // Try a bad argument: decimal
    let args_json = json!([0.3f32, address1]);
    assert!(SuiJsonValue::new(args_json.as_array().unwrap().first().unwrap().clone()).is_err());

    // Try a bad argument: too few args
    let args_json = json!([300usize]);
    let mut args = vec![];
    for a in args_json.as_array().unwrap() {
        args.push(SuiJsonValue::new(a.clone()).unwrap());
    }

    let resp = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "create".to_string(),
        type_args: vec![],
        args: args.to_vec(),
        payment: PaymentArgs { gas: vec![gas] },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await;

    assert!(resp.is_err());

    let err_string = format!("{} ", resp.err().unwrap());
    assert!(err_string.contains("Expected 2 args, found 1"));

    // Try a transfer
    // This should fail due to mismatch of object being sent
    let args = [
        SuiJsonValue::new(json!(obj))?,
        SuiJsonValue::new(json!(address2))?,
    ];

    let resp = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "transfer".to_string(),
        type_args: vec![],
        args: args.to_vec(),
        payment: PaymentArgs { gas: vec![gas] },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await;

    assert!(resp.is_err());

    // Try a transfer with explicitly set gas price.
    // It should fail due to that gas price is below RGP.
    let args = [
        SuiJsonValue::new(json!(created_obj))?,
        SuiJsonValue::new(json!(address2))?,
    ];

    let resp = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "transfer".to_string(),
        type_args: vec![],
        args: args.to_vec(),
        payment: PaymentArgs { gas: vec![gas] },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
            gas_price: Some(1),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await;

    assert!(resp.is_err());
    let err_string = format!("{} ", resp.err().unwrap());
    assert!(
        err_string.contains("Gas price 1 under reference gas price"),
        "Error: {err_string}"
    );

    // FIXME: uncomment once we figure out what is going on with `resolve_and_type_check`
    // let err_string = format!("{} ", resp.err().unwrap());
    // let framework_addr = SUI_FRAMEWORK_ADDRESS.to_hex_literal();
    // let package_addr = package.to_hex_literal();
    // assert!(err_string.contains(&format!("Expected argument of type {package_addr}::object_basics::Object, but found type {framework_addr}::coin::Coin<{framework_addr}::sui::SUI>")));

    // Try a proper transfer
    let args = [
        SuiJsonValue::new(json!(created_obj))?,
        SuiJsonValue::new(json!(address2))?,
    ];

    SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "transfer".to_string(),
        type_args: vec![],
        args: args.to_vec(),
        payment: PaymentArgs { gas: vec![gas] },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    // Try a call with customized gas price.
    let args = vec![
        SuiJsonValue::new(json!("123"))?,
        SuiJsonValue::new(json!(address1))?,
    ];

    let result = SuiClientCommands::Call {
        package,
        module: "object_basics".to_string(),
        function: "create".to_string(),
        type_args: vec![],
        args,
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS),
            gas_price: Some(12345),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::TransactionBlock(txn_response) = result {
        assert_eq!(
            txn_response.transaction.unwrap().data.gas_data().price,
            12345
        );
    } else {
        panic!("Command failed with unexpected result.")
    };

    Ok(())
}

/// Test publish command and the package management's publication to file logic
#[sim_test]
async fn test_package_publish_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let chain_id = client.read_api().get_chain_identifier().await.unwrap();
    let (_tmp, package_path) =
        create_temp_dir_with_framework_packages("dummy_modules_publish", Some(chain_id))?;

    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies: false,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        response
            .effects
            .as_ref()
            .unwrap()
            .created()
            .iter()
            .map(|refe| refe.reference.object_id)
            .collect::<Vec<_>>()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for obj_id in obj_ids {
        get_parsed_object_assert_existence(obj_id, context).await;
    }

    Ok(())
}

#[sim_test]
async fn test_package_management_on_publish_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let chain_id = client.read_api().get_chain_identifier().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let build_config = BuildConfig::new_for_testing().config;

    let (_tmp, pkg_path) =
        create_temp_dir_with_framework_packages("pkg_mgmt_modules_publish", Some(chain_id))?;

    // Publish the package
    let resp = SuiClientCommands::Publish(PublishArgs {
        package_path: pkg_path.clone(),
        build_config: build_config.clone(),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
        payment: PaymentArgs {
            gas: vec![gas_obj_id],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    })
    .execute(context)
    .await?;

    // Get Package ID and version
    let (expect_original_id, expect_version, _) =
        if let SuiClientCommandResult::TransactionBlock(response) = resp {
            assert_eq!(
                response.effects.as_ref().unwrap().gas_object().object_id(),
                gas_obj_id
            );
            response
                .get_new_package_obj()
                .ok_or_else(|| anyhow::anyhow!("No package object response"))?
        } else {
            unreachable!("Invalid response");
        };

    // read the file with published data after publish command successfully executed
    let pubfile_str = std::fs::read_to_string(pkg_path.join("Published.toml"))
        .expect("to read from Published.toml file");
    let parsed: ParsedPublishedFile<SuiFlavor> = toml_edit::de::from_str(&pubfile_str).unwrap();

    let published_addresses = parsed.published.get("localnet").unwrap().addresses.clone();

    assert_eq!(expect_original_id, published_addresses.original_id.0.into());
    assert_eq!(
        expect_original_id,
        published_addresses.published_at.0.into()
    );

    let v = parsed.published.get("localnet").unwrap().version.into();
    assert_eq!(
        expect_version, v,
        "Published package version does not match with publication data written to file, expected {expect_version} but got {v}"
    );

    Ok(())
}

#[sim_test]
async fn test_delete_shared_object() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("sod");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies: false,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await?;

    let owned_obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        let x = response.effects.unwrap();
        x.created().to_vec()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for OwnedObjectRef { reference, .. } in &owned_obj_ids {
        get_parsed_object_assert_existence(reference.object_id, context).await;
    }

    let package_id = owned_obj_ids
        .into_iter()
        .find(|OwnedObjectRef { owner, .. }| owner == &Owner::Immutable)
        .expect("Must find published package ID")
        .reference;

    // Start and then receive the object
    let start_call_result = SuiClientCommands::Call {
        package: package_id.object_id,
        module: "sod".to_string(),
        function: "start".to_string(),
        type_args: vec![],
        args: vec![],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let shared_id = if let SuiClientCommandResult::TransactionBlock(response) = start_call_result {
        response.effects.unwrap().created().to_vec()[0]
            .reference
            .object_id
    } else {
        unreachable!("Invalid response");
    };

    let delete_result = SuiClientCommands::Call {
        package: package_id.object_id,
        module: "sod".to_string(),
        function: "delete".to_string(),
        type_args: vec![],
        args: vec![SuiJsonValue::from_str(&shared_id.to_string()).unwrap()],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::TransactionBlock(response) = delete_result {
        assert!(response.effects.unwrap().into_status().is_ok());
    } else {
        unreachable!("Invalid response");
    };

    Ok(())
}

#[sim_test]
async fn test_receive_argument() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("tto");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies: false,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await?;

    let owned_obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        let x = response.effects.unwrap();
        x.created().to_vec()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for OwnedObjectRef { reference, .. } in &owned_obj_ids {
        get_parsed_object_assert_existence(reference.object_id, context).await;
    }

    let package_id = owned_obj_ids
        .into_iter()
        .find(|OwnedObjectRef { owner, .. }| owner == &Owner::Immutable)
        .expect("Must find published package ID")
        .reference;

    // Start and then receive the object
    let start_call_result = SuiClientCommands::Call {
        package: package_id.object_id,
        module: "tto".to_string(),
        function: "start".to_string(),
        type_args: vec![],
        args: vec![],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let (parent, child) =
        if let SuiClientCommandResult::TransactionBlock(response) = start_call_result {
            let created = response.effects.unwrap().created().to_vec();
            let owners: BTreeSet<ObjectID> = created
                .iter()
                .flat_map(|refe| {
                    refe.owner
                        .get_address_owner_address()
                        .ok()
                        .map(|x| x.into())
                })
                .collect();
            let child = created
                .iter()
                .find(|refe| !owners.contains(&refe.reference.object_id))
                .unwrap();
            let parent = created
                .iter()
                .find(|refe| owners.contains(&refe.reference.object_id))
                .unwrap();
            (parent.reference.clone(), child.reference.clone())
        } else {
            unreachable!("Invalid response");
        };

    let receive_result = SuiClientCommands::Call {
        package: package_id.object_id,
        module: "tto".to_string(),
        function: "receiver".to_string(),
        type_args: vec![],
        args: vec![
            SuiJsonValue::from_str(&parent.object_id.to_string()).unwrap(),
            SuiJsonValue::from_str(&child.object_id.to_string()).unwrap(),
        ],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::TransactionBlock(response) = receive_result {
        assert!(response.effects.unwrap().into_status().is_ok());
    } else {
        unreachable!("Invalid response");
    };

    Ok(())
}

#[sim_test]
async fn test_receive_argument_by_immut_ref() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("tto");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies: false,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await?;

    let owned_obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        let x = response.effects.unwrap();
        x.created().to_vec()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for OwnedObjectRef { reference, .. } in &owned_obj_ids {
        get_parsed_object_assert_existence(reference.object_id, context).await;
    }

    let package_id = owned_obj_ids
        .into_iter()
        .find(|OwnedObjectRef { owner, .. }| owner == &Owner::Immutable)
        .expect("Must find published package ID")
        .reference;

    // Start and then receive the object
    let start_call_result = SuiClientCommands::Call {
        package: package_id.object_id,
        module: "tto".to_string(),
        function: "start".to_string(),
        type_args: vec![],
        args: vec![],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let (parent, child) =
        if let SuiClientCommandResult::TransactionBlock(response) = start_call_result {
            let created = response.effects.unwrap().created().to_vec();
            let owners: BTreeSet<ObjectID> = created
                .iter()
                .flat_map(|refe| {
                    refe.owner
                        .get_address_owner_address()
                        .ok()
                        .map(|x| x.into())
                })
                .collect();
            let child = created
                .iter()
                .find(|refe| !owners.contains(&refe.reference.object_id))
                .unwrap();
            let parent = created
                .iter()
                .find(|refe| owners.contains(&refe.reference.object_id))
                .unwrap();
            (parent.reference.clone(), child.reference.clone())
        } else {
            unreachable!("Invalid response");
        };

    let receive_result = SuiClientCommands::Call {
        package: package_id.object_id,
        module: "tto".to_string(),
        function: "invalid_call_immut_ref".to_string(),
        type_args: vec![],
        args: vec![
            SuiJsonValue::from_str(&parent.object_id.to_string()).unwrap(),
            SuiJsonValue::from_str(&child.object_id.to_string()).unwrap(),
        ],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::TransactionBlock(response) = receive_result {
        assert!(response.effects.unwrap().into_status().is_ok());
    } else {
        unreachable!("Invalid response");
    };

    Ok(())
}

#[sim_test]
async fn test_receive_argument_by_mut_ref() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("tto");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            with_unpublished_dependencies: false,
            verify_deps: true,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await?;

    let owned_obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        let x = response.effects.unwrap();
        x.created().to_vec()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for OwnedObjectRef { reference, .. } in &owned_obj_ids {
        get_parsed_object_assert_existence(reference.object_id, context).await;
    }

    let package_id = owned_obj_ids
        .into_iter()
        .find(|OwnedObjectRef { owner, .. }| owner == &Owner::Immutable)
        .expect("Must find published package ID")
        .reference;

    // Start and then receive the object
    let start_call_result = SuiClientCommands::Call {
        package: package_id.object_id,
        module: "tto".to_string(),
        function: "start".to_string(),
        type_args: vec![],
        args: vec![],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let (parent, child) =
        if let SuiClientCommandResult::TransactionBlock(response) = start_call_result {
            let created = response.effects.unwrap().created().to_vec();
            let owners: BTreeSet<ObjectID> = created
                .iter()
                .flat_map(|refe| {
                    refe.owner
                        .get_address_owner_address()
                        .ok()
                        .map(|x| x.into())
                })
                .collect();
            let child = created
                .iter()
                .find(|refe| !owners.contains(&refe.reference.object_id))
                .unwrap();
            let parent = created
                .iter()
                .find(|refe| owners.contains(&refe.reference.object_id))
                .unwrap();
            (parent.reference.clone(), child.reference.clone())
        } else {
            unreachable!("Invalid response");
        };

    let receive_result = SuiClientCommands::Call {
        package: package_id.object_id,
        module: "tto".to_string(),
        function: "invalid_call_mut_ref".to_string(),
        type_args: vec![],
        args: vec![
            SuiJsonValue::from_str(&parent.object_id.to_string()).unwrap(),
            SuiJsonValue::from_str(&child.object_id.to_string()).unwrap(),
        ],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::TransactionBlock(response) = receive_result {
        assert!(response.effects.unwrap().into_status().is_ok());
    } else {
        unreachable!("Invalid response");
    };

    Ok(())
}

#[sim_test]
async fn test_package_publish_command_with_unpublished_dependency_succeeds()
-> Result<(), anyhow::Error> {
    let with_unpublished_dependencies = true; // Value under test, results in successful response.

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object()?.object_id;

    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("module_publish_with_unpublished_dependency");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            verify_deps: false,
            with_unpublished_dependencies,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let obj_ids = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        response
            .effects
            .as_ref()
            .unwrap()
            .created()
            .iter()
            .map(|refe| refe.reference.object_id)
            .collect::<Vec<_>>()
    } else {
        unreachable!("Invalid response");
    };

    // Check the objects
    for obj_id in obj_ids {
        get_parsed_object_assert_existence(obj_id, context).await;
    }

    Ok(())
}

#[sim_test]
async fn test_package_publish_command_with_unpublished_dependency_fails()
-> Result<(), anyhow::Error> {
    let with_unpublished_dependencies = false; // Value under test, results in error response.

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("module_publish_with_unpublished_dependency");
    let build_config = BuildConfig::new_for_testing().config;
    let result = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await;

    let expect = expect![[r#"
        Err(
            "The package has unpublished dependencies. If you want to publish with unpublished dependencies, please publish them one by one, or (not recommended) pass the `--with-unpublished-dependencies` flag.\n Unpublished dependencies: Unpublished\n        ",
        )
    "#]];
    expect.assert_debug_eq(&result);
    Ok(())
}

#[sim_test]
async fn test_package_publish_command_non_zero_unpublished_dep_fails() -> Result<(), anyhow::Error>
{
    let with_unpublished_dependencies = true; // Value under test, incompatible with dependencies that specify non-zero address.

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(address, None, None, None)
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("module_publish_with_unpublished_dependency_with_non_zero_address");
    let build_config = BuildConfig::new_for_testing().config;
    let result = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await;
    let err = result.unwrap_err().to_string();

    // errors due to tree shaking wanting to fetch the linkage table of this unpublished pkg
    assert!(err.contains("Failed to fetch package UnpublishedNonZeroAddress"));
    Ok(())
}

#[sim_test]
async fn test_package_publish_command_failure_invalid() -> Result<(), anyhow::Error> {
    let with_unpublished_dependencies = true; // Invalid packages should fail to publish, even if we allow unpublished dependencies.

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("module_publish_failure_invalid");
    let build_config = BuildConfig::new_for_testing().config;
    let result = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await;

    let expect = expect![[r#"
        "Error while loading dependency tests/data/module_dependency_invalid: error while loading legacy manifest \"tests/data/module_dependency_invalid/Move.toml\": Unable to parse AccountAddress (must be hex string of length 32)"
    "#]];
    expect.assert_debug_eq(&result.unwrap_err().to_string());
    Ok(())
}

#[sim_test]
async fn test_package_publish_test_flag() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(address, None, None, None)
        .await?
        .data;

    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("module_publish_with_nonexistent_dependency");
    let mut build_config: MoveBuildConfig = BuildConfig::new_for_testing().config;
    // this would have been the result of calling `sui client publish --test`
    build_config.test_mode = true;

    let result = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies: false,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await;

    let expect = expect![[r#"
        Err(
            ModulePublishFailure {
                error: "The `publish` subcommand should not be used with the `--test` flag\n\nCode in published packages must not depend on test code.\nIn order to fix this and publish the package without `--test`, remove any non-test dependencies on test-only code.\nYou can ensure all test-only dependencies have been removed by compiling the package normally with `sui move build`.",
            },
        )
    "#]];
    expect.assert_debug_eq(&result);
    Ok(())
}

#[sim_test]
async fn test_package_publish_empty() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("empty");
    let build_config = BuildConfig::new_for_testing().config;
    let result = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path,
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies: false,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await;

    // should return error
    let expect = expect![[r#"
        Err(
            ModulePublishFailure {
                error: "No modules found in the package",
            },
        )
    "#]];

    expect.assert_debug_eq(&result);
    Ok(())
}

#[sim_test]
async fn test_package_upgrade_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let chain_id = client.read_api().get_chain_identifier().await.unwrap();
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let (_tmp, package_path) =
        create_temp_dir_with_framework_packages("dummy_modules_upgrade", Some(chain_id))?;

    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish(PublishArgs {
        package_path: package_path.clone(),
        build_config: build_config.clone(),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
        payment: PaymentArgs {
            gas: vec![gas_obj_id],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    })
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let SuiClientCommandResult::TransactionBlock(response) = resp else {
        unreachable!("Invalid response");
    };

    let SuiTransactionBlockEffects::V1(effects) = response.effects.unwrap();

    assert!(effects.status.is_ok());
    assert_eq!(effects.gas_object().object_id(), gas_obj_id);

    // Now run the upgrade
    let resp = SuiClientCommands::Upgrade {
        package_path,
        upgrade_capability: None,
        build_config,
        skip_verify_compatibility: false,
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
        payment: PaymentArgs {
            gas: vec![gas_obj_id],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    resp.print(true);

    let SuiClientCommandResult::TransactionBlock(response) = resp else {
        unreachable!("Invalid upgrade response");
    };
    let SuiTransactionBlockEffects::V1(effects) = response.effects.unwrap();

    assert!(effects.status.is_ok());
    assert_eq!(effects.gas_object().object_id(), gas_obj_id);

    let obj_ids = effects
        .created()
        .iter()
        .map(|refe| refe.reference.object_id)
        .collect::<Vec<_>>();

    // Check the objects
    for obj_id in obj_ids {
        get_parsed_object_assert_existence(obj_id, context).await;
    }

    Ok(())
}

#[sim_test]
async fn test_package_management_on_upgrade_command() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let chain_id = client.read_api().get_chain_identifier().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    let (_tmp, package_path) =
        create_temp_dir_with_framework_packages("dummy_modules_upgrade", Some(chain_id))?;

    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::Publish(PublishArgs {
        package_path: package_path.clone(),
        build_config: build_config.clone(),
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
        payment: PaymentArgs {
            gas: vec![gas_obj_id],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    })
    .execute(context)
    .await?;

    let SuiClientCommandResult::TransactionBlock(publish_response) = resp else {
        unreachable!("Invalid response");
    };

    let SuiTransactionBlockEffects::V1(effects) = publish_response.clone().effects.unwrap();

    assert!(effects.status.is_ok());
    assert_eq!(effects.gas_object().object_id(), gas_obj_id);

    // Now run the upgrade
    let upgrade_response = SuiClientCommands::Upgrade {
        package_path: package_path.to_path_buf(),
        upgrade_capability: None,
        build_config: build_config.clone(),
        skip_verify_compatibility: false,
        skip_dependency_verification: false,
        verify_deps: true,
        with_unpublished_dependencies: false,
        payment: PaymentArgs {
            gas: vec![gas_obj_id],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    // Get Original Package ID and version
    let (expect_original_id, _, _) = publish_response
        .get_new_package_obj()
        .ok_or_else(|| anyhow::anyhow!("No package object response"))?;

    // Get Upgraded Package ID and version
    let (expect_upgrade_latest_id, expect_upgrade_version, _) =
        if let SuiClientCommandResult::TransactionBlock(response) = upgrade_response {
            assert_eq!(
                response.effects.as_ref().unwrap().gas_object().object_id(),
                gas_obj_id
            );
            response
                .get_new_package_obj()
                .ok_or_else(|| anyhow::anyhow!("No package object response"))?
        } else {
            unreachable!("Invalid response");
        };

    let published_file_str = std::fs::read_to_string(package_path.join("Published.toml")).unwrap();
    let published_file: ParsedPublishedFile<SuiFlavor> =
        toml_edit::de::from_str(&published_file_str).expect("to deserialize published file");
    let data = published_file
        .published
        .get("localnet")
        .expect("should have a localnet publication info");

    assert_eq!(
        expect_upgrade_latest_id,
        data.addresses.published_at.0.into()
    );
    assert_eq!(expect_original_id, data.addresses.original_id.0.into());
    assert_eq!(expect_upgrade_version, data.version.into());

    Ok(())
}

#[sim_test]
async fn test_native_transfer() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let recipient = SuiAddress::random_for_testing_only();
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;
    let obj_id = object_refs.get(1).unwrap().object().unwrap().object_id;

    let resp = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(recipient),
        object_id: obj_id,
        payment: PaymentArgs {
            gas: vec![gas_obj_id],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    // Get the mutated objects
    let (mut_obj1, mut_obj2) = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        assert!(
            response.status_ok().unwrap(),
            "Command failed: {:?}",
            response
        );
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            gas_obj_id
        );
        (
            response
                .effects
                .as_ref()
                .unwrap()
                .mutated()
                .first()
                .unwrap()
                .reference
                .object_id,
            response
                .effects
                .as_ref()
                .unwrap()
                .mutated()
                .get(1)
                .unwrap()
                .reference
                .object_id,
        )
    } else {
        panic!()
    };

    // Check the objects
    let resp = SuiClientCommands::Object {
        id: mut_obj1,
        bcs: false,
    }
    .execute(context)
    .await?;
    let mut_obj1 = if let SuiClientCommandResult::Object(resp) = resp {
        if let Some(obj) = resp.data {
            obj
        } else {
            panic!()
        }
    } else {
        panic!();
    };

    let resp2 = SuiClientCommands::Object {
        id: mut_obj2,
        bcs: false,
    }
    .execute(context)
    .await?;
    let mut_obj2 = if let SuiClientCommandResult::Object(resp2) = resp2 {
        if let Some(obj) = resp2.data {
            obj
        } else {
            panic!()
        }
    } else {
        panic!();
    };

    let (gas, obj) = if mut_obj1.owner.clone().unwrap().get_owner_address().unwrap() == address {
        (mut_obj1, mut_obj2)
    } else {
        (mut_obj2, mut_obj1)
    };

    assert_eq!(gas.owner.unwrap().get_owner_address().unwrap(), address);
    assert_eq!(obj.owner.unwrap().get_owner_address().unwrap(), recipient);

    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?;

    // Check log output contains all object ids.
    let obj_id = object_refs.data.get(1).unwrap().object().unwrap().object_id;

    let resp = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(recipient),
        object_id: obj_id,
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    // Get the mutated objects
    let (_mut_obj1, _mut_obj2) = if let SuiClientCommandResult::TransactionBlock(response) = resp {
        (
            response
                .effects
                .as_ref()
                .unwrap()
                .mutated()
                .first()
                .unwrap()
                .reference
                .object_id,
            response
                .effects
                .as_ref()
                .unwrap()
                .mutated()
                .get(1)
                .unwrap()
                .reference
                .object_id,
        )
    } else {
        panic!()
    };

    Ok(())
}

#[test]
// Test for issue https://github.com/MystenLabs/sui/issues/1078
fn test_bug_1078() {
    let read = SuiClientCommandResult::Object(SuiObjectResponse::new_with_error(
        SuiObjectResponseError::NotExists {
            object_id: ObjectID::random(),
        },
    ));
    let mut writer = String::new();
    // fmt ObjectRead should not fail.
    write!(writer, "{}", read).unwrap();
    write!(writer, "{:?}", read).unwrap();
}

#[sim_test]
async fn test_switch_command() -> Result<(), anyhow::Error> {
    let mut cluster = TestClusterBuilder::new().build().await;
    let addr2 = cluster.get_address_1();
    let context = cluster.wallet_mut();

    // Get the active address
    let addr1 = context.active_address()?;

    // Run a command with address omitted
    let os = SuiClientCommands::Objects { address: None }
        .execute(context)
        .await?;

    let mut cmd_objs = if let SuiClientCommandResult::Objects(v) = os {
        v
    } else {
        panic!("Command failed")
    };

    // Check that we indeed fetched for addr1
    let client = context.get_client().await?;
    let mut actual_objs = client
        .read_api()
        .get_owned_objects(
            addr1,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content(),
            )),
            None,
            None,
        )
        .await
        .unwrap()
        .data;
    cmd_objs.sort();
    actual_objs.sort();
    assert_eq!(cmd_objs, actual_objs);

    // Switch the address
    let resp = SuiClientCommands::Switch {
        address: Some(KeyIdentity::Address(addr2)),
        env: None,
    }
    .execute(context)
    .await?;
    assert_eq!(addr2, context.active_address()?);
    assert_ne!(addr1, context.active_address()?);
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(addr2.to_string()),
                env: None
            })
        )
    );

    // Wipe all the address info
    context.config.active_address = None;

    // Create a new address
    let os = SuiClientCommands::NewAddress {
        key_scheme: SignatureScheme::ED25519,
        alias: None,
        derivation_path: None,
        word_length: None,
    }
    .execute(context)
    .await?;
    let new_addr = if let SuiClientCommandResult::NewAddress(x) = os {
        x.address
    } else {
        panic!("Command failed")
    };

    // Check that we can switch to this address
    // Switch the address
    let resp = SuiClientCommands::Switch {
        address: Some(KeyIdentity::Address(new_addr)),
        env: None,
    }
    .execute(context)
    .await?;
    assert_eq!(new_addr, context.active_address()?);
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(new_addr.to_string()),
                env: None
            })
        )
    );
    Ok(())
}

#[sim_test]
async fn test_new_address_command_by_flag() -> Result<(), anyhow::Error> {
    let mut cluster = TestClusterBuilder::new().build().await;
    let context = cluster.wallet_mut();

    // keypairs loaded from config are Ed25519
    assert_eq!(
        context
            .config
            .keystore
            .entries()
            .iter()
            .filter(|k| k.flag() == Ed25519SuiSignature::SCHEME.flag())
            .count(),
        5
    );

    SuiClientCommands::NewAddress {
        key_scheme: SignatureScheme::Secp256k1,
        alias: None,
        derivation_path: None,
        word_length: None,
    }
    .execute(context)
    .await?;

    // new keypair generated is Secp256k1
    assert_eq!(
        context
            .config
            .keystore
            .entries()
            .iter()
            .filter(|k| k.flag() == Secp256k1SuiSignature::SCHEME.flag())
            .count(),
        1
    );

    Ok(())
}

#[sim_test]
async fn test_remove_address_command() -> Result<(), anyhow::Error> {
    let mut cluster = TestClusterBuilder::new().build().await;
    let context = cluster.wallet_mut();

    let addr = context.config.keystore.addresses().get(1).cloned().unwrap();

    SuiClientCommands::RemoveAddress {
        alias_or_address: addr.to_string(),
    }
    .execute(context)
    .await?;

    assert_eq!(
        context
            .config
            .keystore
            .addresses()
            .iter()
            .filter(|k| *k == &addr)
            .count(),
        0
    );

    Ok(())
}

#[sim_test]
async fn test_active_address_command() -> Result<(), anyhow::Error> {
    let mut cluster = TestClusterBuilder::new().build().await;
    let context = cluster.wallet_mut();

    // Get the active address
    let addr1 = context.active_address()?;

    // Run a command with address omitted
    let os = SuiClientCommands::ActiveAddress {}.execute(context).await?;

    let a = if let SuiClientCommandResult::ActiveAddress(Some(v)) = os {
        v
    } else {
        panic!("Command failed")
    };
    assert_eq!(a, addr1);

    let addr2 = context.config.keystore.addresses().get(1).cloned().unwrap();
    let resp = SuiClientCommands::Switch {
        address: Some(KeyIdentity::Address(addr2)),
        env: None,
    }
    .execute(context)
    .await?;
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(addr2.to_string()),
                env: None
            })
        )
    );

    // switch back to addr1 by using its alias
    let alias1 = context.config.keystore.get_alias(&addr1).unwrap();
    let resp = SuiClientCommands::Switch {
        address: Some(KeyIdentity::Alias(alias1)),
        env: None,
    }
    .execute(context)
    .await?;
    assert_eq!(
        format!("{resp}"),
        format!(
            "{}",
            SuiClientCommandResult::Switch(SwitchResponse {
                address: Some(addr1.to_string()),
                env: None
            })
        )
    );

    Ok(())
}

fn get_gas_value(o: &SuiObjectData) -> u64 {
    GasCoin::try_from(o).unwrap().value()
}

async fn get_object(id: ObjectID, context: &WalletContext) -> Option<SuiObjectData> {
    let client = context.get_client().await.unwrap();
    let response = client
        .read_api()
        .get_object_with_options(id, SuiObjectDataOptions::full_content())
        .await
        .unwrap();
    response.data
}

async fn get_parsed_object_assert_existence(
    object_id: ObjectID,
    context: &WalletContext,
) -> SuiObjectData {
    get_object(object_id, context)
        .await
        .expect("Object {object_id} does not exist.")
}

#[sim_test]
async fn test_merge_coin() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Check log output contains all object ids.
    let gas = object_refs.first().unwrap().object().unwrap().object_id;
    let primary_coin = object_refs.get(1).unwrap().object().unwrap().object_id;
    let coin_to_merge = object_refs.get(2).unwrap().object().unwrap().object_id;

    let total_value = get_gas_value(&get_object(primary_coin, context).await.unwrap())
        + get_gas_value(&get_object(coin_to_merge, context).await.unwrap());

    // Test with gas specified
    let resp = SuiClientCommands::MergeCoin {
        primary_coin,
        coin_to_merge,
        payment: PaymentArgs { gas: vec![gas] },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;
    let g = if let SuiClientCommandResult::TransactionBlock(r) = resp {
        assert!(r.status_ok().unwrap(), "Command failed: {:?}", r);
        assert_eq!(r.effects.as_ref().unwrap().gas_object().object_id(), gas);
        let object_id = r
            .effects
            .as_ref()
            .unwrap()
            .mutated_excluding_gas()
            .into_iter()
            .next()
            .unwrap()
            .reference
            .object_id;
        get_parsed_object_assert_existence(object_id, context).await
    } else {
        panic!("Command failed")
    };

    // Check total value is expected
    assert_eq!(get_gas_value(&g), total_value);

    // Check that old coin is deleted
    assert_eq!(get_object(coin_to_merge, context).await, None);

    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?;

    let primary_coin = object_refs.data.get(1).unwrap().object()?.object_id;
    let coin_to_merge = object_refs.data.get(2).unwrap().object()?.object_id;

    let total_value = get_gas_value(&get_object(primary_coin, context).await.unwrap())
        + get_gas_value(&get_object(coin_to_merge, context).await.unwrap());

    // Test with no gas specified
    let resp = SuiClientCommands::MergeCoin {
        primary_coin,
        coin_to_merge,
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let g = if let SuiClientCommandResult::TransactionBlock(r) = resp {
        let object_id = r
            .effects
            .as_ref()
            .unwrap()
            .mutated_excluding_gas()
            .into_iter()
            .next()
            .unwrap()
            .reference
            .object_id;
        get_parsed_object_assert_existence(object_id, context).await
    } else {
        panic!("Command failed")
    };

    // Check total value is expected
    assert_eq!(get_gas_value(&g), total_value);

    // Check that old coin is deleted
    assert_eq!(get_object(coin_to_merge, context).await, None);

    Ok(())
}

#[sim_test]
async fn test_split_coin() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?;

    // Check log output contains all object ids.
    let gas = object_refs.data.first().unwrap().object()?.object_id;
    let mut coin = object_refs.data.get(1).unwrap().object()?.object_id;

    let orig_value = get_gas_value(&get_object(coin, context).await.unwrap());

    // Test with gas specified
    let resp = SuiClientCommands::SplitCoin {
        coin_id: coin,
        amounts: Some(vec![1000, 10]),
        count: None,
        payment: PaymentArgs { gas: vec![gas] },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let (updated_coin, new_coins) = if let SuiClientCommandResult::TransactionBlock(r) = resp {
        assert!(r.status_ok().unwrap(), "Command failed: {:?}", r);
        assert_eq!(r.effects.as_ref().unwrap().gas_object().object_id(), gas);
        let updated_object_id = r
            .effects
            .as_ref()
            .unwrap()
            .mutated_excluding_gas()
            .into_iter()
            .next()
            .unwrap()
            .reference
            .object_id;
        let updated_obj = get_parsed_object_assert_existence(updated_object_id, context).await;
        let new_object_refs = r.effects.unwrap().created().to_vec();
        let mut new_objects = Vec::with_capacity(new_object_refs.len());
        for obj_ref in new_object_refs {
            new_objects.push(
                get_parsed_object_assert_existence(obj_ref.reference.object_id, context).await,
            );
        }
        (updated_obj, new_objects)
    } else {
        panic!("Command failed")
    };

    // Check values expected
    assert_eq!(get_gas_value(&updated_coin) + 1000 + 10, orig_value);
    assert!((get_gas_value(&new_coins[0]) == 1000) || (get_gas_value(&new_coins[0]) == 10));
    assert!((get_gas_value(&new_coins[1]) == 1000) || (get_gas_value(&new_coins[1]) == 10));
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Get another coin
    for c in object_refs {
        let coin_data = c.into_object().unwrap();
        if get_gas_value(&get_object(coin_data.object_id, context).await.unwrap()) > 2000 {
            coin = coin_data.object_id;
        }
    }
    let orig_value = get_gas_value(&get_object(coin, context).await.unwrap());

    // Test split coin into equal parts
    let resp = SuiClientCommands::SplitCoin {
        coin_id: coin,
        amounts: None,
        count: Some(3),
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let (updated_coin, new_coins) = if let SuiClientCommandResult::TransactionBlock(r) = resp {
        assert!(r.status_ok().unwrap(), "Command failed: {:?}", r);
        let updated_object_id = r
            .effects
            .as_ref()
            .unwrap()
            .mutated_excluding_gas()
            .into_iter()
            .next()
            .unwrap()
            .reference
            .object_id;
        let updated_obj = get_parsed_object_assert_existence(updated_object_id, context).await;
        let new_object_refs = r.effects.unwrap().created().to_vec();
        let mut new_objects = Vec::with_capacity(new_object_refs.len());
        for obj_ref in new_object_refs {
            new_objects.push(
                get_parsed_object_assert_existence(obj_ref.reference.object_id, context).await,
            );
        }
        (updated_obj, new_objects)
    } else {
        panic!("Command failed")
    };

    // Check values expected
    assert_eq!(
        get_gas_value(&updated_coin),
        orig_value / 3 + orig_value % 3
    );
    assert_eq!(get_gas_value(&new_coins[0]), orig_value / 3);
    assert_eq!(get_gas_value(&new_coins[1]), orig_value / 3);

    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Get another coin
    for c in object_refs {
        let coin_data = c.into_object().unwrap();
        if get_gas_value(&get_object(coin_data.object_id, context).await.unwrap()) > 2000 {
            coin = coin_data.object_id;
        }
    }
    let orig_value = get_gas_value(&get_object(coin, context).await.unwrap());

    // Test with no gas specified
    let resp = SuiClientCommands::SplitCoin {
        coin_id: coin,
        amounts: Some(vec![1000, 10]),
        count: None,
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let (updated_coin, new_coins) = if let SuiClientCommandResult::TransactionBlock(r) = resp {
        assert!(r.status_ok().unwrap(), "Command failed: {:?}", r);
        let updated_object_id = r
            .effects
            .as_ref()
            .unwrap()
            .mutated_excluding_gas()
            .into_iter()
            .next()
            .unwrap()
            .reference
            .object_id;
        let updated_obj = get_parsed_object_assert_existence(updated_object_id, context).await;
        let new_object_refs = r.effects.unwrap().created().to_vec();
        let mut new_objects = Vec::with_capacity(new_object_refs.len());
        for obj_ref in new_object_refs {
            new_objects.push(
                get_parsed_object_assert_existence(obj_ref.reference.object_id, context).await,
            );
        }
        (updated_obj, new_objects)
    } else {
        panic!("Command failed")
    };

    // Check values expected
    assert_eq!(get_gas_value(&updated_coin) + 1000 + 10, orig_value);
    assert!((get_gas_value(&new_coins[0]) == 1000) || (get_gas_value(&new_coins[0]) == 10));
    assert!((get_gas_value(&new_coins[1]) == 1000) || (get_gas_value(&new_coins[1]) == 10));
    Ok(())
}

#[sim_test]
async fn test_signature_flag() -> Result<(), anyhow::Error> {
    let res = SignatureScheme::from_flag("0");
    assert!(res.is_ok());
    assert_eq!(res.unwrap().flag(), SignatureScheme::ED25519.flag());

    let res = SignatureScheme::from_flag("1");
    assert!(res.is_ok());
    assert_eq!(res.unwrap().flag(), SignatureScheme::Secp256k1.flag());

    let res = SignatureScheme::from_flag("2");
    assert!(res.is_ok());
    assert_eq!(res.unwrap().flag(), SignatureScheme::Secp256r1.flag());

    let res = SignatureScheme::from_flag("something");
    assert!(res.is_err());
    Ok(())
}

#[sim_test]
async fn test_execute_signed_tx() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    let mut txns = batch_make_transfer_transactions(context, 1).await;
    let txn = txns.swap_remove(0);

    let (tx_data, signatures) = txn.to_tx_bytes_and_signatures();
    SuiClientCommands::ExecuteSignedTx {
        tx_bytes: tx_data.encoded(),
        signatures: signatures.into_iter().map(|s| s.encoded()).collect(),
    }
    .execute(context)
    .await?;
    Ok(())
}

#[sim_test]
async fn test_serialize_tx() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let address1 = test_cluster.get_address_1();
    let context = &mut test_cluster.wallet;
    let alias1 = context.config.keystore.get_alias(&address1).unwrap();
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;
    let coin = object_refs.get(1).unwrap().object().unwrap().object_id;

    SuiClientCommands::TransferSui {
        to: KeyIdentity::Address(address1),
        sui_coin_object_id: coin,
        amount: Some(1),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs {
            serialize_unsigned_transaction: true,
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    SuiClientCommands::TransferSui {
        to: KeyIdentity::Address(address1),
        sui_coin_object_id: coin,
        amount: Some(1),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs {
            serialize_signed_transaction: true,
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    // use alias for transfer
    SuiClientCommands::TransferSui {
        to: KeyIdentity::Alias(alias1),
        sui_coin_object_id: coin,
        amount: Some(1),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs {
            serialize_signed_transaction: true,
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    let ptb_args = vec![
        "--split-coins".to_string(),
        "gas".to_string(),
        "[1000]".to_string(),
        "--assign".to_string(),
        "new_coin".to_string(),
        "--transfer-objects".to_string(),
        "[new_coin]".to_string(),
        format!("@{}", address1),
        "--gas-budget".to_string(),
        "50000000".to_string(),
    ];
    let mut args = ptb_args.clone();
    args.push("--serialize-signed-transaction".to_string());
    let ptb = PTB { args };
    SuiClientCommands::PTB(ptb).execute(context).await.unwrap();
    let mut args = ptb_args.clone();
    args.push("--serialize-unsigned-transaction".to_string());
    let ptb = PTB { args };
    SuiClientCommands::PTB(ptb).execute(context).await.unwrap();

    Ok(())
}

#[tokio::test]
async fn test_stake_with_none_amount() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let coins = client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await?
        .data;

    let config_path = test_cluster.swarm.dir().join(SUI_CLIENT_CONFIG);
    let validator_addr = client
        .governance_api()
        .get_latest_sui_system_state()
        .await?
        .active_validators[0]
        .sui_address;

    test_with_sui_binary(&[
        "client",
        "--client.config",
        config_path.to_str().unwrap(),
        "call",
        "--package",
        "0x3",
        "--module",
        "sui_system",
        "--function",
        "request_add_stake_mul_coin",
        "--args",
        "0x5",
        &format!("[{}]", coins.first().unwrap().coin_object_id),
        "[]",
        &validator_addr.to_string(),
        "--gas-budget",
        "1000000000",
    ])
    .await?;

    let stake = client.governance_api().get_stakes(address).await?;

    assert_eq!(1, stake.len());
    assert_eq!(
        coins.first().unwrap().balance,
        stake.first().unwrap().stakes.first().unwrap().principal
    );
    Ok(())
}

#[tokio::test]
async fn test_stake_with_u64_amount() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let coins = client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await?
        .data;

    let config_path = test_cluster.swarm.dir().join(SUI_CLIENT_CONFIG);
    let validator_addr = client
        .governance_api()
        .get_latest_sui_system_state()
        .await?
        .active_validators[0]
        .sui_address;

    test_with_sui_binary(&[
        "client",
        "--client.config",
        config_path.to_str().unwrap(),
        "call",
        "--package",
        "0x3",
        "--module",
        "sui_system",
        "--function",
        "request_add_stake_mul_coin",
        "--args",
        "0x5",
        &format!("[{}]", coins.first().unwrap().coin_object_id),
        "[1000000000]",
        &validator_addr.to_string(),
        "--gas-budget",
        "1000000000",
    ])
    .await?;

    let stake = client.governance_api().get_stakes(address).await?;

    assert_eq!(1, stake.len());
    assert_eq!(
        1000000000,
        stake.first().unwrap().stakes.first().unwrap().principal
    );
    Ok(())
}

async fn test_with_sui_binary(args: &[&str]) -> Result<(), anyhow::Error> {
    let mut cmd = assert_cmd::Command::cargo_bin("sui").unwrap();
    let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    // test cluster will not response if this call is in the same thread
    let out = thread::spawn(move || cmd.args(args).assert());
    while !out.is_finished() {
        sleep(Duration::from_millis(100)).await;
    }
    out.join().unwrap().success();
    Ok(())
}

#[sim_test]
async fn test_get_owned_objects_owned_by_address_and_check_pagination() -> Result<(), anyhow::Error>
{
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;

    let client = context.get_client().await?;
    let object_responses = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new(
                Some(SuiObjectDataFilter::StructType(GasCoin::type_())),
                Some(
                    SuiObjectDataOptions::new()
                        .with_type()
                        .with_owner()
                        .with_previous_transaction(),
                ),
            )),
            None,
            None,
        )
        .await?;

    // assert that all the objects_returned are owned by the address
    for resp in &object_responses.data {
        let obj_owner = resp.object().unwrap().owner.clone().unwrap();
        assert_eq!(
            obj_owner.get_owner_address().unwrap().to_string(),
            address.to_string()
        )
    }
    // assert that has next page is false
    assert!(!object_responses.has_next_page);

    // Pagination check
    let mut has_next = true;
    let mut cursor = None;
    let mut response_data: Vec<SuiObjectResponse> = Vec::new();
    while has_next {
        let object_responses = client
            .read_api()
            .get_owned_objects(
                address,
                Some(SuiObjectResponseQuery::new(
                    Some(SuiObjectDataFilter::StructType(GasCoin::type_())),
                    Some(
                        SuiObjectDataOptions::new()
                            .with_type()
                            .with_owner()
                            .with_previous_transaction(),
                    ),
                )),
                cursor,
                Some(1),
            )
            .await?;

        response_data.push(object_responses.data.first().unwrap().clone());

        if object_responses.has_next_page {
            cursor = object_responses.next_cursor;
        } else {
            has_next = false;
        }
    }

    assert_eq!(&response_data, &object_responses.data);

    Ok(())
}

#[tokio::test]
async fn key_identity_test() {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let alias = context.config.keystore.get_alias(&address).unwrap();

    // by alias
    assert_eq!(
        address,
        context
            .get_identity_address(Some(KeyIdentity::Alias(alias)))
            .unwrap()
    );
    // by address
    assert_eq!(
        address,
        context
            .get_identity_address(Some(KeyIdentity::Address(address)))
            .unwrap()
    );
    // alias does not exist
    assert!(
        context
            .get_identity_address(Some(KeyIdentity::Alias("alias".to_string())))
            .is_err()
    );

    // get active address instead when no alias/address is given
    assert_eq!(
        context.active_address().unwrap(),
        context.get_identity_address(None).unwrap()
    );
}

fn assert_dry_run(dry_run: SuiClientCommandResult, object_id: ObjectID, command: &str) {
    if let SuiClientCommandResult::DryRun(response) = dry_run {
        assert_eq!(
            *response.effects.status(),
            SuiExecutionStatus::Success,
            "{command} dry run test effects is not success"
        );
        assert_eq!(
            response.effects.gas_object().object_id(),
            object_id,
            "{command} dry run test failed, gas object used is not the expected one"
        );
    } else {
        panic!("{} dry run failed", command);
    }
}

#[sim_test]
async fn test_dry_run() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content(),
            )),
            None,
            None,
        )
        .await?;

    let object_id = object_refs
        .data
        .first()
        .unwrap()
        .object()
        .unwrap()
        .object_id;
    let object_to_send = object_refs.data.get(1).unwrap().object().unwrap().object_id;

    // === TRANSFER === //
    let transfer_dry_run = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(SuiAddress::random_for_testing_only()),
        object_id: object_to_send,
        payment: PaymentArgs {
            gas: vec![object_id],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs {
            dry_run: true,
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    assert_dry_run(transfer_dry_run, object_id, "Transfer");

    // === TRANSFER SUI === //
    let transfer_sui_dry_run = SuiClientCommands::TransferSui {
        to: KeyIdentity::Address(SuiAddress::random_for_testing_only()),
        sui_coin_object_id: object_to_send,
        amount: Some(1),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs {
            dry_run: true,
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    assert_dry_run(transfer_sui_dry_run, object_to_send, "TransferSui");

    // === PAY === //
    let pay_dry_run = SuiClientCommands::Pay {
        input_coins: vec![object_id],
        recipients: vec![KeyIdentity::Address(SuiAddress::random_for_testing_only())],
        amounts: vec![1],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs {
            dry_run: true,
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::DryRun(response) = pay_dry_run {
        assert_eq!(*response.effects.status(), SuiExecutionStatus::Success);
        assert_ne!(response.effects.gas_object().object_id(), object_id);
    } else {
        panic!("Pay dry run failed");
    }

    // specify which gas object to use
    let gas_coin_id = object_refs.data.last().unwrap().object().unwrap().object_id;
    let pay_dry_run = SuiClientCommands::Pay {
        input_coins: vec![object_id],
        recipients: vec![KeyIdentity::Address(SuiAddress::random_for_testing_only())],
        amounts: vec![1],
        payment: PaymentArgs {
            gas: vec![gas_coin_id],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs {
            dry_run: true,
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    assert_dry_run(pay_dry_run, gas_coin_id, "Pay");

    // === PAY SUI === //
    let pay_sui_dry_run = SuiClientCommands::PaySui {
        input_coins: vec![object_id],
        recipients: vec![KeyIdentity::Address(SuiAddress::random_for_testing_only())],
        amounts: vec![1],
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs {
            dry_run: true,
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    assert_dry_run(pay_sui_dry_run, object_id, "PaySui");

    // === PAY ALL SUI === //
    let pay_all_sui_dry_run = SuiClientCommands::PayAllSui {
        input_coins: vec![object_id],
        recipient: KeyIdentity::Address(SuiAddress::random_for_testing_only()),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs {
            dry_run: true,
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    assert_dry_run(pay_all_sui_dry_run, object_id, "PayAllSui");

    Ok(())
}

async fn test_cluster_helper() -> (
    TestCluster,
    SuiClient,
    u64,
    [ObjectID; 3],
    [KeyIdentity; 2],
    [SuiAddress; 2],
) {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address1 = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await.unwrap();
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address1,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content(),
            )),
            None,
            None,
        )
        .await
        .unwrap();

    let object_id1 = object_refs
        .data
        .first()
        .unwrap()
        .object()
        .unwrap()
        .object_id;
    let object_id2 = object_refs.data.get(1).unwrap().object().unwrap().object_id;
    let object_id3 = object_refs.data.get(2).unwrap().object().unwrap().object_id;
    let address2 = SuiAddress::random_for_testing_only();
    let address3 = SuiAddress::random_for_testing_only();
    let recipient1 = KeyIdentity::Address(address2);
    let recipient2 = KeyIdentity::Address(address3);

    (
        test_cluster,
        client,
        rgp,
        [object_id1, object_id2, object_id3],
        [recipient1, recipient2],
        [address2, address3],
    )
}

#[sim_test]
async fn test_pay() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let (object_id1, object_id2, object_id3) = (objects[0], objects[1], objects[2]);
    let (recipient1, recipient2) = (&recipients[0], &recipients[1]);
    let (address2, address3) = (addresses[0], addresses[1]);
    let context = &mut test_cluster.wallet;
    let pay = SuiClientCommands::Pay {
        input_coins: vec![object_id1, object_id2],
        recipients: vec![recipient1.clone(), recipient2.clone()],
        amounts: vec![5000, 10000],
        payment: PaymentArgs {
            gas: vec![object_id1],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await;

    // we passed the gas object to be one of the input coins, which should fail
    assert!(pay.is_err());

    let amounts = [5000, 10000];
    // we expect this to be the gas coin used
    let pay = SuiClientCommands::Pay {
        input_coins: vec![object_id1, object_id2],
        recipients: vec![recipient1.clone(), recipient2.clone()],
        amounts: amounts.into(),
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    // Pay command takes the input coins and transfers the given amounts from each input coin (in order)
    // to the recipients
    // this test checks if the recipients have received the objects, and if the gas object used is
    // the right one (not one of the input coins, and in this setup it's the 3rd coin of sender)
    // we also check if the balances are right!
    if let SuiClientCommandResult::TransactionBlock(response) = pay {
        // check tx status
        assert!(response.status_ok().unwrap());
        // check gas coin used
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            object_id3
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            client
                .coin_read_api()
                .get_balance(address2, None)
                .await?
                .total_balance,
            amounts[0] as u128
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address3,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(response.status_ok().unwrap());
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            client
                .coin_read_api()
                .get_balance(address3, None)
                .await?
                .total_balance,
            amounts[1] as u128
        );
    } else {
        panic!("Pay test failed");
    }

    Ok(())
}

#[sim_test]
async fn test_pay_sui() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let (object_id1, object_id2) = (objects[0], objects[1]);
    let (recipient1, recipient2) = (&recipients[0], &recipients[1]);
    let (address2, address3) = (addresses[0], addresses[1]);
    let context = &mut test_cluster.wallet;
    let amounts = [1000, 5000];
    let pay_sui = SuiClientCommands::PaySui {
        input_coins: vec![object_id1, object_id2],
        recipients: vec![recipient1.clone(), recipient2.clone()],
        amounts: amounts.into(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    // pay sui takes the input coins and transfers from each of them (in order) the amounts to the
    // respective recipients.
    // check if each recipient has one object, if the tx status is success,
    // and if the gas object used was the first object in the input coins
    // we also check if the balances of each recipient are right!
    if let SuiClientCommandResult::TransactionBlock(response) = pay_sui {
        assert!(response.status_ok().unwrap());
        // check gas coin used
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            object_id1
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            client
                .coin_read_api()
                .get_balance(address2, None)
                .await?
                .total_balance,
            amounts[0] as u128
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address3,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(response.status_ok().unwrap());
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            client
                .coin_read_api()
                .get_balance(address3, None)
                .await?
                .total_balance,
            amounts[1] as u128
        );
    } else {
        panic!("PaySui test failed");
    }
    Ok(())
}

#[sim_test]
async fn test_pay_all_sui() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let (object_id1, object_id2) = (objects[0], objects[1]);
    let recipient1 = &recipients[0];
    let address2 = addresses[0];
    let context = &mut test_cluster.wallet;
    let pay_all_sui = SuiClientCommands::PayAllSui {
        input_coins: vec![object_id1, object_id2],
        recipient: recipient1.clone(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    // pay all sui will take the input coins and smash them into one coin and transfer that coin to
    // the recipient, so we check that the recipient has one object, if the tx status is success,
    // and if the gas object used was the first object in the input coins
    if let SuiClientCommandResult::TransactionBlock(response) = pay_all_sui {
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(response.status_ok().unwrap());
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            response.effects.unwrap().gas_object().object_id(),
            object_id1
        );
    } else {
        panic!("PayAllSui test failed");
    }

    Ok(())
}

#[sim_test]
async fn test_transfer() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let (object_id1, object_id2) = (objects[0], objects[1]);
    let recipient1 = &recipients[0];
    let address2 = addresses[0];
    let context = &mut test_cluster.wallet;
    let transfer = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(address2),
        object_id: object_id1,
        payment: PaymentArgs {
            gas: vec![object_id1],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await;

    // passed the gas object to be the object to transfer, which should fail
    assert!(transfer.is_err());

    let transfer = SuiClientCommands::Transfer {
        to: recipient1.clone(),
        object_id: object_id1,
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;
    // transfer command will transfer the object_id1 to address2, and use object_id2 as gas
    // we check if object1 is owned by address 2 and if the gas object used is object_id2
    if let SuiClientCommandResult::TransactionBlock(response) = transfer {
        assert!(response.status_ok().unwrap());
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            object_id2
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        assert_eq!(
            objs_refs.data.first().unwrap().object().unwrap().object_id,
            object_id1
        );
    } else {
        panic!("Transfer test failed");
    }
    Ok(())
}

#[sim_test]
async fn test_transfer_sui() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let object_id1 = objects[0];
    let recipient1 = &recipients[0];
    let address2 = addresses[0];
    let context = &mut test_cluster.wallet;
    let amount = 1000;
    let transfer_sui = SuiClientCommands::TransferSui {
        to: KeyIdentity::Address(address2),
        sui_coin_object_id: object_id1,
        amount: Some(amount),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    // transfer sui will transfer the amount from object_id1 to address2, and use the same object
    // as gas, and we check if the recipient address received the object, and the expected balance
    // is correct
    if let SuiClientCommandResult::TransactionBlock(response) = transfer_sui {
        assert!(response.status_ok().unwrap());
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            object_id1
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(!objs_refs.has_next_page);
        assert_eq!(objs_refs.data.len(), 1);
        let balance = client
            .coin_read_api()
            .get_balance(address2, None)
            .await?
            .total_balance;
        assert_eq!(balance, amount as u128);
    } else {
        panic!("TransferSui test failed");
    }
    // transfer the whole object by not passing an amount
    let transfer_sui = SuiClientCommands::TransferSui {
        to: recipient1.clone(),
        sui_coin_object_id: object_id1,
        amount: None,
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;
    if let SuiClientCommandResult::TransactionBlock(response) = transfer_sui {
        assert!(response.status_ok().unwrap());
        assert_eq!(
            response.effects.as_ref().unwrap().gas_object().object_id(),
            object_id1
        );
        let objs_refs = client
            .read_api()
            .get_owned_objects(
                address2,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
            )
            .await?;
        assert!(!objs_refs.has_next_page);
        assert_eq!(
            objs_refs.data.len(),
            2,
            "Expected to have two coins when calling transfer sui the 2nd time"
        );
        assert!(
            objs_refs
                .data
                .iter()
                .any(|x| x.object().unwrap().object_id == object_id1)
        );
    } else {
        panic!("TransferSui test failed");
    }
    Ok(())
}

#[sim_test]
async fn test_transfer_gas_smash() -> Result<(), anyhow::Error> {
    // Like `test_transfer` but using multiple gas objects.
    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let (object_id0, object_id1, object_id2) = (objects[0], objects[1], objects[2]);
    let recipient1 = &recipients[0];
    let address2 = addresses[0];
    let context = &mut test_cluster.wallet;
    let transfer = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(address2),
        object_id: object_id1,
        payment: PaymentArgs {
            gas: vec![object_id0, object_id1],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await;

    // Overlap between the object being transferred and the gas objects should fail.
    assert!(transfer.is_err());

    let transfer = SuiClientCommands::Transfer {
        to: recipient1.clone(),
        object_id: object_id2,
        payment: PaymentArgs {
            gas: vec![object_id0, object_id1],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    // transfer command will transfer the object_id2 to address2, and use object_id0, and
    // object_id1 as gas we check if object1 is owned by address 2 and the gas object used.
    let SuiClientCommandResult::TransactionBlock(response) = transfer else {
        panic!("Transfer test failed");
    };

    assert!(response.status_ok().unwrap());
    assert_eq!(
        response.effects.as_ref().unwrap().gas_object().object_id(),
        object_id0
    );
    let objs_refs = client
        .read_api()
        .get_owned_objects(
            address2,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content(),
            )),
            None,
            None,
        )
        .await?;
    assert!(!objs_refs.has_next_page);
    assert_eq!(objs_refs.data.len(), 1);
    assert_eq!(
        objs_refs.data.first().unwrap().object().unwrap().object_id,
        object_id2
    );

    Ok(())
}

#[sim_test]
async fn test_transfer_sponsored() -> Result<(), anyhow::Error> {
    // Like `test_transfer` but the gas is sponsored by the recipient.
    let (mut cluster, _, rgp, o, _, _) = test_cluster_helper().await;
    let a0 = cluster.get_address_0();
    let a1 = cluster.get_address_1();
    let context = &mut cluster.wallet;

    // A0 sends O1 to A1
    let transfer = SuiClientCommands::TransferSui {
        to: KeyIdentity::Address(a1),
        sui_coin_object_id: o[1],
        amount: None,
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::TransactionBlock(response) = transfer else {
        panic!("Failed to set-up test")
    };

    assert_eq!(response.status_ok(), Some(true));

    // A1 sends 01 back to A0, but sponsored by A0.
    let transfer_back = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(a0),
        object_id: o[1],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            gas_sponsor: Some(a0),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::TransactionBlock(response) = transfer_back else {
        panic!("Failed to run sponsored transfer")
    };

    let Some(tx) = &response.transaction else {
        panic!("TransactionBlock response should contain a transaction");
    };

    assert_eq!(response.status_ok(), Some(true));
    assert_eq!(tx.data.gas_data().owner, a0);
    assert_eq!(tx.data.sender(), &a1);

    Ok(())
}

#[sim_test]
async fn test_transfer_serialized_data() -> Result<(), anyhow::Error> {
    // Like `test_transfer` but the transaction is pre-generated and serialized into a
    // Base64 string containing a Base64-encoded TransactionData.
    let (mut cluster, client, rgp, o, _, a) = test_cluster_helper().await;
    let context = &mut cluster.wallet;

    // Build the transaction without running it.
    let transfer = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(a[1]),
        object_id: o[0],
        payment: PaymentArgs { gas: vec![o[1]] },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs {
            serialize_unsigned_transaction: true,
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::SerializedUnsignedTransaction(tx_data) = transfer else {
        panic!("Expected SerializedUnsignedTransaction result");
    };

    let tx_bytes = Base64::encode(bcs::to_bytes(&tx_data)?);
    let transfer_serialized = SuiClientCommands::SerializedTx {
        tx_bytes,
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::TransactionBlock(response) = transfer_serialized else {
        panic!("Expected TransactionBlock result");
    };

    let Some(effects) = &response.effects else {
        panic!("TransactionBlock response should contain effects");
    };

    assert!(effects.status().is_ok());
    assert_eq!(effects.gas_object().object_id(), o[1]);

    let a1_objs = client
        .read_api()
        .get_owned_objects(a[1], None, None, None)
        .await?;

    assert!(!a1_objs.has_next_page);

    let page = a1_objs.data;
    assert_eq!(page.len(), 1);
    assert_eq!(page.first().unwrap().object().unwrap().object_id, o[0]);

    Ok(())
}

#[sim_test]
async fn test_transfer_serialized_kind() -> Result<(), anyhow::Error> {
    // Like `test_transfer` but the transaction is pre-generated and serialized into a
    // Base64 string containing a Base64-encoded TransactionKind.
    let (mut cluster, client, rgp, o, _, a) = test_cluster_helper().await;
    let context = &mut cluster.wallet;

    // Build the transaction without running it.
    let transfer = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(a[1]),
        object_id: o[0],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs::default(),
        processing: TxProcessingArgs {
            serialize_unsigned_transaction: true,
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::SerializedUnsignedTransaction(tx_data) = transfer else {
        panic!("Expected SerializedUnsignedTransaction result");
    };

    let tx_bytes = Base64::encode(bcs::to_bytes(tx_data.kind())?);
    let transfer_serialized = SuiClientCommands::SerializedTxKind {
        tx_bytes,
        payment: PaymentArgs { gas: vec![o[1]] },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::TransactionBlock(response) = transfer_serialized else {
        panic!("Expected TransactionBlock result");
    };

    let Some(effects) = &response.effects else {
        panic!("TransactionBlock response should contain effects");
    };

    assert!(effects.status().is_ok());
    assert_eq!(effects.gas_object().object_id(), o[1]);

    let a1_objs = client
        .read_api()
        .get_owned_objects(a[1], None, None, None)
        .await?;

    assert!(!a1_objs.has_next_page);

    let page = a1_objs.data;
    assert_eq!(page.len(), 1);
    assert_eq!(page.first().unwrap().object().unwrap().object_id, o[0]);

    Ok(())
}

#[sim_test]
async fn test_gas_estimation() -> Result<(), anyhow::Error> {
    let (mut test_cluster, client, rgp, objects, _, addresses) = test_cluster_helper().await;
    let object_id1 = objects[0];
    let address2 = addresses[0];
    let context = &mut test_cluster.wallet;
    let amount = 1000;
    let sender = context.active_address().unwrap();
    let tx_builder = client.transaction_builder();
    let tx_kind = tx_builder.transfer_sui_tx_kind(address2, Some(amount));
    let gas_estimate = estimate_gas_budget(context, sender, tx_kind, rgp, vec![], None).await;
    assert!(gas_estimate.is_ok());

    let transfer_sui_cmd = SuiClientCommands::TransferSui {
        to: KeyIdentity::Address(address2),
        sui_coin_object_id: object_id1,
        amount: Some(amount),
        gas_data: GasDataArgs::default(),
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await
    .unwrap();
    if let SuiClientCommandResult::TransactionBlock(response) = transfer_sui_cmd {
        assert!(response.status_ok().unwrap());
        let gas_used = response.effects.as_ref().unwrap().gas_object().object_id();
        assert_eq!(gas_used, object_id1);
        assert!(
            response
                .effects
                .as_ref()
                .unwrap()
                .gas_cost_summary()
                .gas_used()
                <= gas_estimate.unwrap()
        );
    } else {
        panic!("TransferSui test failed");
    }
    Ok(())
}

#[sim_test]
async fn test_custom_sender() -> Result<(), anyhow::Error> {
    let (mut cluster, client, rgp, o, _, a) = test_cluster_helper().await;

    let custom_sender = cluster.wallet_mut().active_address().unwrap();
    let context = &mut cluster.wallet;

    // Build the transaction without running it.
    let transfer = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(a[1]),
        object_id: o[0],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs::default(),
        processing: TxProcessingArgs {
            serialize_unsigned_transaction: true,
            sender: Some(custom_sender),
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::SerializedUnsignedTransaction(tx_data) = transfer else {
        panic!("Expected SerializedUnsignedTransaction result");
    };

    let tx_bytes = Base64::encode(bcs::to_bytes(tx_data.kind())?);
    let transfer_serialized = SuiClientCommands::SerializedTxKind {
        tx_bytes,
        payment: PaymentArgs { gas: vec![o[1]] },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::TransactionBlock(response) = transfer_serialized else {
        panic!("Expected TransactionBlock result");
    };

    assert_eq!(response.transaction.unwrap().data.sender(), &custom_sender);

    let Some(effects) = &response.effects else {
        panic!("TransactionBlock response should contain effects");
    };

    assert!(effects.status().is_ok());
    assert_eq!(effects.gas_object().object_id(), o[1]);

    let a1_objs = client
        .read_api()
        .get_owned_objects(a[1], None, None, None)
        .await?;

    assert!(!a1_objs.has_next_page);

    let page = a1_objs.data;
    assert_eq!(page.len(), 1);
    assert_eq!(page.first().unwrap().object().unwrap().object_id, o[0]);

    // set sender to another address to which we don't have keys and it should fail

    let custom_sender = SuiAddress::random_for_testing_only();

    // Build the transaction without running it.
    let transfer = SuiClientCommands::Transfer {
        to: KeyIdentity::Address(a[1]),
        object_id: o[0],
        payment: PaymentArgs { gas: vec![o[1]] },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs {
            serialize_unsigned_transaction: true,
            sender: Some(custom_sender),
            ..Default::default()
        },
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::SerializedUnsignedTransaction(tx_data) = transfer else {
        panic!("Expected SerializedUnsignedTransaction result");
    };

    let tx_bytes = Base64::encode(bcs::to_bytes(tx_data.kind())?);
    let transfer_serialized = SuiClientCommands::SerializedTxKind {
        tx_bytes,
        payment: PaymentArgs { gas: vec![o[1]] },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await;

    // wrong gas objects, not owned by custom sender
    assert!(transfer_serialized.is_err());

    Ok(())
}

#[sim_test]
async fn test_clever_errors() -> Result<(), anyhow::Error> {
    // Publish the package
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let address = test_cluster.get_address_0();
    let context = &mut test_cluster.wallet;
    let client = context.get_client().await?;
    let object_refs = client
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    // Check log output contains all object ids.
    let gas_obj_id = object_refs.first().unwrap().object().unwrap().object_id;

    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("clever_errors");
    let build_config = BuildConfig::new_for_testing().config;
    let resp = SuiClientCommands::TestPublish(TestPublishArgs {
        publish_args: PublishArgs {
            package_path: package_path.clone(),
            build_config,
            skip_dependency_verification: false,
            verify_deps: true,
            with_unpublished_dependencies: false,
            payment: PaymentArgs {
                gas: vec![gas_obj_id],
            },
            gas_data: GasDataArgs {
                gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
                ..Default::default()
            },
            processing: TxProcessingArgs::default(),
        },
        build_env: Some("testnet".to_string()),
        pubfile_path: Some(tempdir()?.path().join("localnet.toml")),
    })
    .execute(context)
    .await?;

    // Print it out to CLI/logs
    resp.print(true);

    let SuiClientCommandResult::TransactionBlock(response) = resp else {
        unreachable!("Invalid response");
    };

    let SuiTransactionBlockEffects::V1(effects) = response.effects.unwrap();

    assert!(effects.status.is_ok());
    assert_eq!(effects.gas_object().object_id(), gas_obj_id);
    let package = effects
        .created()
        .iter()
        .find(|refe| matches!(refe.owner, Owner::Immutable))
        .unwrap();

    let elide_transaction_digest = |s: String| -> String {
        let mut x = s.splitn(5, '\'').collect::<Vec<_>>();
        x[1] = "ELIDED_TRANSACTION_DIGEST";
        let tmp = format!("ELIDED_ADDRESS{}", &x[3][66..]);
        x[3] = &tmp;
        x.join("'")
    };

    // Normal abort
    let non_clever_abort = SuiClientCommands::Call {
        package: package.reference.object_id,
        module: "clever_errors".to_string(),
        function: "aborter".to_string(),
        type_args: vec![],
        args: vec![],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await
    .unwrap_err();

    // Line-only abort
    let line_only_abort = SuiClientCommands::Call {
        package: package.reference.object_id,
        module: "clever_errors".to_string(),
        function: "aborter_line_no".to_string(),
        type_args: vec![],
        args: vec![],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await
    .unwrap_err();

    // Full clever error with utf-8 string
    let clever_error_utf8 = SuiClientCommands::Call {
        package: package.reference.object_id,
        module: "clever_errors".to_string(),
        function: "clever_aborter".to_string(),
        type_args: vec![],
        args: vec![],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await
    .unwrap_err();

    // Full clever error with non-utf-8 string
    let clever_error_non_utf8 = SuiClientCommands::Call {
        package: package.reference.object_id,
        module: "clever_errors".to_string(),
        function: "clever_aborter_not_a_string".to_string(),
        type_args: vec![],
        args: vec![],
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await
    .unwrap_err();

    let error_string = format!(
        "Non-clever-abort\n---\n{}\n---\nLine-only-abort\n---\n{}\n---\nClever-error-utf8\n---\n{}\n---\nClever-error-non-utf8\n---\n{}\n---\n",
        elide_transaction_digest(non_clever_abort.to_string()),
        elide_transaction_digest(line_only_abort.to_string()),
        elide_transaction_digest(clever_error_utf8.to_string()),
        elide_transaction_digest(clever_error_non_utf8.to_string())
    );

    insta::assert_snapshot!(error_string);
    Ok(())
}

#[tokio::test]
async fn test_parse_host_port() {
    let input = "127.0.0.0";
    let result = parse_host_port(input.to_string(), 9123).unwrap();
    assert_eq!(result, "127.0.0.0:9123".parse::<SocketAddr>().unwrap());

    let input = "127.0.0.5:9124";
    let result = parse_host_port(input.to_string(), 9123).unwrap();
    assert_eq!(result, "127.0.0.5:9124".parse::<SocketAddr>().unwrap());

    let input = "9090";
    let result = parse_host_port(input.to_string(), 9123).unwrap();
    assert_eq!(result, "0.0.0.0:9090".parse::<SocketAddr>().unwrap());

    let input = "";
    let result = parse_host_port(input.to_string(), 9123).unwrap();
    assert_eq!(result, "0.0.0.0:9123".parse::<SocketAddr>().unwrap());

    let result = parse_host_port("localhost".to_string(), 9899).unwrap();
    assert_eq!(result, "127.0.0.1:9899".parse::<SocketAddr>().unwrap());

    let input = "asg";
    assert!(parse_host_port(input.to_string(), 9123).is_err());
    let input = "127.0.0:900";
    assert!(parse_host_port(input.to_string(), 9123).is_err());
    let input = "127.0.0";
    assert!(parse_host_port(input.to_string(), 9123).is_err());
    let input = "127.";
    assert!(parse_host_port(input.to_string(), 9123).is_err());
    let input = "127.9.0.1:asb";
    assert!(parse_host_port(input.to_string(), 9123).is_err());
}

#[sim_test]
async fn test_tree_shaking_package_with_unpublished_deps() -> Result<(), anyhow::Error> {
    let mut test = TreeShakingTest::new().await.unwrap();
    let chain_id = test.client.read_api().get_chain_identifier().await.unwrap();
    let _ = update_toml_with_localnet_chain_id(&test.package_path("H"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("G"), chain_id.clone());
    // A package and with unpublished deps
    let (package_id, _) = test.publish_package("H", true).await.unwrap();

    // set with_unpublished_dependencies to true and publish package H
    let linkage_table_h = test.fetch_linkage_table(package_id).await;
    // H depends on G, which is unpublished, so the linkage table should be empty as G will be
    // included in H during publishing
    assert!(linkage_table_h.is_empty());

    // try publish package H but `with_unpublished_dependencies` is false. Should error
    let resp = test.test_publish_package("H", false).await;
    assert!(resp.is_err());

    Ok(())
}

#[sim_test]
async fn test_tree_shaking_package_without_dependencies() -> Result<(), anyhow::Error> {
    let mut test = TreeShakingTest::new().await?;

    // Publish package A and verify empty linkage table
    let (package_a_id, _) = test.test_publish_package("A", false).await?;
    let move_pkg_a = fetch_move_packages(&test.client, vec![package_a_id]).await;
    let linkage_table_a = move_pkg_a.first().unwrap().linkage_table();
    assert!(
        linkage_table_a.is_empty(),
        "Package A should have no dependencies"
    );

    Ok(())
}

#[sim_test]
async fn test_tree_shaking_package_with_direct_dependency() -> Result<(), anyhow::Error> {
    let mut test = TreeShakingTest::new().await?;

    // First publish package A
    let (package_a_id, _) = test.test_publish_package("A", false).await?;

    // Then publish B which depends on A
    let (package_b_id, _) = test.test_publish_package("B_A", false).await?;
    let linkage_table_b = test.fetch_linkage_table(package_b_id).await;
    assert!(
        linkage_table_b.contains_key(&package_a_id),
        "Package B should depend on A"
    );

    Ok(())
}

#[sim_test]
async fn test_tree_shaking_package_with_unused_dependency() -> Result<(), anyhow::Error> {
    let mut test = TreeShakingTest::new().await?;

    // First publish package A
    let (_, _) = test.test_publish_package("A", false).await?;

    // Then publish B which declares but doesn't use A
    let (package_b_id, _) = test.test_publish_package("B_A1", false).await?;
    let linkage_table_b = test.fetch_linkage_table(package_b_id).await;
    assert!(
        linkage_table_b.is_empty(),
        "Package B should have empty linkage table when not using A"
    );

    Ok(())
}

#[sim_test]
async fn test_tree_shaking_package_with_transitive_dependencies1() -> Result<(), anyhow::Error> {
    let mut test = TreeShakingTest::new().await?;

    // Publish packages A and B
    let (package_a_id, _) = test.test_publish_package("A", false).await?;
    let (package_b_id, _) = test
        .test_publish_package(
            "B_A",
            false,
            // we need to use pkg A path here because that's where the published information will
            // be written to.
        )
        .await?;

    // Publish C which depends on B (which depends on A)
    let (package_c_id, _) = test.test_publish_package("C_B_A", false).await?;
    let linkage_table_c = test.fetch_linkage_table(package_c_id).await;

    assert!(
        linkage_table_c.contains_key(&package_a_id),
        "Package C should depend on A"
    );
    assert!(
        linkage_table_c.contains_key(&package_b_id),
        "Package C should depend on B"
    );
    assert_eq!(
        linkage_table_c.len(),
        2,
        "Package C should have exactly two dependencies"
    );

    Ok(())
}

#[sim_test]
async fn test_tree_shaking_package_with_transitive_dependencies_and_no_code_references()
-> Result<(), anyhow::Error> {
    // Publish package C_B with no code references_B and check the linkage table
    let mut test = TreeShakingTest::new().await?;

    // Publish packages A and B
    let (_, _) = test.test_publish_package("A", false).await?;
    let (_, _) = test.test_publish_package("B_A1", false).await?;

    // Publish C which depends on B_A1
    let (package_c_id, _) = test.test_publish_package("C_B", false).await?;
    let linkage_table_c = test.fetch_linkage_table(package_c_id).await;

    assert!(
        linkage_table_c.is_empty(),
        "Package C should have no dependencies"
    );

    Ok(())
}

#[sim_test]
async fn test_tree_shaking_package_deps_on_pkg_upgrade() -> Result<(), anyhow::Error> {
    let mut test = TreeShakingTest::new().await?;
    let chain_id = test.client.read_api().get_chain_identifier().await?;
    let _ = update_toml_with_localnet_chain_id(&test.package_path("A"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("A_v1"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("B_A"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("D_A"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("D_A_v1"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("E"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("E_A_v1"), chain_id);

    // Publish package A and B
    let (package_a_id, cap) = test.publish_package("A", false).await?;
    let (_, _) = test.publish_package("B_A", false).await?;

    // Upgrade package A (named A_v1)
    std::fs::copy(
        test.package_path("A").join("Published.toml"),
        test.package_path("A_v1").join("Published.toml"),
    )?;
    let package_a_v1_id = test.upgrade_package("A_v1", cap).await?;

    // Publish D which depends on A_v1 but no code references A
    let (package_d_id, _) = test.publish_package("D_A", false).await?;
    let linkage_table_d = test.fetch_linkage_table(package_d_id).await;

    assert!(
        linkage_table_d.is_empty(),
        "Package D should have no dependencies"
    );

    // Publish D which depends on A_v1 and code references it
    let (package_d_id, _) = test.publish_package("D_A_v1", false).await?;
    let linkage_table_d = test.fetch_linkage_table(package_d_id).await;

    assert!(
        linkage_table_d.contains_key(&package_a_id),
        "Package D should depend on A"
    );
    assert!(
        linkage_table_d
            .get(&package_a_id)
            .is_some_and(|x| x.upgraded_id == package_a_v1_id),
        "Package D should depend on A_v1 after upgrade, and the UpgradeInfo should have matching ids"
    );

    let (package_e_id, _) = test.publish_package("E_A_v1", false).await?;

    let linkage_table_e = test.fetch_linkage_table(package_e_id).await;
    assert!(
        linkage_table_e.is_empty(),
        "Package E should have no dependencies"
    );

    let (package_e_id, _) = test.publish_package("E", false).await?;

    let linkage_table_e = test.fetch_linkage_table(package_e_id).await;
    assert!(
        linkage_table_e.contains_key(&package_a_id),
        "Package E should depend on A"
    );

    Ok(())
}

#[sim_test]
async fn test_tree_shaking_package_deps_on_pkg_upgrade_1() -> Result<(), anyhow::Error> {
    let mut test = TreeShakingTest::new().await?;
    let chain_id = test.client.read_api().get_chain_identifier().await?;
    let _ = update_toml_with_localnet_chain_id(&test.package_path("A"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("A_v1"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("A_v2"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("I"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("D_A"), chain_id.clone());

    let env = Environment {
        name: "localnet".to_string(),
        id: chain_id.clone(),
    };

    let (package_a_id, cap) = test.publish_package("A", false).await?;
    eprintln!("package_a_id: {package_a_id}");
    // Upgrade package A (named A_v1)
    std::fs::copy(
        test.package_path("A").join("Published.toml"),
        test.package_path("A_v1").join("Published.toml"),
    )?;

    test.upgrade_package("A_v1", cap).await?;

    let (package_d_id, package_d_upgrade_cap) =
        test.publish_package_without_tree_shaking("D_A", &env).await;
    let linkage_table_d = test.fetch_linkage_table(package_d_id).await;
    eprintln!("linkage_table_d: {linkage_table_d:#?}");
    assert!(
        linkage_table_d.contains_key(&package_a_id),
        "Package D should depend on A"
    );

    // published package D without tree shaking, so we need to create the published file manually
    test.create_published_file(
        &test.package_path("D_A"),
        &package_d_id,
        &package_d_upgrade_cap,
    )
    .await?;

    // Upgrade package A (named A_v2)
    std::fs::copy(
        test.package_path("A_v1").join("Published.toml"),
        test.package_path("A_v2").join("Published.toml"),
    )?;
    let package_a_v2_id = test.upgrade_package("A_v2", cap).await?;

    // the old code for publishing a package from sui-test-transaction-builder does not know about
    // move.lock and so on, so we need to add manually the published-at address.

    let (package_i_id, _) = test.publish_package("I", false).await?;
    let linkage_table_i = test.fetch_linkage_table(package_i_id).await;
    assert!(
        linkage_table_i.contains_key(&package_a_id),
        "Package I linkage table should have A"
    );
    assert!(
        linkage_table_i
            .get(&package_a_id)
            .is_some_and(|x| x.upgraded_id == package_a_v2_id),
        "Package I should depend on A_v2 after upgrade, and the UpgradeInfo should have matching ids"
    );

    Ok(())
}

#[sim_test]
async fn test_tree_shaking_package_deps_on_pkg_upgrade_2() -> Result<(), anyhow::Error> {
    let mut test = TreeShakingTest::new().await?;
    let chain_id = test.client.read_api().get_chain_identifier().await?;
    let _ = update_toml_with_localnet_chain_id(&test.package_path("K"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("K_v2"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("L"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("M"), chain_id);

    // Publish package K
    let (package_k_id, cap) = test.publish_package("K", false).await?;
    // Upgrade package K (named K_v2)
    std::fs::copy(
        test.package_path("K").join("Published.toml"),
        test.package_path("K_v2").join("Published.toml"),
    )?;
    let package_k_v2_id = test.upgrade_package("K_v2", cap).await?;

    let (package_l_id, _) = test.publish_package("L", false).await?;
    let linkage_table_l = test.fetch_linkage_table(package_l_id).await;
    assert!(
        linkage_table_l.contains_key(&package_k_id),
        "Package L should depend on K"
    );

    let (package_m_id, _) = test.publish_package("M", false).await?;
    let linkage_table_m = test.fetch_linkage_table(package_m_id).await;
    assert!(
        linkage_table_m.contains_key(&package_k_id),
        "Package M should depend on K"
    );

    assert!(
        linkage_table_m
            .get(&package_k_id)
            .is_some_and(|x| x.upgraded_id == package_k_v2_id),
        "Package I should depend on A_v2 after upgrade, and the UpgradeInfo should have matching ids"
    );

    Ok(())
}

#[sim_test]
async fn test_tree_shaking_package_deps_on_pkg_upgrade_3() -> Result<(), anyhow::Error> {
    let mut test = TreeShakingTest::new().await?;
    let chain_id = test.client.read_api().get_chain_identifier().await?;
    let _ = update_toml_with_localnet_chain_id(&test.package_path("K"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("K_v2"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("L"), chain_id.clone());
    let _ = update_toml_with_localnet_chain_id(&test.package_path("M"), chain_id.clone());

    // This test is identic to #2, except it uses the old test-transaction-builder infrastructure
    // to publish a package without tree shaking. It is also unaware of automated address mgmt,
    // so this test sets up the published file manually.

    // Publish package K
    let (package_k_id, cap) = test.publish_package("K", false).await?;
    // Upgrade package K (named K_v2)
    std::fs::copy(
        test.package_path("K").join("Published.toml"),
        test.package_path("K_v2").join("Published.toml"),
    )?;
    let package_k_v2_id = test.upgrade_package("K_v2", cap).await?;

    let env = Environment {
        name: "localnet".to_string(),
        id: chain_id.clone(),
    };

    let (package_l_id, package_l_upgrade_cap) =
        test.publish_package_without_tree_shaking("L", &env).await;
    let linkage_table_l = test.fetch_linkage_table(package_l_id).await;
    assert!(
        linkage_table_l.contains_key(&package_k_id),
        "Package L should depend on K"
    );

    // published package L without tree shaking, so we need to create the published file manually
    test.create_published_file(
        &test.package_path("L"),
        &package_l_id,
        &package_l_upgrade_cap,
    )
    .await?;

    let (package_m_id, _) = test.publish_package("M", false).await?;
    let linkage_table_m = test.fetch_linkage_table(package_m_id).await;
    assert!(
        linkage_table_m.contains_key(&package_k_id),
        "Package M should depend on K"
    );

    assert!(
        linkage_table_m
            .get(&package_k_id)
            .is_some_and(|x| x.upgraded_id == package_k_v2_id),
        "Package I should depend on A_v2 after upgrade, and the UpgradeInfo should have matching ids"
    );

    Ok(())
}

#[sim_test]
async fn test_tree_shaking_package_system_deps() -> Result<(), anyhow::Error> {
    let mut test = TreeShakingTest::new().await?;

    // Publish package J and verify empty linkage table
    let (package_j_id, _) = test.test_publish_package("J", false).await?;
    let move_pkg_j = fetch_move_packages(&test.client, vec![package_j_id]).await;
    let linkage_table_j = move_pkg_j.first().unwrap().linkage_table();
    assert!(
        linkage_table_j.is_empty(),
        "Package J should have no dependencies"
    );

    Ok(())
}

#[sim_test]
async fn test_party_transfer() -> Result<(), anyhow::Error> {
    // TODO: this test override can be removed when party objects are enabled on mainnet.
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_enable_party_transfer_for_testing(true);
        config
    });

    let (mut test_cluster, client, rgp, objects, recipients, addresses) =
        test_cluster_helper().await;
    let (object_id1, object_id2) = (objects[0], objects[1]);
    let recipient1 = &recipients[0];
    let address2 = addresses[0];
    let context = &mut test_cluster.wallet;

    let party_transfer = SuiClientCommands::PartyTransfer {
        to: recipient1.clone(),
        object_id: object_id1,
        payment: PaymentArgs::default(),
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await?;

    let SuiClientCommandResult::TransactionBlock(response) = party_transfer else {
        panic!("PartyTransfer test failed");
    };

    assert!(response.status_ok().unwrap());
    assert_eq!(
        response.effects.as_ref().unwrap().gas_object().object_id(),
        object_id2
    );

    let object_read = client
        .read_api()
        .get_object_with_options(object_id1, SuiObjectDataOptions::full_content())
        .await?;

    let object_data = object_read.data.unwrap();
    let owner = object_data.owner.unwrap();

    let Owner::ConsensusAddressOwner {
        owner: owner_addr, ..
    } = owner
    else {
        panic!("Expected ConsensusAddressOwner but got different owner type");
    };

    assert_eq!(owner_addr, address2);
    Ok(())
}

#[sim_test]
async fn test_party_transfer_gas_object_as_transfer_object() -> Result<(), anyhow::Error> {
    // TODO: this test override can be removed when party objects are enabled on mainnet.
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_enable_party_transfer_for_testing(true);
        config
    });

    let (mut test_cluster, _client, rgp, objects, _recipients, addresses) =
        test_cluster_helper().await;
    let object_id1 = objects[0];
    let address2 = addresses[0];
    let context = &mut test_cluster.wallet;

    let party_transfer = SuiClientCommands::PartyTransfer {
        to: KeyIdentity::Address(address2),
        object_id: object_id1,
        payment: PaymentArgs {
            gas: vec![object_id1],
        },
        gas_data: GasDataArgs {
            gas_budget: Some(rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
            ..Default::default()
        },
        processing: TxProcessingArgs::default(),
    }
    .execute(context)
    .await;

    assert!(party_transfer.is_err());
    Ok(())
}

// Creates a temp directory in which the test pkg and framework packages are copied into
// so that we can run operations
fn create_temp_dir_with_framework_packages(
    // The "folder" name of the test pkg.
    test_pkg_name: &str,
    // Pass in the chain-id if we wanna set a non-test environment for tests.
    chain_id: Option<String>,
) -> Result<(TempDir, PathBuf), anyhow::Error> {
    let temp = tempdir()?;

    let tempdir = temp.path().to_path_buf();
    let pkg_path = &tempdir.join("test");

    copy_dir_all(PathBuf::from(TEST_DATA_DIR).join(test_pkg_name), pkg_path)
        .expect("to copy the test pkg from data dir to a temp dir");

    copy_dir_all(
        PathBuf::from("../sui-framework/packages"),
        tempdir.join("system-packages"),
    )?;

    if let Some(chain_id) = chain_id {
        let _ = update_toml_with_localnet_chain_id(pkg_path, chain_id);
    }

    Ok((temp, pkg_path.clone()))
}

fn update_toml_with_localnet_chain_id(package_path: &Path, chain_id: String) -> String {
    let orig_toml = std::fs::read_to_string(package_path.join("Move.toml")).unwrap();
    let mut toml = OpenOptions::new()
        .append(true)
        .open(package_path.join("Move.toml"))
        .unwrap();
    writeln!(
        toml,
        "{}",
        &format!("[environments]\nlocalnet=\"{chain_id}\"")
    )
    .unwrap();

    orig_toml
}

#[tokio::test]
async fn test_move_build_dump_bytecode_as_base64() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    let client_config_path = context.config.path();
    let client = context.get_client().await?;
    // we need to cache the chain id as it does not get automatically cached in TestClusterBuilder
    let chain_id = context.cache_chain_id(&client).await?;

    // Create temp directory with the test package and update the Move.toml with localnet chain id
    let (temp_dir, pkg_path) =
        create_temp_dir_with_framework_packages("dummy_modules_publish", Some(chain_id))?;
    let mut cmd = assert_cmd::Command::cargo_bin("sui").unwrap();
    cmd.arg("move")
        .arg("--client.config")
        .arg(client_config_path)
        .arg("build")
        .arg("--dump-bytecode-as-base64")
        .arg("--path")
        .arg(pkg_path.to_str().unwrap());

    let output = cmd.output().expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("Stderr: {}", stderr);

    // check that the output contains the right output; this was computed with the old CLI before
    // the new pkg system to ensure the new one's output is correct
    let expected_output = r#"{"modules":["oRzrCwYAAAAKAQAMAgwkAzAyBGIMBW59B+sByAEIswNgBpMEDwqiBAUMpwRLABIBDQIHAhECEwIUAAMCAAECBwEAAAIADAEAAQIBDAEAAQIEDAEAAQQFAgAFBgcAAAoAAQAACwIBAAARAwEAAQwBBgEAAggICQECAgsQEQEAAw4LAQEMAw8PAQEMBBAMDQADBQQHBgoHDgUHBxICCAAHCAUAAwcLBAEIAAMHCAUCCwQBCAAFAgsDAQgACwQBCAABCAYBCwEBCQABCAAHCQACCgIKAgoCCwEBCAYHCAUCCwQBCQALAwEJAAELAwEIAAEJAAEGCAUBBQELBAEIAAIJAAUDBwsEAQkAAwcIBQELAgEJAAELAgEIAARDb2luDENvaW5NZXRhZGF0YQZPcHRpb24MVFJVU1RFRF9DT0lOC1RyZWFzdXJ5Q2FwCVR4Q29udGV4dANVcmwEY29pbg9jcmVhdGVfY3VycmVuY3kLZHVtbXlfZmllbGQEaW5pdARtaW50BG5vbmUGb3B0aW9uFHB1YmxpY19mcmVlemVfb2JqZWN0D3B1YmxpY190cmFuc2ZlcgZzZW5kZXIIdHJhbnNmZXIMdHJ1c3RlZF9jb2luCnR4X2NvbnRleHQDdXJsAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACCgIIB1RSVVNURUQKAgEAAAIBCQEAAAAABBILADECBwAHAQcBOAAKATgBDAIMAwsCOAILAwsBLhEIOAMCAQEEAAEJCwALAQoCOAQLAi4RCDgFAgIBBAABBAsACwE4AwIA"],"dependencies":["0x0000000000000000000000000000000000000000000000000000000000000001","0x0000000000000000000000000000000000000000000000000000000000000002"],"digest":[116,71,103,38,103,86,151,240,229,223,244,179,42,122,231,174,91,111,66,161,82,255,105,49,217,76,108,41,249,110,214,137]}"#;

    // Simple contains check
    assert!(
        stdout.contains(expected_output),
        "Expected JSON not found in output. Output was:\n{}",
        stdout
    );

    temp_dir.close()?;
    Ok(())
}

#[tokio::test]
async fn test_move_build_dump_bytecode_as_base64_with_unpublished_deps() -> Result<(), anyhow::Error>
{
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    let client_config_path = context.config.path();
    let client = context.get_client().await?;
    let chain_id = context.cache_chain_id(&client).await?;

    // Create temp directory with the test package
    let (temp_dir, pkg_path) = create_temp_dir_with_framework_packages(
        "dummy_module_publish_with_unpublished_dependency",
        Some(chain_id.clone()),
    )?;
    // copy the unpublished dependency
    copy_dir_all(
        PathBuf::from(TEST_DATA_DIR).join("dummy_module_unpublished_dependency"),
        pkg_path
            .parent()
            .unwrap()
            .join("dummy_module_unpublished_dependency"),
    )?;
    update_toml_with_localnet_chain_id(
        &pkg_path
            .parent()
            .unwrap()
            .join("dummy_module_unpublished_dependency"),
        chain_id,
    );

    // try to build without passing --with-unpublished-dependencies; should fail
    let mut cmd = assert_cmd::Command::cargo_bin("sui").unwrap();
    cmd.arg("move")
        .arg("--client.config")
        .arg(client_config_path)
        .arg("build")
        .arg("--dump-bytecode-as-base64")
        .arg("--path")
        .arg(pkg_path.to_str().unwrap());

    let output = cmd.output().expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let expected_output = "The package has unpublished dependencies";
    assert!(
        stdout.contains(expected_output),
        "Expected to fail. Output was:\n{}",
        stdout
    );

    // build by passing --with-unpublished-dependencies; should fail
    let mut cmd = assert_cmd::Command::cargo_bin("sui").unwrap();
    cmd.arg("move")
        .arg("--client.config")
        .arg(client_config_path)
        .arg("build")
        .arg("--with-unpublished-dependencies")
        .arg("--dump-bytecode-as-base64")
        .arg("--path")
        .arg(pkg_path.to_str().unwrap());

    let output = cmd.output().expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let expected_output = r#"{"modules":["oRzrCwYAAAAGAQACAwIFBQcBBwgNCBUgDDUHAAAAAQAAAAAHaW52YWxpZARtYWluAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQQAAAECAA==","oRzrCwYAAAAGAQACAwIFBQcBBwgFCA0gDC0HAAAAAAAAAAAEbWFpbgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEEAAABAgA="],"dependencies":[],"digest":[251,6,57,223,220,227,253,129,151,82,18,74,115,140,93,99,17,131,143,75,136,154,202,251,185,60,187,107,11,151,91,34]}"#;
    assert!(
        stdout.contains(expected_output),
        "Mismatched ouptut: \nExpected:\n{}\n\nOutput was:\n{}",
        expected_output,
        stdout
    );

    temp_dir.close()?;
    Ok(())
}
