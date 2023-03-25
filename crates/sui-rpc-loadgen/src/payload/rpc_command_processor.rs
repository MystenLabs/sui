// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use futures::future::join_all;
use shared_crypto::intent::{Intent, IntentMessage};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_json_rpc_types::SuiTransactionResponseOptions;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::debug;

use crate::load_test::LoadTestConfig;
use sui_json_rpc_types::SuiTransactionEffectsAPI;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::ObjectID;
use sui_types::crypto::{EncodeDecodeBase64, Signature, SuiKeyPair};
use sui_types::messages::{ExecuteTransactionRequestType, Transaction};

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

        let mut signer_infos = vec![config.signer_info.clone(); config.num_threads];

        if let Some(info) = &config.signer_info {
            let coins = get_coin_ids(clients.first().unwrap(), info, config.num_threads).await;
            signer_infos
                .iter_mut()
                .zip(coins.into_iter())
                .for_each(|(info, coin)| {
                    info.as_mut().unwrap().gas_payment = Some(coin);
                });
        };

        Ok(command_payloads
            .into_iter()
            .enumerate()
            .map(|(i, command)| Payload {
                commands: vec![command], // note commands is also a vector
                signer_info: signer_infos[i].clone(),
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

async fn get_coin_ids(
    client: &SuiClient,
    signer_info: &SignerInfo,
    num_coins: usize,
) -> Vec<ObjectID> {
    let sender = signer_info.signer_address;
    let coin_page = client
        .coin_read_api()
        .get_coins(sender, None, None, None)
        .await
        .expect("Did you give gas coins to this address?");
    let coin_object_id = coin_page
        .data
        .first()
        .expect("Did you give gas coins to this address?")
        .coin_object_id;

    let split_coin_tx = client
        .transaction_builder()
        .split_coin_equal(
            sender,
            coin_object_id,
            num_coins as u64,
            None,
            signer_info.gas_budget.unwrap_or(DEFAULT_GAS_BUDGET),
        )
        .await
        .expect("Failed to construct split coin transaction");
    debug!("split_coin_tx {:?}", split_coin_tx);
    let keypair = SuiKeyPair::decode_base64(&signer_info.encoded_keypair)
        .expect("Decoding keypair should not fail");
    let signature = Signature::new_secure(
        &IntentMessage::new(Intent::default(), &split_coin_tx),
        &keypair,
    );
    debug!("signature {:?}", signature);

    let transaction_response = client
        .quorum_driver()
        .execute_transaction(
            Transaction::from_data(split_coin_tx, Intent::default(), vec![signature])
                .verify()
                .expect("signature error"),
            SuiTransactionResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .expect("Splitcoin transaction failed");
    debug!("split coin transaction response {transaction_response:?}");
    let mut results: Vec<ObjectID> = transaction_response
        .effects
        .unwrap_or_else(|| {
            panic!(
                "split coin transaction should have effects {}",
                transaction_response.digest
            )
        })
        .created()
        .iter()
        .map(|owned_object_ref| owned_object_ref.reference.object_id)
        .collect();
    results.push(coin_object_id);
    debug!("Split {coin_object_id} into {results:?} for gas payment");
    assert_eq!(results.len(), num_coins);
    results
}
