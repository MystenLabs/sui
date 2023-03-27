// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::payload::rpc_command_processor::{sign_and_execute, DEFAULT_GAS_BUDGET};
use crate::payload::{PaySui, ProcessPayload, RpcCommandProcessor, SignerInfo};
use async_trait::async_trait;
use futures::future::join_all;
use sui_json_rpc_types::SuiTransactionResponse;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{EncodeDecodeBase64, SuiKeyPair};
use tracing::debug;

#[async_trait]
impl<'a> ProcessPayload<'a, &'a PaySui> for RpcCommandProcessor {
    async fn process(
        &'a self,
        _op: &'a PaySui,
        signer_info: &Option<SignerInfo>,
    ) -> anyhow::Result<()> {
        let clients = self.get_clients().await?;
        let SignerInfo {
            encoded_keypair,
            gas_budget,
            gas_payment,
        } = signer_info.clone().unwrap();
        let recipient = SuiAddress::random_for_testing_only();
        let amount = 1;
        let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
        let gas_payments = gas_payment.unwrap();

        let keypair =
            SuiKeyPair::decode_base64(&encoded_keypair).expect("Decoding keypair should not fail");

        debug!(
            "Transfer Sui {} time to {recipient} with {amount} MIST with {gas_payments:?}",
            gas_payments.len()
        );
        for client in clients.iter() {
            join_all(gas_payments.iter().map(|gas| async {
                transfer_sui(client, &keypair, *gas, gas_budget, recipient, amount).await;
            }))
            .await;
        }

        Ok(())
    }
}

async fn transfer_sui(
    client: &SuiClient,
    keypair: &SuiKeyPair,
    gas_payment: ObjectID,
    gas_budget: u64,
    recipient: SuiAddress,
    amount: u64,
) -> SuiTransactionResponse {
    let sender = SuiAddress::from(&keypair.public());
    let tx = client
        .transaction_builder()
        .transfer_sui(sender, gas_payment, gas_budget, recipient, Some(amount))
        .await
        .expect("Failed to construct transfer coin transaction");
    sign_and_execute(client, keypair, tx).await
}
