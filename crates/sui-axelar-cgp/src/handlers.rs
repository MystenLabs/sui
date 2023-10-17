// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::anyhow;
use axum::extract::State;
use axum::Json;

use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_sdk::rpc_types::SuiTransactionBlockResponseOptions;
use sui_sdk::types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_sdk::types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_sdk::types::transaction::{
    CallArg, ObjectArg, Transaction, TransactionData, TransactionKind,
};
use sui_sdk::types::Identifier;

use crate::types::{Error, Input, ProcessCommandsResponse};
use crate::RelayerState;

// handler for `/process_commands` endpoint
pub async fn process_commands(
    State(state): State<RelayerState>,
    Json(input): Json<Input>,
) -> Result<ProcessCommandsResponse, Error> {
    let payload = bcs::to_bytes(&input)?;

    println!("{payload:?}");

    let mut ptb = ProgrammableTransactionBuilder::default();
    let validator = ObjectArg::SharedObject {
        id: state.validators,
        initial_shared_version: state.validators_shared_version.into(),
        mutable: true,
    };
    ptb.move_call(
        state.gateway_package_id,
        Identifier::from_str("gateway")?,
        Identifier::from_str("process_commands")?,
        vec![],
        vec![
            CallArg::Object(validator),
            CallArg::Pure(bcs::to_bytes(&payload)?),
        ],
    )?;
    let pt = ptb.finish();

    // using read write lock to ensure same coins are not used in multiple tx simultaneously.
    // todo: this could become performance bottleneck, use coin management to increase throughput if needed.
    let sui_client = state.sui_client.write().await;

    let dry_run = sui_client
        .read_api()
        .dev_inspect_transaction_block(
            state.signer_address,
            TransactionKind::ProgrammableTransaction(pt.clone()),
            None,
            None,
        )
        .await?;

    println!("{dry_run:?}");

    let coins = sui_client
        .coin_read_api()
        .get_coins(state.signer_address, None, None, None)
        .await?
        .data;

    let coins = coins.into_iter().map(|c| c.object_ref()).collect();
    let gas_price = sui_client
        .governance_api()
        .get_reference_gas_price()
        .await?;

    let data =
        TransactionData::new_programmable(state.signer_address, coins, pt, 10000000, gas_price);

    let signature = state
        .keystore
        .sign_secure(&state.signer_address, &data, Intent::sui_transaction())
        .map_err(|e| anyhow!(e))?;

    let resp = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(data, Intent::sui_transaction(), vec![signature]),
            SuiTransactionBlockResponseOptions::default(),
            Some(ExecuteTransactionRequestType::WaitForEffectsCert),
        )
        .await?;

    // todo: invoke subsequence contract calls?

    // todo: deal with errors
    Ok(ProcessCommandsResponse {
        tx_hash: resp.digest,
    })
}
