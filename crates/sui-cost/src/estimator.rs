// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::str::FromStr;
use strum_macros::Display;
use strum_macros::EnumString;
use sui_types::gas::GasCostSummary;
use sui_types::messages::SingleTransactionKind;
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
            SingleTransactionKind::Publish(_) => unsupported_tx_kind,
            SingleTransactionKind::Call(_) => unsupported_tx_kind,
            SingleTransactionKind::ChangeEpoch(_) => unsupported_tx_kind,
        },
        TransactionKind::Batch(_) => Err(anyhow!("Batch TXes not supported for estimator")),
    }
}

pub fn read_estimate_file(
) -> Result<BTreeMap<CommonTransactionCosts, GasCostSummary>, anyhow::Error> {
    let json_str = fs::read_to_string(&ESTIMATE_FILE).unwrap();

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
