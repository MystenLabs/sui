// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel_async::RunQueryDsl;
use simulacrum::Simulacrum;
use sui_indexer_alt::config::ConcurrentLayer;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt::config::PipelineLayer;
use sui_indexer_alt::config::PrunerLayer;
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_e2e_tests::find;
use sui_indexer_alt_schema::schema::obj_versions::dsl as ov;
use sui_pg_db::Db;
use sui_pg_db::DbArgs;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::Signature;
use sui_types::crypto::Signer;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::GasData;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionKind;

/// 5 SUI gas budget.
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

#[tokio::test]
async fn test_obj_versions_pruned() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        obj_versions: Some(concurrent_pipeline(2)),
        ..Default::default()
    })
    .await;

    let (sender, keypair, mut gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 20)
        .expect("Failed to create funded account");
    let recipient = SuiAddress::random_for_testing_only();
    let reader = Db::for_read(cluster.db_url(), DbArgs::default())
        .await
        .expect("Failed to connect to database");

    cluster.create_checkpoint().await;

    let object_id = gas.0;
    let mut versions = vec![gas.1.value() as i64];

    for _ in 0..2 {
        gas = transfer_sui(&mut cluster, sender, &keypair, gas, recipient);
        versions.push(gas.1.value() as i64);
        cluster.create_checkpoint().await;
    }

    assert_eq!(object_versions(&reader, object_id).await, versions);

    for _ in 0..2 {
        gas = transfer_sui(&mut cluster, sender, &keypair, gas, recipient);
        versions.push(gas.1.value() as i64);
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("obj_versions", 3, Duration::from_secs(10))
        .await
        .unwrap();

    assert_eq!(object_versions(&reader, object_id).await, versions[2..]);
}

#[tokio::test]
async fn test_multiple_versions_same_object_per_checkpoint() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        obj_versions: Some(concurrent_pipeline(2)),
        ..Default::default()
    })
    .await;

    let (sender, keypair, mut gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 20)
        .expect("Failed to create funded account");
    let recipient = SuiAddress::random_for_testing_only();
    let reader = Db::for_read(cluster.db_url(), DbArgs::default())
        .await
        .expect("Failed to connect to database");

    cluster.create_checkpoint().await;

    let object_id = gas.0;
    let mut versions = vec![gas.1.value() as i64];

    for _ in 0..4 {
        for _ in 0..2 {
            gas = transfer_sui(&mut cluster, sender, &keypair, gas, recipient);
            versions.push(gas.1.value() as i64);
        }
        cluster.create_checkpoint().await;
    }

    wait_for_obj_versions_pruner(&cluster, 3).await;
    assert_eq!(object_versions(&reader, object_id).await, versions[4..]);
}

#[tokio::test]
async fn test_multiple_objects_modified_in_same_checkpoint() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        obj_versions: Some(concurrent_pipeline(2)),
        ..Default::default()
    })
    .await;

    let (sender_a, keypair_a, mut gas_a) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 20)
        .expect("Failed to create first funded account");
    let (sender_b, keypair_b, mut gas_b) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 20)
        .expect("Failed to create second funded account");
    let recipient = SuiAddress::random_for_testing_only();
    let reader = Db::for_read(cluster.db_url(), DbArgs::default())
        .await
        .expect("Failed to connect to database");

    cluster.create_checkpoint().await;

    let object_id_a = gas_a.0;
    let object_id_b = gas_b.0;
    let mut versions_a = vec![gas_a.1.value() as i64];
    let mut versions_b = vec![gas_b.1.value() as i64];

    for _ in 0..4 {
        gas_a = transfer_sui(&mut cluster, sender_a, &keypair_a, gas_a, recipient);
        versions_a.push(gas_a.1.value() as i64);

        gas_b = transfer_sui(&mut cluster, sender_b, &keypair_b, gas_b, recipient);
        versions_b.push(gas_b.1.value() as i64);

        cluster.create_checkpoint().await;
    }

    wait_for_obj_versions_pruner(&cluster, 3).await;

    assert_eq!(object_versions(&reader, object_id_a).await, versions_a[2..]);
    assert_eq!(object_versions(&reader, object_id_b).await, versions_b[2..]);
}

#[tokio::test]
async fn test_deleted_object_versions_fully_pruned() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        obj_versions: Some(concurrent_pipeline(2)),
        ..Default::default()
    })
    .await;

    let (sender, keypair, mut gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 20)
        .expect("Failed to create funded account");
    let recipient = SuiAddress::random_for_testing_only();
    let reader = Db::for_read(cluster.db_url(), DbArgs::default())
        .await
        .expect("Failed to connect to database");

    cluster.create_checkpoint().await;

    let (sponsor, sponsor_keypair, sponsor_gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 5)
        .expect("Failed to create sponsor account");

    let created_coin = split_coin_sponsored(
        &mut cluster,
        sender,
        &keypair,
        gas,
        sender,
        1_000,
        sponsor,
        sponsor_gas,
        &sponsor_keypair,
    );
    gas = created_coin.0;
    let deleted_coin = created_coin.1;
    cluster.create_checkpoint().await;

    let (sponsor, sponsor_keypair, sponsor_gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 5)
        .expect("Failed to create second sponsor account");

    gas = merge_coin_sponsored(
        &mut cluster,
        sender,
        &keypair,
        gas,
        deleted_coin,
        sponsor,
        sponsor_gas,
        &sponsor_keypair,
    );
    cluster.create_checkpoint().await;

    let deleted_coin_versions = object_versions(&reader, deleted_coin.0).await;
    assert_eq!(deleted_coin_versions.len(), 2);

    for _ in 0..2 {
        gas = transfer_sui(&mut cluster, sender, &keypair, gas, recipient);
        cluster.create_checkpoint().await;
    }

    wait_for_obj_versions_pruner(&cluster, 3).await;
    assert!(object_versions(&reader, deleted_coin.0).await.is_empty());
}

async fn cluster_with_pipelines(pipeline: PipelineLayer) -> FullCluster {
    FullCluster::new_with_configs(
        Simulacrum::new(),
        OffchainClusterConfig {
            indexer_config: IndexerConfig {
                pipeline,
                ..IndexerConfig::for_test()
            },
            ..Default::default()
        },
        &prometheus::Registry::new(),
    )
    .await
    .expect("Failed to create cluster")
}

fn concurrent_pipeline(retention: u64) -> ConcurrentLayer {
    ConcurrentLayer {
        pruner: Some(PrunerLayer {
            retention: Some(retention),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn transfer_sui(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    signer: &dyn Signer<Signature>,
    gas: ObjectRef,
    recipient: SuiAddress,
) -> ObjectRef {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(recipient, Some(1));

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![signer]))
        .expect("Failed to execute transaction");

    assert!(fx.status().is_ok());
    fx.gas_object().unwrap().0
}

fn split_coin_sponsored(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    signer: &dyn Signer<Signature>,
    coin: ObjectRef,
    recipient: SuiAddress,
    amount: u64,
    sponsor: SuiAddress,
    sponsor_gas: ObjectRef,
    sponsor_signer: &dyn Signer<Signature>,
) -> (ObjectRef, ObjectRef) {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.split_coin(recipient, coin, vec![amount]);

    let gas_data = GasData {
        payment: vec![sponsor_gas],
        owner: sponsor,
        price: cluster.reference_gas_price(),
        budget: DEFAULT_GAS_BUDGET,
    };

    let data = TransactionData::new_with_gas_data(
        TransactionKind::ProgrammableTransaction(builder.finish()),
        sender,
        gas_data,
    );

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            data,
            vec![signer, sponsor_signer],
        ))
        .expect("Failed to execute sponsored split transaction");

    assert!(fx.status().is_ok());
    (
        find::address_mutated(&fx).unwrap(),
        find::address_owned_by(&fx, recipient).unwrap(),
    )
}

fn merge_coin_sponsored(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    signer: &dyn Signer<Signature>,
    target: ObjectRef,
    coin: ObjectRef,
    sponsor: SuiAddress,
    sponsor_gas: ObjectRef,
    sponsor_signer: &dyn Signer<Signature>,
) -> ObjectRef {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.merge_coins(target, vec![coin]).unwrap();

    let gas_data = GasData {
        payment: vec![sponsor_gas],
        owner: sponsor,
        price: cluster.reference_gas_price(),
        budget: DEFAULT_GAS_BUDGET,
    };

    let data = TransactionData::new_with_gas_data(
        TransactionKind::ProgrammableTransaction(builder.finish()),
        sender,
        gas_data,
    );

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            data,
            vec![signer, sponsor_signer],
        ))
        .expect("Failed to execute sponsored merge transaction");

    assert!(fx.status().is_ok());
    find::address_mutated(&fx).unwrap()
}

async fn wait_for_obj_versions_pruner(cluster: &FullCluster, checkpoint: u64) {
    cluster
        .wait_for_pruner("obj_versions", checkpoint, Duration::from_secs(10))
        .await
        .unwrap();
}

async fn object_versions(db: &Db, object_id: ObjectID) -> Vec<i64> {
    let mut conn = db.connect().await.expect("Failed to connect to database");
    ov::obj_versions
        .select(ov::object_version)
        .filter(ov::object_id.eq(object_id.to_vec()))
        .order_by(ov::object_version.asc())
        .load(&mut conn)
        .await
        .expect("Failed to query object versions")
}
