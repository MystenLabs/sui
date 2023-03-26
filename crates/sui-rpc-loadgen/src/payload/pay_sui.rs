// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::payload::rpc_command_processor::DEFAULT_GAS_BUDGET;
use crate::payload::{PaySui, ProcessPayload, RpcCommandProcessor, SignerInfo};
use async_trait::async_trait;
use shared_crypto::intent::{Intent, IntentMessage};
use sui_json_rpc_types::SuiTransactionResponseOptions;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{EncodeDecodeBase64, Signature, SuiKeyPair};
use sui_types::messages::{ExecuteTransactionRequestType, Transaction};
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

        let keypair =
            SuiKeyPair::decode_base64(&encoded_keypair).expect("Decoding keypair should not fail");
        let signer_address = SuiAddress::from(&keypair.public());

        debug!("Pay Sui to {recipient} with {amount} MIST with {gas_payment:?}");
        for client in clients.iter() {
            let transfer_tx = client
                .transaction_builder()
                .transfer_sui(
                    signer_address,
                    gas_payment.unwrap(),
                    gas_budget,
                    recipient,
                    Some(amount),
                )
                .await?;
            debug!("transfer_tx {:?}", transfer_tx);
            let signature = Signature::new_secure(
                &IntentMessage::new(Intent::default(), &transfer_tx),
                &keypair,
            );

            let transaction_response = client
                .quorum_driver()
                .execute_transaction(
                    Transaction::from_data(transfer_tx, Intent::default(), vec![signature])
                        .verify()?,
                    SuiTransactionResponseOptions::full_content(),
                    Some(ExecuteTransactionRequestType::WaitForLocalExecution),
                )
                .await?;

            debug!("transaction_response {transaction_response:?}");
        }

        Ok(())
    }
}
