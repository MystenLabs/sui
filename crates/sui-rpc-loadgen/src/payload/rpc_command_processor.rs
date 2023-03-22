// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use futures::future::join_all;
use shared_crypto::intent::{Intent, IntentMessage};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_json_rpc_types::{CheckpointId, SuiTransactionResponseOptions};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::log::warn;
use tracing::{debug, error};

use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{EncodeDecodeBase64, Signature, SuiKeyPair};
use sui_types::messages::{ExecuteTransactionRequestType, Transaction};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::payload::{
    CommandData, DryRun, GetCheckpoints, MultiGetTransactions, PaySui, Payload, ProcessPayload,
    Processor,
};

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
        keypair: &Option<SuiKeyPair>,
    ) -> Result<()> {
        match command {
            CommandData::DryRun(ref v) => self.process(v, keypair).await,
            CommandData::GetCheckpoints(ref v) => self.process(v, keypair).await,
            CommandData::MultiGetTransactions(ref v) => self.process(v, keypair).await,
            CommandData::PaySui(ref v) => self.process(v, keypair).await,
        }
    }

    async fn get_clients(&self) -> Result<Vec<SuiClient>> {
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
            let keypair = &payload
                .encoded_keypair
                .as_ref()
                .map(|k| SuiKeyPair::decode_base64(k).expect("Decoding keypair should not fail"));

            for _ in 0..=repeat_n_times {
                let start_time = Instant::now();

                self.process_command_data(&command.data, keypair).await?;

                let elapsed_time = start_time.elapsed();
                if elapsed_time < repeat_interval {
                    let sleep_duration = repeat_interval - elapsed_time;
                    sleep(sleep_duration).await;
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl<'a> ProcessPayload<'a, &'a DryRun> for RpcCommandProcessor {
    async fn process(&'a self, _op: &'a DryRun, _keypair: &Option<SuiKeyPair>) -> Result<()> {
        debug!("DryRun");
        Ok(())
    }
}

#[async_trait]
impl<'a> ProcessPayload<'a, &'a GetCheckpoints> for RpcCommandProcessor {
    async fn process(
        &'a self,
        op: &'a GetCheckpoints,
        _keypair: &Option<SuiKeyPair>,
    ) -> Result<()> {
        let clients = self.get_clients().await?;

        let end_checkpoints: Vec<CheckpointSequenceNumber> =
            join_all(clients.iter().map(|client| async {
                match op.end {
                    Some(e) => e,
                    None => client
                        .read_api()
                        .get_latest_checkpoint_sequence_number()
                        .await
                        .expect("get_latest_checkpoint_sequence_number should not fail"),
                }
            }))
            .await;

        // The latest `latest_checkpoint` among all rpc servers
        let max_checkpoint = end_checkpoints
            .iter()
            .max()
            .expect("get_latest_checkpoint_sequence_number should not return empty");

        debug!("GetCheckpoints({}, {:?})", op.start, max_checkpoint);
        for seq in op.start..=*max_checkpoint {
            let checkpoints = join_all(clients.iter().enumerate().map(|(i, client)| {
                let end_checkpoints = end_checkpoints.clone();
                async move {

                    if end_checkpoints[i] < seq {
                        // TODO(chris) log actual url
                        warn!(
                        "The RPC server corresponding to the {i}th url has a outdated checkpoint number {}.\
                         The latest checkpoint number is {seq}",
                        end_checkpoints[i]
                    );
                        None
                    } else {
                        let start_time = Instant::now();
                        let checkpoint = match client
                            .read_api()
                            .get_checkpoint(CheckpointId::SequenceNumber(seq))
                            .await {
                            Ok(t) => {
                                if t.sequence_number != seq {
                                    error!("The RPC server corresponding to the {i}th url has unexpected checkpoint sequence number {}, expected {seq}", t.sequence_number,);
                                }
                                Some(t)
                            },
                            Err(err) => {
                                error!("Failed to fetch checkpoint {seq} on the {i}th url: {err}");
                                None
                            }
                        };
                        let elapsed_time = start_time.elapsed();

                        println!(
                            "GetCheckpoint Request latency {:.4}",
                            elapsed_time.as_secs_f64(),
                        );
                        checkpoint
                    }
                }
            }))
            .await;

            // TODO(chris): read `cross_validate` from config
            let cross_validate = true;
            if cross_validate {
                let (valid_checkpoint_idx, valid_checkpoint) = checkpoints
                    .iter()
                    .enumerate()
                    .find_map(|(i, x)| {
                        if x.is_some() {
                            Some((i, x.clone().unwrap()))
                        } else {
                            None
                        }
                    })
                    .expect("There should be at least one RPC server returning a checkpoint");
                for (i, x) in checkpoints.iter().enumerate() {
                    if i == valid_checkpoint_idx {
                        continue;
                    }
                    // ignore the None value because it's warned above
                    let eq = x.is_none() || x.as_ref().unwrap() == &valid_checkpoint;
                    if !eq {
                        error!("getCheckpoint {seq} has a different result between the {valid_checkpoint_idx}th and {i}th URL {:?} {:?}", x, checkpoints[valid_checkpoint_idx])
                    }
                }
            }

            if seq % 100 == 0 {
                debug!("Finished processing checkpoint {seq}")
            }
        }

        Ok(())
    }
}

#[async_trait]
impl<'a> ProcessPayload<'a, &'a MultiGetTransactions> for RpcCommandProcessor {
    async fn process(
        &'a self,
        _op: &'a MultiGetTransactions,
        _keypair: &Option<SuiKeyPair>,
    ) -> Result<()> {
        debug!("MultiGetTransactions");
        Ok(())
    }
}

#[async_trait]
impl<'a> ProcessPayload<'a, &'a PaySui> for RpcCommandProcessor {
    async fn process(&'a self, _op: &'a PaySui, keypair: &Option<SuiKeyPair>) -> Result<()> {
        let clients = self.get_clients().await?;

        // TODO(chris): allow customization and clean up this function
        let sender: SuiAddress =
            "0xbc33e6e4818f9f2ef77d020b35c24be738213e64d9e58839ee7b4222029610de"
                .parse()
                .unwrap();
        let recipient = SuiAddress::random_for_testing_only();
        let amount = 11;
        let gas_budget = 10000;

        let coin_page = clients
            .first()
            .unwrap()
            .coin_read_api()
            .get_coins(sender, None, None, None)
            .await?;
        let gas_object_id = coin_page
            .data
            .first()
            .expect("Did you give gas coins to this address?")
            .coin_object_id;

        debug!("Pay Sui to {recipient} with {amount} MIST with {gas_object_id}");
        let keypair = keypair.as_ref().unwrap();
        for client in clients.iter() {
            let transfer_tx = client
                .transaction_builder()
                .transfer_sui(sender, gas_object_id, gas_budget, recipient, Some(amount))
                .await?;
            debug!("transfer_tx {:?}", transfer_tx);
            let signature = Signature::new_secure(
                &IntentMessage::new(Intent::default(), &transfer_tx),
                keypair,
            );
            debug!("signature {:?}", signature);

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
