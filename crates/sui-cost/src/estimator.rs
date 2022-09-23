// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::str::FromStr;
use std::sync::Arc;
use strum_macros::Display;
use strum_macros::EnumString;
use sui_adapter::temporary_store::TemporaryStore;
use sui_core::authority::AuthorityState;
use sui_core::transaction_input_checker;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::TransactionDigest;
use sui_types::crypto::get_key_pair;
use sui_types::crypto::AccountKeyPair;
use sui_types::crypto::Signature as SuiSignature;
use sui_types::crypto::ToFromBytes;
use sui_types::error::SuiResult;
use sui_types::gas::start_gas_metering;
use sui_types::gas::GasCostSummary;
use sui_types::gas::SuiGas;
use sui_types::gas::MAX_GAS_BUDGET;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::SingleTransactionKind;
use sui_types::messages::Transaction;
use sui_types::messages::TransactionData;
use sui_types::messages::TransactionKind;

pub const ESTIMATE_FILE: &str = "tests/snapshots/empirical_transaction_cost__good_snapshot.snap";
#[derive(
    Debug, Eq, PartialEq, Hash, Serialize, Deserialize, Ord, PartialOrd, Clone, Display, EnumString,
)]
pub enum CommonTransactionCosts {
    Publish,
    MergeCoin,
    SplitCoin(usize),
    TransferWholeCoin,
    TransferWholeSuiCoin,
    TransferPortionSuiCoin,
    SharedCounterCreate,
    SharedCounterAssertValue,
    SharedCounterIncrement,
}

pub fn estimate_computational_costs_for_transaction(
    tx_kind: TransactionKind,
) -> Result<GasCostSummary, anyhow::Error> {
    let cost_map = read_estimate_file()?;
    let unsupported_tx_kind = Err(anyhow!("Transaction kind not supported for estimator yet"));
    match tx_kind {
        TransactionKind::Single(s) => match s {
            SingleTransactionKind::TransferSui(t) => Ok(if t.amount.is_none() {
                cost_map.get(&CommonTransactionCosts::TransferWholeSuiCoin)
            } else {
                cost_map.get(&CommonTransactionCosts::TransferPortionSuiCoin)
            }
            .unwrap()
            .clone()),
            SingleTransactionKind::TransferObject(_) => unsupported_tx_kind,
            SingleTransactionKind::Pay(_) => unsupported_tx_kind,
            SingleTransactionKind::Publish(_) => unsupported_tx_kind,
            SingleTransactionKind::Call(_) => unsupported_tx_kind,
            SingleTransactionKind::ChangeEpoch(_) => unsupported_tx_kind,
        },
        TransactionKind::Batch(_) => Err(anyhow!("Batch TXes not supported for estimator")),
    }
}

pub fn read_estimate_file(
) -> Result<BTreeMap<CommonTransactionCosts, GasCostSummary>, anyhow::Error> {
    let json_str = fs::read_to_string(ESTIMATE_FILE).unwrap();

    // Remove the metadata: first 4 lines form snapshot tests
    let json_str = json_str
        .split('\n')
        .skip(4)
        .map(|q| q.to_string())
        .collect::<Vec<String>>()
        .join("\n");

    let cost_map: BTreeMap<String, GasCostSummary> = serde_json::from_str(&json_str).unwrap();

    let cost_map: BTreeMap<CommonTransactionCosts, GasCostSummary> = cost_map
        .iter()
        .map(|(k, v)| (CommonTransactionCosts::from_str(k).unwrap(), v.clone()))
        .collect();

    Ok(cost_map)
}

// Emulation-based estimator
// Step 1: charge min tx fee : can be computed precisely
// Step 2: if contains shared objs, charge consensus fee : can be computed precisely
// Step 3: charge storage read for all input objects : can be computed precisely
// Step 4: if package publish, charge for it per size of modules : can be computed precisely
// Step 5: charge VM flat fee if uses VM : can be computed precisely
// Step 6: charge for mutations, deletions, rebates: cannot be computed precisely, will approx
fn estimate_transaction_inner<S>(
    tx: TransactionKind,
    computation_gas_unit_price: u64,
    storage_gas_unit_price: u64,
    mutated_object_sizes_before: usize,
    mutated_object_sizes_after: usize,
    storage_rebate: SuiGas,
    temporary_store: &TemporaryStore<S>,
) -> SuiResult<GasCostSummary> {
    let mut gas_status = start_gas_metering(
        *MAX_GAS_BUDGET,
        computation_gas_unit_price,
        storage_gas_unit_price,
    )?;

    // Step 1: charge min tx fee : can be computed precisely
    gas_status.charge_min_tx_gas()?;
    // Step 2: if contains shared objs, charge consensus fee : can be computed precisely
    if tx.shared_input_objects().next().is_some() {
        gas_status.charge_consensus()?;
    }
    // Step 3: charge storage read for all input objects : can be computed precisely
    // dummy gas obj
    let gas_obj = GasCoin::new(ObjectID::random(), 0).to_object(SequenceNumber::new());
    let mut total_size: usize = temporary_store
        .objects()
        .values()
        .map(|obj| obj.object_size_for_gas_metering())
        .sum();
    total_size += gas_obj.object_size_for_gas_metering();
    gas_status.charge_storage_read(total_size)?;

    // Steps 4 & 5
    for single_tx in tx.single_transactions() {
        match single_tx {
            SingleTransactionKind::Publish(module) => {
                gas_status.charge_publish_package(module.modules.iter().map(|v| v.len()).sum())?
            }
            SingleTransactionKind::Call(_) => (),
            _ => continue,
        }

        // Charge for Call and Publish
        // Emulate charging for flat fee, pending: https://github.com/MystenLabs/sui/pull/4607
        gas_status.charge_vm_gas()?;
    }

    // Step 6: charge for mutations, deletions, rebates: cannot be computed precisely, will approx
    // At this point we need to estimate the effects of mutating objects after execution
    // We need to use some approx this
    gas_status.charge_storage_mutation(
        mutated_object_sizes_before,
        mutated_object_sizes_after,
        storage_rebate,
    )?;

    Ok(gas_status.summary(true))
}

pub async fn estimate_transaction_computation_cost(
    tx_data: TransactionData,
    state: Arc<AuthorityState>,
    computation_gas_unit_price: u64,
    storage_gas_unit_price: u64,
    mutated_object_sizes_before: usize,
    mutated_object_sizes_after: usize,
    storage_rebate: u64,
) -> anyhow::Result<GasCostSummary> {
    // Make a dummy transaction
    let (_, keypair): (_, AccountKeyPair) = get_key_pair();
    let dummy_sig = SuiSignature::new(&tx_data, &keypair);
    let tx = Transaction::new(tx_data, dummy_sig);

    let (_gas_status, input_objects) =
        transaction_input_checker::check_transaction_input(&state.db(), &tx).await?;

    let in_mem_temporary_store =
        TemporaryStore::new(state.db(), input_objects, TransactionDigest::random());

    estimate_transaction_inner(
        tx.signed_data.data.kind,
        computation_gas_unit_price,
        storage_gas_unit_price,
        mutated_object_sizes_before,
        mutated_object_sizes_after,
        SuiGas::new(storage_rebate),
        &in_mem_temporary_store,
    )
    .map_err(|e| anyhow!("{e}"))
}
