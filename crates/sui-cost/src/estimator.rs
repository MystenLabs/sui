// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::empirical_transaction_cost::{run_common_tx_costs, CommonTransactionCosts};
use anyhow::anyhow;
use std::collections::BTreeMap;
use std::fs;
use std::str::FromStr;
use sui_json_rpc_types::SuiGasCostSummary;
use sui_types::messages::SingleTransactionKind;
use sui_types::messages::TransactionKind;
pub const ESTIMATE_FILE: &str = "estimate.toml";

fn get_estimate(tx_kind: TransactionKind) -> Result<SuiGasCostSummary, anyhow::Error> {
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
        TransactionKind::Batch(_) => return Err(anyhow!("Batch TXes not supported for estimator")),
    }
}

#[cfg(test)]
async fn generate_estimates() -> Result<(), anyhow::Error> {
    let common_costs: BTreeMap<_, _> = run_common_tx_costs()
        .await?
        .iter()
        .map(|(k, v)| (format!("{k}"), v.clone()))
        .collect();

    let out_string = toml::to_string(&common_costs).unwrap();

    fs::write(ESTIMATE_FILE, out_string).expect("Could not write estimator to file!");
    Ok(())
}

pub fn read_estimate_file(
) -> Result<BTreeMap<CommonTransactionCosts, SuiGasCostSummary>, anyhow::Error> {
    let toml_str = fs::read_to_string(&ESTIMATE_FILE).unwrap();

    let cost_map: BTreeMap<String, SuiGasCostSummary> = toml::from_str(&toml_str).unwrap();
    let cost_map: BTreeMap<CommonTransactionCosts, SuiGasCostSummary> = cost_map
        .iter()
        .map(|(k, v)| (CommonTransactionCosts::from_str(k).unwrap(), v.clone()))
        .collect();

    Ok(cost_map)
}

#[cfg(test)]
mod test {
    use sui_types::base_types::SuiAddress;
    use test_utils::{
        messages::make_transfer_sui_transaction, objects::test_gas_objects, test_account_keys,
    };

    use crate::empirical_transaction_cost::run_common_tx_costs;
    use std::{collections::BTreeMap, fs};

    use super::{generate_estimates, get_estimate, read_estimate_file, ESTIMATE_FILE};

    #[tokio::test]
    async fn check_estimates() {
        // Generate the estimates to file
        generate_estimates().await.unwrap();

        // Read the estimates
        let cost_map = read_estimate_file().unwrap();

        // Check that Sui Transfer estimate
        let mut gas_objects = test_gas_objects();
        let (sender, keypair) = test_account_keys().pop().unwrap();
        let whole_sui_coin_tx = make_transfer_sui_transaction(
            gas_objects.pop().unwrap().compute_object_reference(),
            SuiAddress::default(),
            None,
            sender,
            &keypair,
        );
        let partial_sui_coin_tx = make_transfer_sui_transaction(
            gas_objects.pop().unwrap().compute_object_reference(),
            SuiAddress::default(),
            Some(100),
            sender,
            &keypair,
        );

        let _ = get_estimate(whole_sui_coin_tx.signed_data.data.kind).unwrap();
        let _ = get_estimate(partial_sui_coin_tx.signed_data.data.kind).unwrap();
    }
}
