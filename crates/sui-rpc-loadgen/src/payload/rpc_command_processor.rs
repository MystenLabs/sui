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

pub(crate) const DEFAULT_GAS_BUDGET: u64 = 10_000;
pub(crate) const DEFAULT_LARGE_GAS_BUDGET: u64 = 100_000_000;

#[derive(Clone)]
pub struct RpcCommandProcessor {
    clients: Arc<RwLock<Vec<SuiClient>>>,
}

impl RpcCommandProcessor {
    pub async fn new(urls: &[String]) -> Self {
        let clients = join_all(urls.iter().map(|url| async {
            SuiClientBuilder::default()
                .max_concurrent_requests(usize::MAX)
                .request_timeout(Duration::from_secs(60))
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
                    config.num_threads * config.num_chunks_per_thread,
                    config.max_repeat as u64 + 1,
                )
                .await,
            )
        } else {
            None
        };

        let num_chunks = config.num_chunks_per_thread;
        Ok(command_payloads
            .into_iter()
            .enumerate()
            .map(|(i, command)| Payload {
                commands: vec![command], // note commands is also a vector
                signer_info: coins_and_keys
                    .as_ref()
                    .map(|(coins, encoded_keypair)| SignerInfo {
                        encoded_keypair: encoded_keypair.clone(),
                        gas_payment: Some(coins[num_chunks * i..(i + 1) * num_chunks].to_vec()),
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
    num_transactions_per_coin: u64,
) -> (Vec<ObjectID>, String) {
    // TODO: This is due to the limit of number of objects that can be created in a single transaction
    // we can bypass the limit to use multiple split coin transactions
    if num_coins > 2000 {
        panic!("num_threads * num_chunks_per_thread cannot exceed 2000, current val {num_coins}")
    }
    let num_coins = num_coins as u64;
    let primary_keypair = SuiKeyPair::decode_base64(&signer_info.encoded_keypair)
        .expect("Decoding keypair should not fail");
    let sender = SuiAddress::from(&primary_keypair.public());
    let (coin, balance) = get_coin_with_max_balance(client, sender).await;

    // We don't want to split coins in our primary address because we want to avoid having
    // a million coin objects in our address. We can also fetch directly from the faucet, but in
    // some environment that might not be possible when faucet resource is scarce
    let (burner_address, burner_keypair): (_, AccountKeyPair) = get_key_pair();
    let burner_keypair = SuiKeyPair::Ed25519(burner_keypair);
    // Some coins has an enormous amount of gas, we want to limit the max pay amount so that
    // we don't squander the entire balance
    // TODO(chris): consider reference gas price
    let max_pay_amount = num_transactions_per_coin * DEFAULT_GAS_BUDGET * num_coins;
    let amount_for_primary_coin =
        // we need to subtract the following
        // 1. gas fee(i.e., `DEFAULT_GAS_BUDGET`) for pay_sui from the primary address to the burner address
        // 2. gas fee(i.e., DEFAULT_LARGE_GAS_BUDGET) for splitting the primary coin into `num_coins`
        max_pay_amount.min(balance - DEFAULT_LARGE_GAS_BUDGET - DEFAULT_GAS_BUDGET);
    pay_sui(
        client,
        &primary_keypair,
        vec![coin],
        DEFAULT_GAS_BUDGET,
        vec![burner_address; 2],
        vec![amount_for_primary_coin, DEFAULT_LARGE_GAS_BUDGET],
    )
    .await;

    let coin_to_split =
        get_coin_with_balance(client, burner_address, amount_for_primary_coin).await;
    let mut results: Vec<ObjectID> =
        split_coins(client, &burner_keypair, coin_to_split, num_coins).await;
    results.push(coin_to_split);
    debug!("Split {coin_to_split} into {results:?} for gas payment");
    assert_eq!(results.len(), num_coins as usize);
    (results, burner_keypair.encode_base64())
}

async fn get_coin_with_max_balance(client: &SuiClient, address: SuiAddress) -> (ObjectID, u64) {
    let coins = get_sui_coin_ids(client, address).await;
    assert!(!coins.is_empty());
    coins.into_iter().max_by(|a, b| a.1.cmp(&b.1)).unwrap()
}

async fn get_coin_with_balance(client: &SuiClient, address: SuiAddress, balance: u64) -> ObjectID {
    let coins = get_sui_coin_ids(client, address).await;
    assert!(!coins.is_empty());
    coins.into_iter().find(|a| a.1 == balance).unwrap().0
}

// TODO: move this to the Rust SDK
async fn get_sui_coin_ids(client: &SuiClient, address: SuiAddress) -> Vec<(ObjectID, u64)> {
    match client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
    {
        Ok(page) => page
            .data
            .into_iter()
            .map(|c| (c.coin_object_id, c.balance))
            .collect::<Vec<_>>(),
        Err(e) => {
            panic!("get_sui_coin_ids error for address {address} {e}")
        }
    }
    // TODO: implement iteration over next page
}

async fn pay_sui(
    client: &SuiClient,
    keypair: &SuiKeyPair,
    input_coins: Vec<ObjectID>,
    gas_budget: u64,
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
) -> SuiTransactionResponse {
    let sender = SuiAddress::from(&keypair.public());
    let tx = client
        .transaction_builder()
        .pay(sender, input_coins, recipients, amounts, None, gas_budget)
        .await
        .expect("Failed to construct pay sui transaction");
    sign_and_execute(client, keypair, tx).await
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
        .split_coin_equal(
            sender,
            coin_to_split,
            num_coins,
            None,
            DEFAULT_LARGE_GAS_BUDGET,
        )
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

pub(crate) async fn sign_and_execute(
    client: &SuiClient,
    keypair: &SuiKeyPair,
    txn_data: TransactionData,
) -> SuiTransactionResponse {
    let signature =
        Signature::new_secure(&IntentMessage::new(Intent::default(), &txn_data), keypair);

    let transaction_response = match client
        .quorum_driver()
        .execute_transaction(
            Transaction::from_data(txn_data, Intent::default(), vec![signature])
                .verify()
                .expect("signature error"),
            SuiTransactionResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
    {
        Ok(response) => response,
        Err(e) => {
            panic!("sign_and_execute error {e}")
        }
    };

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
