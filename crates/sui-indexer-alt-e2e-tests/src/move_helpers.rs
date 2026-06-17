// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Generic helpers for publishing Move packages and executing programmable transactions
//! against a `TestCluster`. Used by integration test harnesses to avoid duplicating gas,
//! signing, and execute boilerplate.

use std::collections::BTreeSet;
use std::path::Path;

use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// Publish a Move package at `path` and return the new package ID.
pub async fn publish_package(cluster: &mut test_cluster::TestCluster, path: &Path) -> ObjectID {
    let sender = cluster.wallet.active_address().unwrap();
    let gas = gas_for(cluster).await;
    let rgp = cluster.wallet.get_reference_gas_price().await.unwrap();
    let tx = cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas, rgp)
                .publish_async(path.to_path_buf())
                .await
                .build(),
        )
        .await;
    let resp = cluster.wallet.execute_transaction_must_succeed(tx).await;
    resp.get_new_package_obj().unwrap().0
}

/// Sign and execute a programmable transaction. Returns the Base58-encoded transaction
/// digest and the resulting effects.
pub async fn execute_ptb(
    cluster: &mut test_cluster::TestCluster,
    ptb: ProgrammableTransactionBuilder,
) -> (String, TransactionEffects) {
    let sender = cluster.wallet.active_address().unwrap();
    let gas = gas_for(cluster).await;
    let rgp = cluster.wallet.get_reference_gas_price().await.unwrap();
    let tx_data =
        TransactionData::new_programmable(sender, vec![gas], ptb.finish(), DEFAULT_GAS_BUDGET, rgp);
    let tx = cluster.wallet.sign_transaction(&tx_data).await;
    let resp = cluster.wallet.execute_transaction_must_succeed(tx).await;
    (
        Base58::encode(*resp.effects.transaction_digest()),
        resp.effects,
    )
}

/// Get an unused gas object reference owned by the active address.
pub async fn gas_for(cluster: &mut test_cluster::TestCluster) -> ObjectRef {
    let sender = cluster.wallet.active_address().unwrap();
    cluster
        .wallet
        .gas_for_owner_budget(sender, DEFAULT_GAS_BUDGET, BTreeSet::new())
        .await
        .unwrap()
        .1
        .compute_object_reference()
}
