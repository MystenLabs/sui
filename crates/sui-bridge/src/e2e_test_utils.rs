// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::server::APPLICATION_JSON;
use crate::types::{AddTokensOnSuiAction, BridgeAction};
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_sdk::wallet_context::WalletContext;
use sui_types::bridge::{
    BridgeChainId, BRIDGE_MODULE_NAME, BRIDGE_REGISTER_FOREIGN_TOKEN_FUNCTION_NAME,
};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, TransactionData};
use sui_types::BRIDGE_PACKAGE_ID;

pub async fn publish_coins_return_add_coins_on_sui_action(
    wallet_context: &mut WalletContext,
    bridge_arg: ObjectArg,
    publish_coin_responses: Vec<SuiTransactionBlockResponse>,
    token_ids: Vec<u8>,
    token_prices: Vec<u64>,
    nonce: u64,
) -> BridgeAction {
    assert!(token_ids.len() == publish_coin_responses.len());
    assert!(token_prices.len() == publish_coin_responses.len());
    let sender = wallet_context.active_address().unwrap();
    let rgp = wallet_context.get_reference_gas_price().await.unwrap();
    let mut token_type_names = vec![];
    for response in publish_coin_responses {
        let object_changes = response.object_changes.unwrap();
        let mut tc = None;
        let mut type_ = None;
        let mut uc = None;
        let mut metadata = None;
        for object_change in &object_changes {
            if let o @ sui_json_rpc_types::ObjectChange::Created { object_type, .. } = object_change
            {
                if object_type.name.as_str().starts_with("TreasuryCap") {
                    assert!(tc.is_none() && type_.is_none());
                    tc = Some(o.clone());
                    type_ = Some(object_type.type_params.first().unwrap().clone());
                } else if object_type.name.as_str().starts_with("UpgradeCap") {
                    assert!(uc.is_none());
                    uc = Some(o.clone());
                } else if object_type.name.as_str().starts_with("CoinMetadata") {
                    assert!(metadata.is_none());
                    metadata = Some(o.clone());
                }
            }
        }
        let (tc, type_, uc, metadata) =
            (tc.unwrap(), type_.unwrap(), uc.unwrap(), metadata.unwrap());

        // register with the bridge
        let mut builder = ProgrammableTransactionBuilder::new();
        let bridge_arg = builder.obj(bridge_arg).unwrap();
        let uc_arg = builder
            .obj(ObjectArg::ImmOrOwnedObject(uc.object_ref()))
            .unwrap();
        let tc_arg = builder
            .obj(ObjectArg::ImmOrOwnedObject(tc.object_ref()))
            .unwrap();
        let metadata_arg = builder
            .obj(ObjectArg::ImmOrOwnedObject(metadata.object_ref()))
            .unwrap();
        builder.programmable_move_call(
            BRIDGE_PACKAGE_ID,
            BRIDGE_MODULE_NAME.into(),
            BRIDGE_REGISTER_FOREIGN_TOKEN_FUNCTION_NAME.into(),
            vec![type_.clone()],
            vec![bridge_arg, tc_arg, uc_arg, metadata_arg],
        );
        let pt = builder.finish();
        let gas = wallet_context
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap();
        let tx = TransactionData::new_programmable(sender, vec![gas], pt, 1_000_000_000, rgp);
        let signed_tx = wallet_context.sign_transaction(&tx);
        let _ = wallet_context
            .execute_transaction_must_succeed(signed_tx)
            .await;

        token_type_names.push(type_);
    }

    BridgeAction::AddTokensOnSuiAction(AddTokensOnSuiAction {
        nonce,
        chain_id: BridgeChainId::SuiLocalTest,
        native: false,
        token_ids,
        token_type_names,
        token_prices,
    })
}

pub async fn wait_for_server_to_be_up(server_url: String, timeout_sec: u64) -> anyhow::Result<()> {
    let now = std::time::Instant::now();
    loop {
        if let Ok(true) = reqwest::Client::new()
            .get(server_url.clone())
            .header(reqwest::header::ACCEPT, APPLICATION_JSON)
            .send()
            .await
            .map(|res| res.status().is_success())
        {
            break;
        }
        if now.elapsed().as_secs() > timeout_sec {
            anyhow::bail!("Server is not up and running after {} seconds", timeout_sec);
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    Ok(())
}
