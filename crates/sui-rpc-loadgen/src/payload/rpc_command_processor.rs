// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use futures::future::join_all;

use shared_crypto::intent::{Intent, IntentMessage};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionEffectsAPI, SuiTransactionResponse,
    SuiTransactionResponseOptions,
};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::debug;

use crate::load_test::LoadTestConfig;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{get_key_pair, AccountKeyPair, EncodeDecodeBase64, Signature, SuiKeyPair};
use sui_types::messages::{ExecuteTransactionRequestType, Transaction, TransactionData};

use crate::payload::{
    Command, CommandData, DryRun, GetCheckpoints, Payload, ProcessPayload, Processor, SignerInfo,
};

pub(crate) const DEFAULT_GAS_BUDGET: u64 = 100_000;

#[derive(Clone)]
pub struct RpcCommandProcessor {
    clients: Arc<RwLock<Vec<SuiClient>>>,
}

impl RpcCommandProcessor {
    pub async fn new(urls: &[String]) -> Self {
        let clients = join_all(urls.iter().map(|url| async {
            SuiClientBuilder::default()
                .max_concurrent_requests(usize::MAX)
                .request_timeout(Duration::from_secs(10))
                .build(url.clone())
                .await
                .unwrap()
        }))
        .await;

        Self {
            clients: Arc::new(RwLock::new(clients)),
        }
    }

    async fn process_command_data(
        &self,
        command: &CommandData,
        signer_info: &Option<SignerInfo>,
    ) -> Result<()> {
        match command {
            CommandData::DryRun(ref v) => self.process(v, signer_info).await,
            CommandData::GetCheckpoints(ref v) => self.process(v, signer_info).await,
            CommandData::PaySui(ref v) => self.process(v, signer_info).await,
            CommandData::QueryTransactions(ref v) => self.process(v, signer_info).await,
        }
    }

    pub(crate) async fn get_clients(&self) -> Result<Vec<SuiClient>> {
        let read = self.clients.read().await;
        Ok(read.clone())
    }
}

#[async_trait]
impl Processor for RpcCommandProcessor {
    async fn apply(&self, payload: &Payload) -> Result<()> {
        let commands = &payload.commands;
        for command in commands.iter() {
            let repeat_interval = command.repeat_interval;
            let repeat_n_times = command.repeat_n_times;
            for _ in 0..=repeat_n_times {
                let start_time = Instant::now();

                self.process_command_data(&command.data, &payload.signer_info)
                    .await?;

                let elapsed_time = start_time.elapsed();
                if elapsed_time < repeat_interval {
                    let sleep_duration = repeat_interval - elapsed_time;
                    sleep(sleep_duration).await;
                }
            }
        }
        Ok(())
    }

    async fn prepare(&self, config: &LoadTestConfig) -> Result<Vec<Payload>> {
        let clients = self.get_clients().await?;
        let command_payloads = match &config.command.data {
            CommandData::GetCheckpoints(data) => {
                if !config.divide_tasks {
                    vec![config.command.clone(); config.num_threads]
                } else {
                    divide_checkpoint_tasks(&clients, data, config.num_threads).await
                }
            }
            _ => vec![config.command.clone(); config.num_threads],
        };

        let coins_and_keys = if config.signer_info.is_some() {
            Some(
                prepare_new_signer_and_coins(
                    clients.first().unwrap(),
                    config.signer_info.as_ref().unwrap(),
                    config.num_threads,
                )
                .await,
            )
        } else {
            None
        };

        Ok(command_payloads
            .into_iter()
            .enumerate()
            .map(|(i, command)| Payload {
                commands: vec![command], // note commands is also a vector
                signer_info: coins_and_keys
                    .as_ref()
                    .map(|(coins, encoded_keypair)| SignerInfo {
                        encoded_keypair: encoded_keypair.clone(),
                        gas_payment: Some(coins[i]),
                        gas_budget: None,
                    }),
            })
            .collect())
    }
}

#[async_trait]
impl<'a> ProcessPayload<'a, &'a DryRun> for RpcCommandProcessor {
    async fn process(&'a self, _op: &'a DryRun, _signer_info: &Option<SignerInfo>) -> Result<()> {
        debug!("DryRun");
        Ok(())
    }
}

async fn divide_checkpoint_tasks(
    clients: &[SuiClient],
    data: &GetCheckpoints,
    num_chunks: usize,
) -> Vec<Command> {
    let start = data.start;
    let end = match data.end {
        Some(end) => end,
        None => {
            let end_checkpoints = join_all(clients.iter().map(|client| async {
                client
                    .read_api()
                    .get_latest_checkpoint_sequence_number()
                    .await
                    .expect("get_latest_checkpoint_sequence_number should not fail")
            }))
            .await;
            *end_checkpoints
                .iter()
                .max()
                .expect("get_latest_checkpoint_sequence_number should not return empty")
        }
    };

    let chunk_size = (end - start) / num_chunks as u64;
    (0..num_chunks)
        .map(|i| {
            let start_checkpoint = start + (i as u64) * chunk_size;
            let end_checkpoint = end.min(start + ((i + 1) as u64) * chunk_size);
            Command::new_get_checkpoints(
                start_checkpoint,
                Some(end_checkpoint),
                data.verify_transactions,
                data.verify_objects,
            )
        })
        .collect()
}

async fn prepare_new_signer_and_coins(
    client: &SuiClient,
    signer_info: &SignerInfo,
    num_coins: usize,
) -> (Vec<ObjectID>, String) {
    let primary_keypair = SuiKeyPair::decode_base64(&signer_info.encoded_keypair)
        .expect("Decoding keypair should not fail");
    let sender = SuiAddress::from(&primary_keypair.public());
    let coins = get_sui_coin_ids(client, sender).await;
    // one coin for splitting, the other coins for gas payment
    let coin_to_split: ObjectID = coins[0];
    let coin_for_split_gas: ObjectID = coins[1];

    // We don't want to split coins in our primary address because we want to avoid having
    // a million coin objects in our address. We can also fetch directly from the faucet, but in
    // some environment that might not be possible when faucet resource is scarce
    let (burner_address, burner_keypair): (_, AccountKeyPair) = get_key_pair();
    let burner_keypair = SuiKeyPair::Ed25519(burner_keypair);
    transfer_coin(client, &primary_keypair, coin_to_split, burner_address).await;
    transfer_coin(client, &primary_keypair, coin_for_split_gas, burner_address).await;

    let mut results: Vec<ObjectID> =
        split_coins(client, &burner_keypair, coin_to_split, num_coins as u64).await;
    results.push(coin_to_split);
    debug!("Split {coin_to_split} into {results:?} for gas payment");
    assert_eq!(results.len(), num_coins);
    (results, burner_keypair.encode_base64())
}

// TODO: move this to the Rust SDK
async fn get_sui_coin_ids(client: &SuiClient, address: SuiAddress) -> Vec<ObjectID> {
    match client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
    {
        Ok(page) => page
            .data
            .into_iter()
            .map(|c| c.coin_object_id)
            .collect::<Vec<_>>(),
        Err(e) => {
            panic!("get_sui_coin_ids error for address {address} {e}")
        }
    }
    // TODO: implement iteration over next page
}

async fn transfer_coin(
    client: &SuiClient,
    keypair: &SuiKeyPair,
    coin_id: ObjectID,
    recipient: SuiAddress,
) -> SuiTransactionResponse {
    let sender = SuiAddress::from(&keypair.public());
    let txn = client
        .transaction_builder()
        .transfer_object(sender, coin_id, None, DEFAULT_GAS_BUDGET, recipient)
        .await
        .expect("Failed to construct transfer coin transaction");
    sign_and_execute(client, keypair, txn).await
}

async fn split_coins(
    client: &SuiClient,
    keypair: &SuiKeyPair,
    coin_to_split: ObjectID,
    num_coins: u64,
) -> Vec<ObjectID> {
    let sender = SuiAddress::from(&keypair.public());
    let split_coin_tx = client
        .transaction_builder()
        .split_coin_equal(sender, coin_to_split, num_coins, None, DEFAULT_GAS_BUDGET)
        .await
        .expect("Failed to construct split coin transaction");
    sign_and_execute(client, keypair, split_coin_tx)
        .await
        .effects
        .unwrap()
        .created()
        .iter()
        .map(|owned_object_ref| owned_object_ref.reference.object_id)
        .collect::<Vec<_>>()
}

async fn sign_and_execute(
    client: &SuiClient,
    keypair: &SuiKeyPair,
    txn_data: TransactionData,
) -> SuiTransactionResponse {
    let signature =
        Signature::new_secure(&IntentMessage::new(Intent::default(), &txn_data), keypair);

    let transaction_response = client
        .quorum_driver()
        .execute_transaction(
            Transaction::from_data(txn_data, Intent::default(), vec![signature])
                .verify()
                .expect("signature error"),
            SuiTransactionResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap_or_else(|_| panic!("Execute Transaction Failed"));
    match &transaction_response.effects {
        Some(effects) => {
            if let SuiExecutionStatus::Failure { error } = effects.status() {
                panic!(
                    "Transaction {} failed with error: {}. Transaction Response: {:?}",
                    transaction_response.digest, error, &transaction_response
                );
            }
        }
        None => {
            panic!(
                "Transaction {} has no effects. Response {:?}",
                transaction_response.digest, &transaction_response
            );
        }
    };
    transaction_response
}
