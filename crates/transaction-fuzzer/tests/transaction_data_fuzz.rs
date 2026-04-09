// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_macros::*;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::SuiError;
use sui_types::transaction::{Transaction, TransactionData, TransactionDataAPI};
use sui_types::utils::to_sender_signed_transaction;
use test_cluster::TestCluster;

use transaction_fuzzer::account_universe::AccountCurrent;
use transaction_fuzzer::account_universe::AccountData;
use transaction_fuzzer::addr_balance_fuzzer::{
    TxFuzzContext, addr_balance_transaction_data_strategy, fuzz_iterations,
    gasless_transaction_data_strategy,
};
use transaction_fuzzer::{
    executor::{ExecutionResult, Executor, assert_is_acceptable_result},
    transaction_data_gen::transaction_data_gen,
};

fn result_category(result: &ExecutionResult) -> String {
    match result {
        Ok(status) => format!("Ok({status:?})"),
        Err(e) => {
            let s = format!("{e:?}");
            if let Some(start) = s.find("error: ") {
                let rest = &s[start + 7..];
                if let Some(end) = rest.find(['{', '(', ' ']) {
                    return rest[..end].trim().to_string();
                }
                if rest.len() > 60 {
                    return rest[..60].to_string();
                }
                return rest.to_string();
            }
            if let Some(pos) = s.find('{') {
                s[..pos].trim().to_string()
            } else if s.len() > 80 {
                s[..80].to_string()
            } else {
                s
            }
        }
    }
}

fn format_results(results: &BTreeMap<String, u32>) -> String {
    let mut lines = Vec::new();
    for (cat, count) in results {
        lines.push(format!("  {count:>5}  {cat}"));
    }
    lines.join("\n")
}

fn assert_result_depth(
    results: &BTreeMap<String, u32>,
    min_categories: usize,
    min_success_divisor: u32,
    max_dominance_pct: u32,
) {
    let total: u32 = results.values().sum();
    let success: u32 = results
        .iter()
        .filter(|(k, _)| k.contains("Success"))
        .map(|(_, v)| *v)
        .sum();
    let max_single = *results.values().max().unwrap();

    let dist = format_results(results);
    assert!(
        results.len() >= min_categories,
        "Only {} result categories (need {}):\n{}",
        results.len(),
        min_categories,
        dist
    );
    assert!(
        success > total / min_success_divisor,
        "Success rate {}/{} below 1/{}:\n{}",
        success,
        total,
        min_success_divisor,
        dist
    );
    assert!(
        max_single * 100 <= total * max_dominance_pct,
        "Single category has {}/{} ({}%) results, max {}%:\n{}",
        max_single,
        total,
        max_single * 100 / total,
        max_dominance_pct,
        dist
    );
}

/// Fetch the randomness state object's initial shared version from the live cluster.
/// Returns `None` if the object isn't present (e.g. randomness disabled).
fn randomness_initial_shared_version(
    cluster: &TestCluster,
) -> Option<sui_types::base_types::SequenceNumber> {
    cluster.fullnode_handle.sui_node.with(|node| {
        sui_types::randomness_state::get_randomness_state_obj_initial_shared_version(
            node.state().get_object_store(),
        )
        .ok()
        .flatten()
    })
}

/// Fetch the cluster's current epoch.
fn current_epoch(cluster: &TestCluster) -> u64 {
    cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.state().epoch_store_for_testing().epoch())
}

/// Sign and execute a transaction, dual-signing if `sender != gas_owner` (sponsored).
async fn sign_and_execute(
    cluster: &TestCluster,
    tx_data: &TransactionData,
) -> Result<sui_types::execution_status::ExecutionStatus, SuiError> {
    let sender = tx_data.sender();
    let gas_owner = tx_data.gas_owner();
    let keystore = &cluster.wallet.config.keystore;
    let mut sigs = vec![
        keystore
            .sign_secure(&sender, tx_data, Intent::sui_transaction())
            .await
            .unwrap(),
    ];
    if sender != gas_owner {
        sigs.push(
            keystore
                .sign_secure(&gas_owner, tx_data, Intent::sui_transaction())
                .await
                .unwrap(),
        );
    }
    let signed_tx = Transaction::from_data(tx_data.clone(), sigs);
    cluster
        .execute_transaction_directly(&signed_tx)
        .await
        .map(|(_, effects)| effects.status().clone())
}

#[test]
#[cfg_attr(msim, ignore)]
fn all_random_transaction_data() {
    let mut exec = Executor::new();
    let account = AccountCurrent::new(AccountData::new_random());
    let strategy = transaction_data_gen(account.initial_data.account.address);
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    for _ in 0..1000 {
        let tx_data = strategy.new_tree(&mut runner).unwrap().current();
        let signed_txn = to_sender_signed_transaction(tx_data, &account.initial_data.account.key);
        let result = exec.execute_transaction(signed_txn);
        assert_is_acceptable_result(&result);
    }
}

#[sim_test]
async fn fuzz_addr_balance() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }
    use sui_test_transaction_builder::FundSource;
    use test_cluster::addr_balance_test_env::TestEnvBuilder;

    let mut test_env = TestEnvBuilder::new().build().await;

    let senders = test_env.get_all_senders();
    assert!(
        senders.len() >= 2,
        "fuzz test requires >=2 senders so sponsor != sender"
    );
    for &sender in &senders {
        test_env
            .fund_one_address_balance(sender, 100_000_000_000)
            .await;
    }

    // Publish a custom coin and fund all senders' address balances with it.
    // PT-input coin reservations and FundsWithdrawal use this non-SUI type, while
    // gas continues to use SUI.
    let (publisher, coin_type) = test_env.setup_custom_coin().await;
    let funding: Vec<_> = senders.iter().map(|&s| (1_000u64, s)).collect();
    let total: u64 = funding.iter().map(|(a, _)| a).sum();
    let tx = test_env
        .tx_builder(publisher)
        .transfer_funds_to_address_balance(
            FundSource::address_fund_with_reservation(total),
            funding,
            coin_type.clone(),
        )
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());

    let rgp = test_env.rgp;
    let chain = test_env.chain_id;
    let epoch = current_epoch(&test_env.cluster);
    let fund_type = Arc::new(coin_type);
    let randomness_isv = randomness_initial_shared_version(&test_env.cluster);

    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut results: BTreeMap<String, u32> = BTreeMap::new();
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let sender = senders[i % senders.len()];
        let sponsor = senders[(i + 1) % senders.len()];
        let tx_data = {
            let ctx = TxFuzzContext {
                sender,
                chain,
                epoch,
                reference_gas_price: rgp,
                fund_type: fund_type.clone(),
                sponsor: Some(sponsor),
                randomness_initial_shared_version: randomness_isv,
            };
            addr_balance_transaction_data_strategy(ctx)
                .new_tree(&mut runner)
                .unwrap()
                .current()
        };

        let result: ExecutionResult = sign_and_execute(&test_env.cluster, &tx_data).await;
        assert_is_acceptable_result(&result);
        *results.entry(result_category(&result)).or_default() += 1;
    }

    assert_result_depth(&results, 5, 20, 80);
}

#[sim_test]
async fn fuzz_gasless() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }
    use sui_test_transaction_builder::FundSource;
    use sui_types::transaction::add_gasless_token_for_testing;
    use test_cluster::addr_balance_test_env::TestEnvBuilder;

    let mut test_env = TestEnvBuilder::new().build().await;

    let senders = test_env.get_all_senders();
    assert!(
        senders.len() >= 2,
        "fuzz test requires >=2 senders so sponsor != sender"
    );

    // Publish a custom coin and fund all senders' address balances
    let (publisher, coin_type) = test_env.setup_custom_coin().await;
    let funding: Vec<_> = senders.iter().map(|&s| (1_000u64, s)).collect();
    let total: u64 = funding.iter().map(|(a, _)| a).sum();
    let tx = test_env
        .tx_builder(publisher)
        .transfer_funds_to_address_balance(
            FundSource::address_fund_with_reservation(total),
            funding,
            coin_type.clone(),
        )
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    add_gasless_token_for_testing(coin_type.to_canonical_string(true), 0);

    let rgp = test_env.rgp;
    let chain = test_env.chain_id;
    let epoch = current_epoch(&test_env.cluster);
    let fund_type = Arc::new(coin_type);

    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut results: BTreeMap<String, u32> = BTreeMap::new();
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let sender = senders[i % senders.len()];
        let sponsor = senders[(i + 1) % senders.len()];
        let tx_data = {
            let ctx = TxFuzzContext {
                sender,
                chain,
                epoch,
                reference_gas_price: rgp,
                fund_type: fund_type.clone(),
                sponsor: Some(sponsor),
                randomness_initial_shared_version: None,
            };
            gasless_transaction_data_strategy(ctx)
                .new_tree(&mut runner)
                .unwrap()
                .current()
        };

        let result: ExecutionResult = sign_and_execute(&test_env.cluster, &tx_data).await;
        assert_is_acceptable_result(&result);
        *results.entry(result_category(&result)).or_default() += 1;
    }

    assert_result_depth(&results, 3, 10, 80);
}
