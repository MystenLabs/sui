// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::future::join_all;
use shared_crypto::intent::{Intent, IntentMessage};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_json_rpc_types::{
    BigInt, CheckpointId, ObjectChange, Page, SuiObjectDataOptions, SuiTransactionResponse,
    SuiTransactionResponseOptions, SuiTransactionResponseQuery, TransactionsPage,
};
use sui_types::digests::TransactionDigest;
use sui_types::query::TransactionFilter;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::log::warn;
use tracing::{debug, error, info};

use crate::load_test::LoadTestConfig;
use sui_json_rpc_types::SuiTransactionEffectsAPI;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{EncodeDecodeBase64, Signature, SuiKeyPair};
use sui_types::messages::{ExecuteTransactionRequestType, Transaction};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::payload::{
    Command, CommandData, DryRun, GetCheckpoints, PaySui, Payload, ProcessPayload, Processor,
    QueryTransactions, SignerInfo,
};

const DEFAULT_GAS_BUDGET: u64 = 100_000;

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

#[async_trait]
impl<'a> ProcessPayload<'a, &'a GetCheckpoints> for RpcCommandProcessor {
    async fn process(
        &'a self,
        op: &'a GetCheckpoints,
        _signer_info: &Option<SignerInfo>,
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

        // TODO(chris): read `cross_validate` from config
        let cross_validate = true;

        for seq in op.start..=*max_checkpoint {
            let checkpoints = join_all(clients.iter().enumerate().map(|(i, client)| {
                let end_checkpoint_for_clients = end_checkpoints.clone();
                async move {
                    if end_checkpoint_for_clients[i] < seq {
                        // TODO(chris) log actual url
                        warn!(
                            "The RPC server corresponding to the {i}th url has a outdated checkpoint number {}.\
                            The latest checkpoint number is {seq}",
                            end_checkpoint_for_clients[i]
                        );
                        return None;
                    }

                    match client
                        .read_api()
                        .get_checkpoint(CheckpointId::SequenceNumber(<BigInt>::from(seq)))
                        .await {
                        Ok(t) => {
                            if t.sequence_number != <BigInt>::from(seq) {
                                error!("The RPC server corresponding to the {i}th url has unexpected checkpoint sequence number {}, expected {seq}", t.sequence_number,);
                            }
                            if op.verify_transactions {
                                check_transactions(&self.clients, &t.transactions, cross_validate, op.verify_objects).await;
                            }
                            Some(t)
                        },
                        Err(err) => {
                            error!("Failed to fetch checkpoint {seq} on the {i}th url: {err}");
                            None
                        }
                    }
                }
            }))
                .await;

            if cross_validate {
                let valid_checkpoint = checkpoints.iter().enumerate().find_map(|(i, x)| {
                    if x.is_some() {
                        Some((i, x.clone().unwrap()))
                    } else {
                        None
                    }
                });

                if valid_checkpoint.is_none() {
                    error!("none of the urls are returning valid checkpoint for seq {seq}");
                    continue;
                }
                // safe to unwrap because we check some above
                let (valid_checkpoint_idx, valid_checkpoint) = valid_checkpoint.unwrap();
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

            if seq % 10000 == 0 {
                info!("Finished processing checkpoint {seq}");
            }
        }

        Ok(())
    }
}

pub async fn check_transactions(
    clients: &Arc<RwLock<Vec<SuiClient>>>,
    digests: &[TransactionDigest],
    cross_validate: bool,
    verify_objects: bool,
) {
    let read = clients.read().await;
    let cloned_clients = read.clone();

    let transactions = join_all(
        cloned_clients
            .iter()
            .enumerate()
            .map(|(i, client)| async move {
                let start_time = Instant::now();
                let transactions = client
                    .read_api()
                    .multi_get_transactions_with_options(
                        digests.to_vec(),
                        SuiTransactionResponseOptions::full_content(), // todo(Will) support options for this
                    )
                    .await;
                let elapsed_time = start_time.elapsed();
                debug!(
                    "MultiGetTransactions Request latency {:.4} for rpc at url {i}",
                    elapsed_time.as_secs_f64()
                );
                transactions
            }),
    )
    .await;

    // TODO: support more than 2 transactions
    if cross_validate && transactions.len() == 2 {
        if let (Some(t1), Some(t2)) = (transactions.get(0), transactions.get(1)) {
            let first = match t1 {
                Ok(vec) => vec.as_slice(),
                Err(err) => {
                    error!("Error unwrapping first vec of transactions: {:?}", err);
                    error!("Logging digests, {:?}", digests);
                    return;
                }
            };
            let second = match t2 {
                Ok(vec) => vec.as_slice(),
                Err(err) => {
                    error!("Error unwrapping second vec of transactions: {:?}", err);
                    error!("Logging digests, {:?}", digests);
                    return;
                }
            };

            cross_validate_entities(digests, first, second, "TransactionDigest", "Transaction");

            if verify_objects {
                // todo: this can be written better
                for response in first {
                    let object_changes: Vec<ObjectID> = response
                        .object_changes
                        .clone()
                        .unwrap()
                        .iter()
                        .filter_map(get_object_id)
                        .collect();
                    check_objects(clients, object_changes.as_slice(), cross_validate).await;
                }
                for response in second {
                    let object_changes: Vec<ObjectID> = response
                        .object_changes
                        .clone()
                        .unwrap()
                        .iter()
                        .filter_map(get_object_id)
                        .collect();
                    check_objects(clients, object_changes.as_slice(), cross_validate).await;
                }
            }
        }
    }
}

fn get_object_id(object_change: &ObjectChange) -> Option<ObjectID> {
    match object_change {
        ObjectChange::Transferred { object_id, .. } => Some(*object_id),
        ObjectChange::Mutated { object_id, .. } => Some(*object_id),
        ObjectChange::Created { object_id, .. } => Some(*object_id),
        // TODO(gegaowp): needs separate checks for packages and modules publishing
        // TODO(gegaowp): ?? needs separate checks for deleted and wrapped objects
        _ => None,
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

async fn query_transactions(
    client: &SuiClient,
    query: SuiTransactionResponseQuery,
    cursor: Option<TransactionDigest>,
    limit: Option<usize>, // TODO: we should probably set a limit and paginate
) -> Result<Page<SuiTransactionResponse, TransactionDigest>> {
    let transactions = client
        .read_api()
        .query_transactions(query, cursor, limit, true)
        .await
        .unwrap();
    Ok(transactions)
}

// todo: this and check_transactions can be generic
pub async fn check_objects(
    clients: &Arc<RwLock<Vec<SuiClient>>>,
    object_ids: &[ObjectID],
    cross_validate: bool,
) {
    let read = clients.read().await;
    let clients = read.clone();

    let objects = join_all(clients.iter().enumerate().map(|(i, client)| async move {
        let start_time = Instant::now();
        let transactions = client
            .read_api()
            .multi_get_object_with_options(
                object_ids.to_vec(),
                SuiObjectDataOptions::full_content(), // todo(Will) support options for this
            )
            .await;
        let elapsed_time = start_time.elapsed();
        debug!(
            "MultiGetObject Request latency {:.4} for rpc at url {i}",
            elapsed_time.as_secs_f64()
        );
        transactions
    }))
    .await;

    // TODO: support more than 2 transactions
    if cross_validate && objects.len() == 2 {
        if let (Some(t1), Some(t2)) = (objects.get(0), objects.get(1)) {
            let first = match t1 {
                Ok(vec) => vec.as_slice(),
                Err(err) => {
                    error!(
                        "Error unwrapping first vec of objects: {:?} for objectIDs {:?}",
                        err, object_ids
                    );
                    return;
                }
            };
            let second = match t2 {
                Ok(vec) => vec.as_slice(),
                Err(err) => {
                    error!(
                        "Error unwrapping second vec of objects: {:?} for objectIDs {:?}",
                        err, object_ids
                    );
                    return;
                }
            };

            cross_validate_entities(object_ids, first, second, "ObjectID", "Object");
        }
    }
}

fn cross_validate_entities<T, U>(
    keys: &[T],
    first: &[U],
    second: &[U],
    key_name: &str,
    entity_name: &str,
) where
    T: std::fmt::Debug,
    U: PartialEq + std::fmt::Debug,
{
    if first.len() != second.len() {
        error!(
            "Entity: {} lengths do not match: {} vs {}",
            entity_name,
            first.len(),
            second.len()
        );
        return;
    }

    for (i, (a, b)) in first.iter().zip(second.iter()).enumerate() {
        if a != b {
            error!(
                "Entity: {} mismatch with index {}: {}: {:?}, first: {:?}, second: {:?}",
                entity_name, i, key_name, keys[i], a, b
            );
        }
    }
}

#[async_trait]
impl<'a> ProcessPayload<'a, &'a QueryTransactions> for RpcCommandProcessor {
    async fn process(
        &'a self,
        op: &'a QueryTransactions,
        _signer_info: &Option<SignerInfo>,
    ) -> Result<()> {
        let clients = self.get_clients().await?;
        let filter = match (op.from_address, op.to_address) {
            (Some(_), Some(_)) => {
                return Err(anyhow!("Cannot specify both from_address and to_address"));
            }
            (Some(address), None) => Some(TransactionFilter::FromAddress(address)),
            (None, Some(address)) => Some(TransactionFilter::ToAddress(address)),
            (None, None) => None,
        };
        let query = SuiTransactionResponseQuery {
            filter,
            options: None, // not supported on indexer
        };

        let results: Vec<TransactionsPage> = Vec::new();

        // Paginate results, if any
        while results.is_empty() || results.iter().any(|r| r.has_next_page) {
            let cursor = if results.is_empty() {
                None
            } else {
                match (
                    results.get(0).unwrap().next_cursor,
                    results.get(1).unwrap().next_cursor,
                ) {
                    (Some(first_cursor), Some(second_cursor)) => {
                        if first_cursor != second_cursor {
                            warn!("Cursors are not the same, received {} vs {}. Selecting the first cursor to continue", first_cursor, second_cursor);
                        }
                        Some(first_cursor)
                    }
                    (Some(cursor), None) | (None, Some(cursor)) => Some(cursor),
                    (None, None) => None,
                }
            };

            let results = join_all(clients.iter().enumerate().map(|(i, client)| {
                let with_query = query.clone();
                async move {
                    let start_time = Instant::now();
                    let transactions = query_transactions(client, with_query, cursor, None)
                        .await
                        .unwrap();
                    let elapsed_time = start_time.elapsed();
                    debug!(
                        "QueryTransactions Request latency {:.4} for rpc at url {i}",
                        elapsed_time.as_secs_f64()
                    );
                    transactions
                }
            }))
            .await;

            // compare results
            let digests = results[0]
                .data
                .iter()
                .map(|transaction| transaction.digest)
                .collect::<Vec<TransactionDigest>>();

            cross_validate_entities(
                &digests,
                &results[0].data,
                &results[1].data,
                "TransactionDigest",
                "Transaction",
            );
        }

        Ok(())
    }
}

#[async_trait]
impl<'a> ProcessPayload<'a, &'a PaySui> for RpcCommandProcessor {
    async fn process(&'a self, _op: &'a PaySui, signer_info: &Option<SignerInfo>) -> Result<()> {
        let clients = self.get_clients().await?;
        let SignerInfo {
            encoded_keypair,
            signer_address,
            gas_budget,
            gas_payment,
        } = signer_info.clone().unwrap();
        let recipient = SuiAddress::random_for_testing_only();
        let amount = 11;
        let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);

        let keypair =
            SuiKeyPair::decode_base64(&encoded_keypair).expect("Decoding keypair should not fail");

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
