// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use strum_macros::Display;
use strum_macros::EnumString;
use sui_core::authority::AuthorityState;
use sui_core::test_utils::to_sender_signed_transaction;
use sui_core::transaction_input_checker;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::TransactionDigest;
use sui_types::crypto::get_key_pair;
use sui_types::crypto::AccountKeyPair;
use sui_types::error::SuiResult;
use sui_types::gas::start_gas_metering;
use sui_types::gas::GasCostSummary;
use sui_types::gas::SuiGas;
use sui_types::gas::MAX_GAS_BUDGET;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::SingleTransactionKind;
use sui_types::messages::TransactionData;
use sui_types::messages::TransactionKind;
use sui_types::temporary_store::TemporaryStore;

const DEFAULT_COMPUTATION_GAS_UNIT_PRICE: u64 = 1;
const DEFAULT_STORAGE_GAS_UNIT_PRICE: u64 = 1;
const DEFAULT_STORAGE_REBATE: u64 = 0;

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

impl CommonTransactionCosts {
    pub fn is_shared_object_tx(&self) -> bool {
        matches!(
            self,
            CommonTransactionCosts::SharedCounterAssertValue
                | CommonTransactionCosts::SharedCounterIncrement
        )
    }
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
    computation_gas_unit_price: Option<u64>,
    storage_gas_unit_price: Option<u64>,
    mutated_object_sizes_after: Option<usize>,
    storage_rebate: SuiGas,
    temporary_store: &TemporaryStore<S>,
) -> SuiResult<GasCostSummary> {
    let computation_gas_unit_price =
        computation_gas_unit_price.unwrap_or(DEFAULT_COMPUTATION_GAS_UNIT_PRICE);
    let storage_gas_unit_price = storage_gas_unit_price.unwrap_or(DEFAULT_STORAGE_GAS_UNIT_PRICE);

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
    // Dummy gas obj
    let gas_obj = GasCoin::new(ObjectID::random(), 0).to_object(SequenceNumber::new());
    let mut total_input_obj_size: usize = temporary_store
        .objects()
        .values()
        .map(|obj| obj.object_size_for_gas_metering())
        .sum();
    total_input_obj_size += gas_obj.object_size_for_gas_metering();
    gas_status.charge_storage_read(total_input_obj_size)?;

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

    // The assumption here is that size number of output objects is roughly double at most
    // This assumption is not based on any real data. Feedback is welcomed
    let mutated_object_sizes_after = mutated_object_sizes_after.unwrap_or(total_input_obj_size * 2);

    // Step 6: charge for mutations, deletions, rebates: cannot be computed precisely, will approx
    // At this point we need to estimate the effects of mutating objects after execution
    // We need to use some approx this
    gas_status.charge_storage_mutation(
        total_input_obj_size,
        mutated_object_sizes_after,
        storage_rebate,
    )?;

    Ok(gas_status.summary(true))
}

pub async fn estimate_transaction_computation_cost(
    tx_data: TransactionData,
    state: Arc<AuthorityState>,
    computation_gas_unit_price: Option<u64>,
    storage_gas_unit_price: Option<u64>,
    mutated_object_sizes_after: Option<usize>,
    storage_rebate: Option<u64>,
) -> anyhow::Result<GasCostSummary> {
    // Make a dummy transaction
    let (_, keypair): (_, AccountKeyPair) = get_key_pair();
    let tx = to_sender_signed_transaction(tx_data, &keypair);

    let (_gas_status, input_objects) =
        transaction_input_checker::check_transaction_input(&state.db(), &tx.data().data).await?;
    let in_mem_temporary_store =
        TemporaryStore::new(state.db(), input_objects, TransactionDigest::random());

    estimate_transaction_inner(
        tx.into_inner().into_data().data.kind,
        computation_gas_unit_price,
        storage_gas_unit_price,
        mutated_object_sizes_after,
        SuiGas::new(storage_rebate.unwrap_or(DEFAULT_STORAGE_REBATE)),
        &in_mem_temporary_store,
    )
    .map_err(|e| anyhow!("{e}"))
}
