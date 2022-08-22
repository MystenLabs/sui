// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use sui_json_rpc_types::{SuiExecuteTransactionResponse, SuiGasCostSummary};
use sui_types::crypto::SignatureScheme;
use sui_types::messages::{ExecuteTransactionRequest, ExecuteTransactionRequestType, TransactionKind};
use sui_types::sui_serde::Base64;
use sui_types::{
    crypto,
    crypto::SignableBytes,
    messages::{Transaction, TransactionData},
};

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::io::{self, BufRead};
use std::path::Path;
use std::str::FromStr;
use toml::{map::Map, Value};
use sui_types::messages::SingleTransactionKind;
use crate::empirical_transaction_cost::CommonTransactionCosts;
pub const ESTIMATE_FILE: &str = "estimate.toml";

fn get_estimate(tx_kind: TransactionKind) -> Result<SuiGasCostSummary, anyhow::Error> {
    let toml_str = fs::read_to_string(&ESTIMATE_FILE).unwrap();

    let cost_map: BTreeMap<String, SuiGasCostSummary> = toml::from_str(&toml_str).unwrap();
    let cost_map: BTreeMap<CommonTransactionCosts, SuiGasCostSummary> = cost_map
        .iter()
        .map(|(k, v)| (CommonTransactionCosts::from_str(&k).unwrap(), v.clone()))
        .collect();

    let unsupported = Err(anyhow!("Batch TXes not supported for estimator"));

    match tx_kind {
        TransactionKind::Single(s) => match s {
            // Transfer obj does not depend on content
            SingleTransactionKind::TransferObject(_) => todo!(),
            SingleTransactionKind::TransferSui(_) => todo!(),

            SingleTransactionKind::Publish(_) => todo!(),
            SingleTransactionKind::Call(_) => todo!(),
            SingleTransactionKind::ChangeEpoch(_) => todo!(),
        },
        TransactionKind::Batch(_) => anyhow!("Batch TXes not supported for estimator"),
    }
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

#[cfg(test)]
mod test {
    use crate::empirical_transaction_cost::run_common_tx_costs;
    use std::{collections::BTreeMap, fmt::format, fs};
    use toml::{map::Map, Value};

    use super::ESTIMATE_FILE;

    #[tokio::test]
    async fn generate_estimates() {
        let common_costs: BTreeMap<_, _> = run_common_tx_costs()
            .await
            .unwrap()
            .iter()
            .map(|(k, v)| (format!("{k}"), v.clone()))
            .collect();

        let out_string = toml::to_string(&common_costs).unwrap();

        fs::write(ESTIMATE_FILE, out_string).expect("Could not write estimator to file!");
    }
}
