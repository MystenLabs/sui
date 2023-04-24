// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use dashmap::{DashMap, DashSet};
use futures::future::join_all;
use serde::de::DeserializeOwned;
use serde::Serialize;
use shared_crypto::intent::{Intent, IntentMessage};
use std::fmt;
use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiObjectDataOptions, SuiTransactionBlockDataAPI,
    SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_types::digests::TransactionDigest;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, info};

use crate::load_test::LoadTestConfig;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::{get_key_pair, AccountKeyPair, EncodeDecodeBase64, Signature, SuiKeyPair};
use sui_types::messages::{ExecuteTransactionRequestType, Transaction, TransactionData};

use crate::payload::checkpoint_utils::get_latest_checkpoint_stats;
use crate::payload::validation::chunk_entities;
use crate::payload::{
    Command, CommandData, DryRun, GetAllBalances, GetCheckpoints, GetObject, MultiGetObjects,
    Payload, ProcessPayload, Processor, QueryTransactionBlocks, SignerInfo,
};

use super::MultiGetTransactionBlocks;

pub(crate) const DEFAULT_GAS_BUDGET: u64 = 500_000_000;
pub(crate) const DEFAULT_LARGE_GAS_BUDGET: u64 = 50_000_000_000;
pub(crate) const MAX_NUM_NEW_OBJECTS_IN_SINGLE_TRANSACTION: usize = 120;

#[derive(Clone)]
pub struct RpcCommandProcessor {
    clients: Arc<RwLock<Vec<SuiClient>>>,
    // for equivocation prevention in `WaitForEffectsCert` mode
    object_ref_cache: Arc<DashMap<ObjectID, ObjectRef>>,
    transaction_digests: Arc<DashSet<TransactionDigest>>,
    addresses: Arc<DashSet<SuiAddress>>,
    data_dir: String,
}

impl RpcCommandProcessor {
    pub async fn new(urls: &[String], data_dir: String) -> Self {
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
            object_ref_cache: Arc::new(DashMap::new()),
            transaction_digests: Arc::new(DashSet::new()),
            addresses: Arc::new(DashSet::new()),
            data_dir,
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
            CommandData::QueryTransactionBlocks(ref v) => self.process(v, signer_info).await,
            CommandData::MultiGetTransactionBlocks(ref v) => self.process(v, signer_info).await,
            CommandData::MultiGetObjects(ref v) => self.process(v, signer_info).await,
            CommandData::GetObject(ref v) => self.process(v, signer_info).await,
            CommandData::GetAllBalances(ref v) => self.process(v, signer_info).await,
            CommandData::GetReferenceGasPrice(ref v) => self.process(v, signer_info).await,
        }
    }

    pub(crate) async fn get_clients(&self) -> Result<Vec<SuiClient>> {
        let read = self.clients.read().await;
        Ok(read.clone())
    }

    /// sign_and_execute transaction and update `object_ref_cache`
    pub(crate) async fn sign_and_execute(
        &self,
        client: &SuiClient,
        keypair: &SuiKeyPair,
        txn_data: TransactionData,
        request_type: ExecuteTransactionRequestType,
    ) -> SuiTransactionBlockResponse {
        let resp = sign_and_execute(client, keypair, txn_data, request_type).await;
        let effects = resp.effects.as_ref().unwrap();
        let object_ref_cache = self.object_ref_cache.clone();
        // NOTE: for now we don't need to care about deleted objects
        for (owned_object_ref, _) in effects.all_changed_objects() {
            let id = owned_object_ref.object_id();
            let current = object_ref_cache.get_mut(&id);
            match current {
                Some(mut c) => {
                    if c.1 < owned_object_ref.version() {
                        *c = owned_object_ref.reference.to_object_ref();
                    }
                }
                None => {
                    object_ref_cache.insert(id, owned_object_ref.reference.to_object_ref());
                }
            };
        }
        resp
    }

    /// get the latest object ref from local cache, and if not exist, fetch from fullnode
    pub(crate) async fn get_object_ref(
        &self,
        client: &SuiClient,
        object_id: &ObjectID,
    ) -> ObjectRef {
        let object_ref_cache = self.object_ref_cache.clone();
        let current = object_ref_cache.get_mut(object_id);
        match current {
            Some(c) => *c,
            None => {
                let resp = client
                    .read_api()
                    .get_object_with_options(*object_id, SuiObjectDataOptions::new())
                    .await
                    .unwrap_or_else(|_| panic!("Unable to fetch object reference {object_id}"));
                let object_ref = resp.object_ref_if_exists().unwrap_or_else(|| {
                    panic!("Unable to extract object reference {object_id} from response {resp:?}")
                });
                object_ref_cache.insert(*object_id, object_ref);
                object_ref
            }
        }
    }

    pub(crate) fn add_transaction_digests(&self, digests: Vec<TransactionDigest>) {
        // extend method requires mutable access to the underlying DashSet, which is not allowed by the Arc
        for digest in digests {
            self.transaction_digests.insert(digest);
        }
    }

    pub(crate) fn add_addresses_from_response(&self, responses: &[SuiTransactionBlockResponse]) {
        for response in responses {
            let transaction = &response.transaction;
            if let Some(transaction) = transaction {
                let data = &transaction.data;
                self.addresses.insert(*data.sender());
            }
        }
    }

    pub(crate) fn add_object_ids_from_response(&self, responses: &[SuiTransactionBlockResponse]) {
        for response in responses {
            let effects = &response.effects;
            if let Some(effects) = effects {
                let all_changed_objects = effects.all_changed_objects();
                for (object_ref, _) in all_changed_objects {
                    self.object_ref_cache
                        .insert(object_ref.object_id(), object_ref.reference.to_object_ref());
                }
            }
        }
    }

    pub(crate) fn dump_cache_to_file(&self) {
        // TODO: be more granular
        let digests: Vec<TransactionDigest> = self.transaction_digests.iter().map(|x| *x).collect();
        if !digests.is_empty() {
            debug!("dumping transaction digests to file {:?}", digests.len());
            write_data_to_file(
                &digests,
                &format!("{}/{}", &self.data_dir, CacheType::TransactionDigest),
            )
            .unwrap();
        }

        let addresses: Vec<SuiAddress> = self.addresses.iter().map(|x| *x).collect();
        if !addresses.is_empty() {
            debug!("dumping addresses to file {:?}", addresses.len());
            write_data_to_file(
                &addresses,
                &format!("{}/{}", &self.data_dir, CacheType::SuiAddress),
            )
            .unwrap();
        }

        let mut object_ids: Vec<ObjectID> = Vec::new();
        let cloned_object_cache = self.object_ref_cache.clone();

        for item in cloned_object_cache.iter() {
            let object_id = item.key();
            object_ids.push(*object_id);
        }

        if !object_ids.is_empty() {
            debug!("dumping object_ids to file {:?}", object_ids.len());
            write_data_to_file(
                &object_ids,
                &format!("{}/{}", &self.data_dir, CacheType::ObjectID),
            )
            .unwrap();
        }
    }
}

#[async_trait]
impl Processor for RpcCommandProcessor {
    async fn apply(&self, payload: &Payload) -> Result<()> {
        let commands = &payload.commands;
        for command in commands.iter() {
            let repeat_interval = command.repeat_interval;
            let repeat_n_times = command.repeat_n_times;
            for i in 0..=repeat_n_times {
                let start_time = Instant::now();

                self.process_command_data(&command.data, &payload.signer_info)
                    .await?;

                let elapsed_time = start_time.elapsed();
                if elapsed_time < repeat_interval {
                    let sleep_duration = repeat_interval - elapsed_time;
                    sleep(sleep_duration).await;
                }
                let clients = self.get_clients().await?;
                let checkpoint_stats = get_latest_checkpoint_stats(&clients, None).await;
                info!("Repeat {i}: Checkpoint stats {checkpoint_stats}, elapse {:.4} since last repeat", elapsed_time.as_secs_f64());
            }
        }
        Ok(())
    }

    async fn prepare(&self, config: &LoadTestConfig) -> Result<Vec<Payload>> {
        let clients = self.get_clients().await?;
        let Command {
            repeat_n_times,
            repeat_interval,
            ..
        } = &config.command;
        let command_payloads = match &config.command.data {
            CommandData::GetCheckpoints(data) => {
                if !config.divide_tasks {
                    vec![config.command.clone(); config.num_threads]
                } else {
                    divide_checkpoint_tasks(&clients, data, config.num_threads).await
                }
            }
            CommandData::QueryTransactionBlocks(data) => {
                if !config.divide_tasks {
                    vec![config.command.clone(); config.num_threads]
                } else {
                    divide_query_transaction_blocks_tasks(data, config.num_threads).await
                }
            }
            CommandData::MultiGetTransactionBlocks(data) => {
                if !config.divide_tasks {
                    vec![config.command.clone(); config.num_threads]
                } else {
                    divide_multi_get_transaction_blocks_tasks(data, config.num_threads).await
                }
            }
            CommandData::GetAllBalances(data) => {
                if !config.divide_tasks {
                    vec![config.command.clone(); config.num_threads]
                } else {
                    divide_get_all_balances_tasks(data, config.num_threads).await
                }
            }
            CommandData::MultiGetObjects(data) => {
                if !config.divide_tasks {
                    vec![config.command.clone(); config.num_threads]
                } else {
                    divide_multi_get_objects_tasks(data, config.num_threads).await
                }
            }
            CommandData::GetObject(data) => {
                if !config.divide_tasks {
                    vec![config.command.clone(); config.num_threads]
                } else {
                    divide_get_object_tasks(data, config.num_threads).await
                }
            }
            _ => vec![config.command.clone(); config.num_threads],
        };

        let command_payloads = command_payloads.into_iter().map(|command| {
            command
                .with_repeat_interval(*repeat_interval)
                .with_repeat_n_times(*repeat_n_times)
        });

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

    fn dump_cache_to_file(&self, config: &LoadTestConfig) {
        if let CommandData::GetCheckpoints(data) = &config.command.data {
            if data.record {
                self.dump_cache_to_file();
            }
        }
    }
}

#[async_trait]
impl<'a> ProcessPayload<'a, &'a DryRun> for RpcCommandProcessor {
    async fn process(&'a self, _op: &'a DryRun, _signer_info: &Option<SignerInfo>) -> Result<()> {
        debug!("DryRun");
        Ok(())
    }
}

fn write_data_to_file<T: Serialize>(data: &T, file_path: &str) -> Result<(), anyhow::Error> {
    let mut path_buf = PathBuf::from(&file_path);
    path_buf.pop();
    fs::create_dir_all(&path_buf).map_err(|e| anyhow!("Error creating directory: {}", e))?;

    let file_name = format!("{}.json", file_path);
    let file = File::create(file_name).map_err(|e| anyhow!("Error creating file: {}", e))?;
    serde_json::to_writer(file, data).map_err(|e| anyhow!("Error writing to file: {}", e))?;

    Ok(())
}

pub enum CacheType {
    SuiAddress,
    TransactionDigest,
    ObjectID,
}

impl fmt::Display for CacheType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheType::SuiAddress => write!(f, "SuiAddress"),
            CacheType::TransactionDigest => write!(f, "TransactionDigest"),
            CacheType::ObjectID => write!(f, "ObjectID"),
        }
    }
}

// TODO(Will): Consider using enums for input and output? Would mean we need to do checks any time we use generic load_cache_from_file
pub fn load_addresses_from_file(filepath: String) -> Vec<SuiAddress> {
    let path = format!("{}/{}", filepath, CacheType::SuiAddress);
    let addresses: Vec<SuiAddress> = read_data_from_file(&path).expect("Failed to read addresses");
    addresses
}

pub fn load_objects_from_file(filepath: String) -> Vec<ObjectID> {
    let path = format!("{}/{}", filepath, CacheType::ObjectID);
    let objects: Vec<ObjectID> = read_data_from_file(&path).expect("Failed to read objects");
    objects
}

pub fn load_digests_from_file(filepath: String) -> Vec<TransactionDigest> {
    let path = format!("{}/{}", filepath, CacheType::TransactionDigest);
    let digests: Vec<TransactionDigest> =
        read_data_from_file(&path).expect("Failed to read transaction digests");
    digests
}

fn read_data_from_file<T: DeserializeOwned>(file_path: &str) -> Result<T, anyhow::Error> {
    let mut path_buf = PathBuf::from(file_path);

    // Check if the file has a JSON extension
    if path_buf.extension().map_or(true, |ext| ext != "json") {
        // If not, add .json to the filename
        path_buf.set_extension("json");
    }

    let path = path_buf.as_path();
    if !path.exists() {
        return Err(anyhow!("File not found: {}", file_path));
    }

    let file = File::open(path).map_err(|e| anyhow::anyhow!("Error opening file: {}", e))?;
    let deserialized_data: T =
        serde_json::from_reader(file).map_err(|e| anyhow!("Deserialization error: {}", e))?;

    Ok(deserialized_data)
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
                data.record,
            )
        })
        .collect()
}

async fn divide_query_transaction_blocks_tasks(
    data: &QueryTransactionBlocks,
    num_chunks: usize,
) -> Vec<Command> {
    let chunk_size = if data.addresses.len() < num_chunks {
        1
    } else {
        data.addresses.len() as u64 / num_chunks as u64
    };
    let chunked = chunk_entities(data.addresses.as_slice(), Some(chunk_size as usize));
    chunked
        .into_iter()
        .map(|chunk| Command::new_query_transaction_blocks(data.address_type.clone(), chunk))
        .collect()
}

async fn divide_multi_get_transaction_blocks_tasks(
    data: &MultiGetTransactionBlocks,
    num_chunks: usize,
) -> Vec<Command> {
    let chunk_size = if data.digests.len() < num_chunks {
        1
    } else {
        data.digests.len() as u64 / num_chunks as u64
    };
    let chunked = chunk_entities(data.digests.as_slice(), Some(chunk_size as usize));
    chunked
        .into_iter()
        .map(Command::new_multi_get_transaction_blocks)
        .collect()
}

async fn divide_get_all_balances_tasks(data: &GetAllBalances, num_threads: usize) -> Vec<Command> {
    let per_thread_size = if data.addresses.len() < num_threads {
        1
    } else {
        data.addresses.len() / num_threads
    };

    let chunked = chunk_entities(data.addresses.as_slice(), Some(per_thread_size));
    chunked
        .into_iter()
        .map(|chunk| Command::new_get_all_balances(chunk, data.chunk_size))
        .collect()
}

// TODO: probs can do generic divide tasks
async fn divide_multi_get_objects_tasks(data: &MultiGetObjects, num_chunks: usize) -> Vec<Command> {
    let chunk_size = if data.object_ids.len() < num_chunks {
        1
    } else {
        data.object_ids.len() as u64 / num_chunks as u64
    };
    let chunked = chunk_entities(data.object_ids.as_slice(), Some(chunk_size as usize));
    chunked
        .into_iter()
        .map(Command::new_multi_get_objects)
        .collect()
}

async fn divide_get_object_tasks(data: &GetObject, num_threads: usize) -> Vec<Command> {
    let per_thread_size = if data.object_ids.len() < num_threads {
        1
    } else {
        data.object_ids.len() / num_threads
    };

    let chunked = chunk_entities(data.object_ids.as_slice(), Some(per_thread_size));
    chunked
        .into_iter()
        .map(|chunk| Command::new_get_object(chunk, data.chunk_size))
        .collect()
}

async fn prepare_new_signer_and_coins(
    client: &SuiClient,
    signer_info: &SignerInfo,
    num_coins: usize,
    num_transactions_per_coin: u64,
) -> (Vec<ObjectID>, String) {
    // TODO(chris): consider reference gas price
    let amount_per_coin = num_transactions_per_coin * DEFAULT_GAS_BUDGET;
    let pay_amount = amount_per_coin * num_coins as u64;
    let num_split_txns =
        num_transactions_needed(num_coins, MAX_NUM_NEW_OBJECTS_IN_SINGLE_TRANSACTION);
    let (gas_fee_for_split, gas_fee_for_pay_sui) = (
        DEFAULT_LARGE_GAS_BUDGET * num_split_txns as u64,
        DEFAULT_GAS_BUDGET,
    );

    let primary_keypair = SuiKeyPair::decode_base64(&signer_info.encoded_keypair)
        .expect("Decoding keypair should not fail");
    let sender = SuiAddress::from(&primary_keypair.public());
    let (coin, balance) = get_coin_with_max_balance(client, sender).await;
    // The balance needs to cover `pay_amount` plus
    // 1. gas fee for pay_sui from the primary address to the burner address
    // 2. gas fee for splitting the primary coin into `num_coins`
    let required_balance = pay_amount + gas_fee_for_split + gas_fee_for_pay_sui;
    if required_balance > balance {
        panic!("Current balance {balance} is smaller than require amount of MIST to fund the operation {required_balance}");
    }

    // There is a limit for the number of new objects in a transactions, therefore we need
    // multiple split transactions if the `num_coins` is large
    let split_amounts = calculate_split_amounts(
        num_coins,
        amount_per_coin,
        MAX_NUM_NEW_OBJECTS_IN_SINGLE_TRANSACTION,
    );

    debug!("split_amounts {split_amounts:?}");

    // We don't want to split coins in our primary address because we want to avoid having
    // a million coin objects in our address. We can also fetch directly from the faucet, but in
    // some environment that might not be possible when faucet resource is scarce
    let (burner_address, burner_keypair): (_, AccountKeyPair) = get_key_pair();
    let burner_keypair = SuiKeyPair::Ed25519(burner_keypair);
    let pay_amounts = split_amounts
        .iter()
        .map(|(amount, _)| *amount)
        .chain(std::iter::once(gas_fee_for_split))
        .collect::<Vec<_>>();

    debug!("pay_amounts {pay_amounts:?}");

    pay_sui(
        client,
        &primary_keypair,
        vec![coin],
        DEFAULT_GAS_BUDGET,
        vec![burner_address; pay_amounts.len()],
        pay_amounts,
    )
    .await;

    let coins = get_sui_coin_ids(client, burner_address).await;
    let gas_coin_id = get_coin_with_balance(&coins, gas_fee_for_split);
    let primary_coin = get_coin_with_balance(&coins, split_amounts[0].0);
    assert!(!coins.is_empty());
    let mut results: Vec<ObjectID> = vec![];
    assert!(!split_amounts.is_empty());
    if split_amounts.len() == 1 && split_amounts[0].1 == 0 {
        results.push(get_coin_with_balance(&coins, split_amounts[0].0));
    } else if split_amounts.len() == 1 {
        results.extend(
            split_coins(
                client,
                &burner_keypair,
                primary_coin,
                gas_coin_id,
                split_amounts[0].1 as u64,
            )
            .await,
        );
    } else {
        let (max_amount, max_split) = &split_amounts[0];
        let (remainder_amount, remainder_split) = split_amounts.last().unwrap();
        let primary_coins = coins
            .iter()
            .filter(|(_, balance)| balance == max_amount)
            .map(|(id, _)| (*id, *max_split as u64))
            .chain(
                coins
                    .iter()
                    .filter(|(_, balance)| balance == remainder_amount)
                    .map(|(id, _)| (*id, *remainder_split as u64)),
            )
            .collect::<Vec<_>>();

        for (coin_id, splits) in primary_coins {
            results
                .extend(split_coins(client, &burner_keypair, coin_id, gas_coin_id, splits).await);
        }
    }
    assert_eq!(results.len(), num_coins);
    debug!("Split off {} coins for gas payment {results:?}", num_coins);
    (results, burner_keypair.encode_base64())
}

/// Calculate the number of transactions needed to split the given number of coins.
/// new_coins_per_txn must be greater than 0
fn num_transactions_needed(num_coins: usize, new_coins_per_txn: usize) -> usize {
    assert!(new_coins_per_txn > 0);
    if num_coins == 1 {
        return 0;
    }
    (num_coins + new_coins_per_txn - 1) / new_coins_per_txn
}

/// Calculate the split amounts for a given number of coins, amount per coin, and maximum number of coins per transaction.
/// Returns a Vec of (primary_coin_amount, split_into_n_coins)
fn calculate_split_amounts(
    num_coins: usize,
    amount_per_coin: u64,
    max_coins_per_txn: usize,
) -> Vec<(u64, usize)> {
    let total_amount = amount_per_coin * num_coins as u64;
    let num_transactions = num_transactions_needed(num_coins, max_coins_per_txn);

    if num_transactions == 0 {
        return vec![(total_amount, 0)];
    }

    let amount_per_transaction = max_coins_per_txn as u64 * amount_per_coin;
    let remaining_amount = total_amount - amount_per_transaction * (num_transactions as u64 - 1);
    let mut split_amounts: Vec<(u64, usize)> =
        vec![(amount_per_transaction, max_coins_per_txn); num_transactions - 1];
    split_amounts.push((
        remaining_amount,
        num_coins - max_coins_per_txn * (num_transactions - 1),
    ));
    split_amounts
}

async fn get_coin_with_max_balance(client: &SuiClient, address: SuiAddress) -> (ObjectID, u64) {
    let coins = get_sui_coin_ids(client, address).await;
    assert!(!coins.is_empty());
    coins.into_iter().max_by(|a, b| a.1.cmp(&b.1)).unwrap()
}

fn get_coin_with_balance(coins: &[(ObjectID, u64)], target: u64) -> ObjectID {
    coins.iter().find(|(_, b)| b == &target).unwrap().0
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
) -> SuiTransactionBlockResponse {
    let sender = SuiAddress::from(&keypair.public());
    let tx = client
        .transaction_builder()
        .pay(sender, input_coins, recipients, amounts, None, gas_budget)
        .await
        .expect("Failed to construct pay sui transaction");
    sign_and_execute(
        client,
        keypair,
        tx,
        ExecuteTransactionRequestType::WaitForLocalExecution,
    )
    .await
}

async fn split_coins(
    client: &SuiClient,
    keypair: &SuiKeyPair,
    coin_to_split: ObjectID,
    gas_payment: ObjectID,
    num_coins: u64,
) -> Vec<ObjectID> {
    let sender = SuiAddress::from(&keypair.public());
    let split_coin_tx = client
        .transaction_builder()
        .split_coin_equal(
            sender,
            coin_to_split,
            num_coins,
            Some(gas_payment),
            DEFAULT_LARGE_GAS_BUDGET,
        )
        .await
        .expect("Failed to construct split coin transaction");
    sign_and_execute(
        client,
        keypair,
        split_coin_tx,
        ExecuteTransactionRequestType::WaitForLocalExecution,
    )
    .await
    .effects
    .unwrap()
    .created()
    .iter()
    .map(|owned_object_ref| owned_object_ref.reference.object_id)
    .chain(std::iter::once(coin_to_split))
    .collect::<Vec<_>>()
}

pub(crate) async fn sign_and_execute(
    client: &SuiClient,
    keypair: &SuiKeyPair,
    txn_data: TransactionData,
    request_type: ExecuteTransactionRequestType,
) -> SuiTransactionBlockResponse {
    let signature = Signature::new_secure(
        &IntentMessage::new(Intent::sui_transaction(), &txn_data),
        keypair,
    );

    let transaction_response = match client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(txn_data, Intent::sui_transaction(), vec![signature])
                .verify()
                .expect("signature error"),
            SuiTransactionBlockResponseOptions::new().with_effects(),
            Some(request_type),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{assert_eq, vec};

    #[test]
    fn test_calculate_split_amounts_no_split_needed() {
        let num_coins = 10;
        let amount_per_coin = 100;
        let max_coins_per_txn = 20;
        let expected = vec![(1000, 10)];
        let result = calculate_split_amounts(num_coins, amount_per_coin, max_coins_per_txn);

        assert_eq!(expected, result);
    }

    #[test]
    fn test_calculate_split_amounts_exact_split() {
        let num_coins = 10;
        let amount_per_coin = 100;
        let max_coins_per_txn = 5;
        let expected = vec![(500, 5), (500, 5)];
        let result = calculate_split_amounts(num_coins, amount_per_coin, max_coins_per_txn);

        assert_eq!(expected, result);
    }

    #[test]
    fn test_calculate_split_amounts_with_remainder() {
        let num_coins = 12;
        let amount_per_coin = 100;
        let max_coins_per_txn = 5;
        let expected = vec![(500, 5), (500, 5), (200, 2)];
        let result = calculate_split_amounts(num_coins, amount_per_coin, max_coins_per_txn);

        assert_eq!(expected, result);
    }

    #[test]
    fn test_calculate_split_amounts_single_coin() {
        let num_coins = 1;
        let amount_per_coin = 100;
        let max_coins_per_txn = 5;
        let expected = vec![(100, 0)];
        let result = calculate_split_amounts(num_coins, amount_per_coin, max_coins_per_txn);

        assert_eq!(expected, result);
    }

    #[test]
    fn test_calculate_split_amounts_max_coins_equals_num_coins() {
        let num_coins = 5;
        let amount_per_coin = 100;
        let max_coins_per_txn = 5;
        let expected = vec![(500, 5)];
        let result = calculate_split_amounts(num_coins, amount_per_coin, max_coins_per_txn);

        assert_eq!(expected, result);
    }

    #[test]
    #[should_panic(expected = "assertion failed: new_coins_per_txn > 0")]
    fn test_calculate_split_amounts_zero_max_coins() {
        let num_coins = 5;
        let amount_per_coin = 100;
        let max_coins_per_txn = 0;

        calculate_split_amounts(num_coins, amount_per_coin, max_coins_per_txn);
    }
}
