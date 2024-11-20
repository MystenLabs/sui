// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use config::{DownloadFeedConfigs, UploadFeedConfig, UploadParameters};
use metrics::OracleMetrics;
use mysten_metrics::monitored_scope;
use once_cell::sync::OnceCell;
use prometheus::Registry;
use std::ops::Add;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use std::{collections::HashMap, time::Instant};
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockResponseOptions,
};
use sui_sdk::apis::ReadApi;
use sui_sdk::rpc_types::SuiObjectResponse;
use sui_sdk::SuiClient;
use sui_types::error::UserInputError;
use sui_types::object::{Object, Owner};
use sui_types::parse_sui_type_tag;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::quorum_driver_types::NON_RECOVERABLE_ERROR_MSG;
use sui_types::transaction::{Argument, Transaction};
use sui_types::transaction::{Command, ObjectArg};
use sui_types::Identifier;
use sui_types::{
    base_types::SuiAddress,
    transaction::{CallArg, TransactionData},
};
use tap::tap::TapFallible;

use sui_sdk::wallet_context::WalletContext;
use sui_types::base_types::{random_object_ref, ObjectID, ObjectRef};
use tracing::{debug, error, info, warn};
pub mod config;
mod metrics;

// TODO: allow more flexible decimals
const DECIMAL: u8 = 6;
const METRICS_MULTIPLIER: f64 = 10u64.pow(DECIMAL as u32) as f64;
const UPLOAD_FAILURE_RECOVER_SEC: u64 = 10;
static STALE_OBJ_ERROR: OnceCell<String> = OnceCell::new();

pub struct OracleNode {
    upload_feeds: HashMap<String, HashMap<String, UploadFeedConfig>>,
    gas_obj_id: ObjectID,
    download_feeds: DownloadFeedConfigs,
    wallet_ctx: WalletContext,
    metrics: Arc<OracleMetrics>,
}

impl OracleNode {
    pub fn new(
        upload_feeds: HashMap<String, HashMap<String, UploadFeedConfig>>,
        gas_obj_id: ObjectID,
        download_feeds: DownloadFeedConfigs,
        wallet_ctx: WalletContext,
        registry: Registry,
    ) -> Self {
        Self {
            upload_feeds,
            gas_obj_id,
            download_feeds,
            wallet_ctx,
            metrics: Arc::new(OracleMetrics::new(&registry)),
        }
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        info!("Starting OracleNode...");
        let signer_address = self.wallet_ctx.active_address()?;
        let client = Arc::new(self.wallet_ctx.get_client().await?);

        let wallet_ctx = Arc::new(self.wallet_ctx);
        DataProviderRunner::new(
            self.upload_feeds,
            self.gas_obj_id,
            wallet_ctx,
            client.clone(),
            signer_address,
            self.metrics.clone(),
        )
        .await
        .spawn();

        let (sender, mut receiver) = tokio::sync::mpsc::channel(1000);

        // Spawn a reader thread if reader_interval is configured.
        if let Some(read_interval) = self.download_feeds.read_interval {
            tokio::spawn(
                OnChainDataReader {
                    client: client.clone(),
                    read_interval,
                    read_configs: self.download_feeds.read_feeds,
                    metrics: self.metrics.clone(),
                }
                .start(sender.clone()),
            );
        }

        while let Some((read_feed, object_id, value)) = receiver.recv().await {
            info!(
                read_feed,
                ?object_id,
                ?value,
                "Received data from on chain reader."
            );
        }

        Ok(())
    }
}

struct DataProviderRunner {
    providers: Vec<Arc<DataProvider>>,
    uploader: OnChainDataUploader,
}

impl DataProviderRunner {
    pub async fn new(
        upload_feeds: HashMap<String, HashMap<String, UploadFeedConfig>>,
        gas_coin_id: ObjectID,
        wallet_ctx: Arc<WalletContext>,
        client: Arc<SuiClient>,
        signer_address: SuiAddress,
        metrics: Arc<OracleMetrics>,
    ) -> Self {
        let mut providers = vec![];
        let mut staleness_tolerance = HashMap::new();
        let mut oracle_object_args = HashMap::new();
        let (sender, receiver) = tokio::sync::mpsc::channel(10000);
        for (feed_name, upload_feed) in upload_feeds {
            for (source_name, data_feed) in upload_feed {
                staleness_tolerance.insert(
                    make_onchain_feed_name(&feed_name, &source_name),
                    data_feed.submission_interval,
                );
                let oracle_obj_id = data_feed.upload_parameters.write_data_provider_object_id;
                let data_provider = DataProvider {
                    feed_name: feed_name.clone(),
                    source_name: source_name.clone(),
                    upload_feed: Arc::new(data_feed),
                    sender: sender.clone(),
                    metrics: metrics.clone(),
                };
                providers.push(Arc::new(data_provider));
                if let std::collections::hash_map::Entry::Vacant(e) =
                    oracle_object_args.entry(oracle_obj_id)
                {
                    e.insert(
                        get_object_arg(client.read_api(), oracle_obj_id, true)
                            .await
                            .unwrap(),
                    );
                }
            }
        }
        info!("Staleness tolerance: {:?}", staleness_tolerance);

        let gas_obj_ref = get_gas_obj_ref(client.read_api(), gas_coin_id, signer_address).await;
        info!("Gas object: {:?}", gas_obj_ref);

        let uploader = OnChainDataUploader {
            wallet_ctx: wallet_ctx.clone(),
            client: client.clone(),
            receiver,
            signer_address,
            gas_obj_ref,
            staleness_tolerance,
            oracle_object_args,
            metrics: metrics.clone(),
        };
        Self {
            providers,
            uploader,
        }
    }

    pub fn spawn(mut self) {
        for data_provider in self.providers {
            tokio::spawn(async move {
                data_provider.run().await;
            });
        }
        tokio::spawn(async move {
            self.uploader.run().await;
        });
    }
}

async fn get_gas_obj_ref(
    read_api: &ReadApi,
    gas_obj_id: ObjectID,
    owner_address: SuiAddress,
) -> ObjectRef {
    loop {
        match read_api
            .get_object_with_options(gas_obj_id, SuiObjectDataOptions::default().with_owner())
            .await
            .map(|resp| resp.data)
        {
            Ok(Some(gas_obj)) => {
                assert_eq!(
                    gas_obj.owner,
                    Some(Owner::AddressOwner(owner_address)),
                    "Provided gas obj {:?} does not belong to {}",
                    gas_obj,
                    owner_address
                );
                return gas_obj.object_ref();
            }
            other => {
                warn!("Can't get gas object: {:?}: {:?}", gas_obj_id, other);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

struct DataProvider {
    pub feed_name: String,
    pub source_name: String,
    pub upload_feed: Arc<UploadFeedConfig>,
    pub sender: tokio::sync::mpsc::Sender<DataPoint>,
    metrics: Arc<OracleMetrics>,
}

impl DataProvider {
    pub async fn run(&self) {
        info!(
            feed_name = self.feed_name,
            source_name = self.source_name,
            "Starting DataProvider"
        );
        let mut interval = tokio::time::interval(self.upload_feed.submission_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            self.run_once().await;
        }
    }

    async fn run_once(&self) {
        debug!(
            feed_name = self.feed_name,
            source_name = self.source_name,
            "Running data provider once."
        );
        let value = self.retrieve_from_data_source().await;
        if value.is_err() {
            error!(
                feed_name = self.feed_name,
                source_name = self.source_name,
                "Failed to retrieve data from data source: {:?}",
                value
            );
            self.metrics
                .data_source_errors
                .with_label_values(&[&self.feed_name, &self.source_name])
                .inc();
            return;
        }

        self.metrics
            .data_source_successes
            .with_label_values(&[&self.feed_name, &self.source_name])
            .inc();

        // TODO: allow more flexible multiplers and data types
        let value = (value.unwrap() * METRICS_MULTIPLIER) as u64;
        self.send_to_uploader(value).await;
    }

    async fn retrieve_from_data_source(&self) -> anyhow::Result<f64> {
        // TODO: support websocket
        let url = &self.upload_feed.data_source_config.url;
        let json_path = &self.upload_feed.data_source_config.json_path;
        let response = reqwest::Client::new().get(url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to fetch data: {:?}", response);
        }

        let json_blob: serde_json::Value = response.json().await.unwrap();
        let data = jsonpath_lib::select(&json_blob, json_path)?;

        if data.is_empty() {
            anyhow::bail!(
                "Failed to find data from json blob: {:?} with json path: {:?}",
                json_blob,
                json_path
            );
        }
        // Assume there is one single value per request
        match data[0].as_str() {
            Some(value_str) => match value_str.parse::<f64>() {
                Ok(value) => Ok(value),
                Err(_) => anyhow::bail!(
                    "Failed to parse data {:?} as f64 from json blob: {:?}",
                    data[0],
                    json_blob
                ),
            },
            None => anyhow::bail!(
                "Failed to parse data {:?} as string from json blob: {:?}",
                data[0],
                json_blob
            ),
        }
    }

    async fn send_to_uploader(&self, value: u64) {
        let _ = self
            .sender
            .send(DataPoint {
                feed_name: make_onchain_feed_name(&self.feed_name, &self.source_name),
                upload_parameters: self.upload_feed.upload_parameters.clone(),
                value,
                retrieval_timestamp: SystemTime::now(),
                retrieval_instant: Instant::now(),
            })
            .await
            .tap_err(|err| error!("Failed to send data point to uploader: {:?}", err));
    }
}

fn make_onchain_feed_name(feed_name: &str, source_name: &str) -> String {
    format!(
        "{}-{}",
        feed_name.to_ascii_lowercase(),
        source_name.to_ascii_lowercase()
    )
}

struct OnChainDataUploader {
    wallet_ctx: Arc<WalletContext>,
    client: Arc<SuiClient>,
    receiver: tokio::sync::mpsc::Receiver<DataPoint>,
    signer_address: SuiAddress,
    gas_obj_ref: ObjectRef,
    staleness_tolerance: HashMap<String, Duration>,
    oracle_object_args: HashMap<ObjectID, ObjectArg>,
    metrics: Arc<OracleMetrics>,
}

impl OnChainDataUploader {
    async fn run(&mut self) {
        info!("Starting OnChainDataUploader");
        // The minimal latency is 1 second so we collect data every 1 second
        let mut read_interval = tokio::time::interval(Duration::from_millis(500));
        read_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            read_interval.tick().await;
            let data_points = self.collect().await;
            if !data_points.is_empty() {
                if let Err(err) = self.upload(data_points).await {
                    error!("Upload failure: {err}. About to resting for {UPLOAD_FAILURE_RECOVER_SEC} sec.");
                    tokio::time::sleep(Duration::from_secs(UPLOAD_FAILURE_RECOVER_SEC)).await;
                    self.gas_obj_ref = get_gas_obj_ref(
                        self.client.read_api(),
                        self.gas_obj_ref.0,
                        self.signer_address,
                    )
                    .await;
                    error!("Updated gas object reference: {:?}", self.gas_obj_ref);
                }
            }
        }
    }

    async fn collect(&mut self) -> Vec<DataPoint> {
        let start = Instant::now();
        let mut data_points = vec![];
        while let Ok(Some(data_point)) =
            tokio::time::timeout(Duration::from_millis(100), self.receiver.recv()).await
        {
            let feed_name = &data_point.feed_name;
            debug!(
                feed_name = data_point.feed_name,
                value = data_point.value,
                "Received data from data provider."
            );
            // TODO: for each source, at most take one value in each submission
            let staleness_tolerance =
                self.staleness_tolerance.get(feed_name).unwrap_or_else(|| {
                    panic!("Bug, missing staleness tolerance for feed: {}", feed_name)
                });
            let duration_since = data_point.retrieval_instant.elapsed();
            if duration_since > staleness_tolerance.add(Duration::from_secs(1)) {
                warn!(
                    feed_name,
                    value = data_point.value,
                    ?duration_since,
                    ?staleness_tolerance,
                    "Data is too stale, skipping."
                );
                self.metrics
                    .data_staleness
                    .with_label_values(&[feed_name])
                    .inc();
            } else {
                data_points.push(data_point);
            }

            // One run only waits for 1 second
            // But if we don't have any valid data points, we wait until we do.
            if data_points.is_empty() && start.elapsed() >= Duration::from_millis(500) {
                break;
            }
        }
        debug!("Collected {} data points", data_points.len());
        data_points
    }

    async fn upload(
        &mut self,
        data_points: Vec<DataPoint>,
    ) -> anyhow::Result<SuiTransactionBlockEffects> {
        let _scope = monitored_scope("Oracle::OnChainDataUploader::upload");
        // TODO add more error handling & polling perhaps
        let mut builder = ProgrammableTransactionBuilder::new();
        let mut is_first = true;
        for data_point in &data_points {
            let package_id = data_point.upload_parameters.write_package_id;
            let feed_name = &data_point.feed_name;
            let oracle_obj_arg = *self
                .oracle_object_args
                .get(&data_point.upload_parameters.write_data_provider_object_id)
                .unwrap_or_else(|| {
                    panic!("Bug, missing oracle object arg for feed: {}", feed_name)
                });
            let duration_since_start = data_point.retrieval_instant.elapsed();
            let data_point_ts: DateTime<Utc> =
                DateTime::from(data_point.retrieval_timestamp + duration_since_start);

            let mut arguments = if is_first {
                vec![
                    builder.input(CallArg::Object(oracle_obj_arg)).unwrap(),
                    builder.input(CallArg::CLOCK_IMM).unwrap(),
                ]
            } else {
                vec![Argument::Input(0), Argument::Input(1)]
            };

            let decimal = builder
                .input(CallArg::Pure(bcs::to_bytes(&DECIMAL).unwrap()))
                .unwrap();
            let value = builder
                .input(CallArg::Pure(bcs::to_bytes(&data_point.value).unwrap()))
                .unwrap();

            arguments.extend_from_slice(&[
                builder
                    .input(CallArg::Pure(bcs::to_bytes(&feed_name)?))
                    .unwrap(),
                builder.programmable_move_call(
                    package_id,
                    Identifier::from_str("decimal_value").unwrap(),
                    Identifier::from_str("new").unwrap(),
                    vec![],
                    vec![value, decimal],
                ),
                builder
                    .input(CallArg::Pure(bcs::to_bytes(&format!("{}", data_point_ts))?))
                    .unwrap(),
            ]);

            builder.command(Command::move_call(
                package_id,
                Identifier::new(data_point.upload_parameters.write_module_name.clone()).unwrap(),
                Identifier::new(data_point.upload_parameters.write_function_name.clone()).unwrap(),
                // TODO: allow more generic data types
                vec![
                    parse_sui_type_tag(&format!("{package_id}::decimal_value::DecimalValue"))
                        .unwrap(),
                ],
                arguments,
            ));
            is_first = false;
        }
        let pt = builder.finish();
        let rgp = self
            .client
            .governance_api()
            .get_reference_gas_price()
            .await?;
        let tx = TransactionData::new_programmable(
            self.signer_address,
            vec![self.gas_obj_ref],
            pt,
            // 15_000_000 is a heuristic number
            15_000_000 * data_points.len() as u64,
            rgp,
        );

        let signed_tx = self.wallet_ctx.sign_transaction(&tx);
        let tx_digest = *signed_tx.digest();

        let timer_start = Instant::now();
        let response = self.execute(signed_tx).await?;
        let time_spend_sec = timer_start.elapsed().as_secs_f32();

        // We asked for effects.
        // But is there a better way to handle this instead of panic?
        let effects = response.effects.expect("Expect to see effects in response");

        // It's critical to update the gas object reference for next transaction
        self.gas_obj_ref = effects.gas_object().reference.to_object_ref();

        let success = effects.status().is_ok();

        // Update metrics
        for data_point in &data_points {
            if success {
                self.metrics
                    .upload_successes
                    .with_label_values(&[&data_point.feed_name])
                    .inc();
                self.metrics
                    .uploaded_values
                    .with_label_values(&[&data_point.feed_name])
                    .observe(data_point.value as f64);
            } else {
                self.metrics
                    .upload_data_errors
                    .with_label_values(&[&data_point.feed_name])
                    .inc();
            }
        }

        let gas_usage = effects.gas_cost_summary().gas_used();
        let storage_rebate = effects.gas_cost_summary().storage_rebate;
        let computation_cost = effects.gas_cost_summary().computation_cost;
        let net_gas_usage = effects.gas_cost_summary().net_gas_usage();
        self.metrics
            .computation_gas_used
            .observe(computation_cost as f64);
        self.metrics.total_gas_cost.inc_by(gas_usage);
        self.metrics.total_gas_rebate.inc_by(storage_rebate);

        if success {
            self.metrics
                .total_data_points_uploaded
                .inc_by(data_points.len() as u64);
            info!(
                ?tx_digest,
                net_gas_usage,
                time_spend_sec,
                "Upload succeeded with {} data points",
                data_points.len(),
            );
            Ok(effects)
        } else {
            error!(
                ?tx_digest,
                net_gas_usage,
                "Upload failed with {} data points. Err: {:?}",
                data_points.len(),
                effects.status(),
            );
            anyhow::bail!("Failed to submit data on chain: {:?}", effects.status());
        }
    }

    async fn execute(&mut self, tx: Transaction) -> anyhow::Result<SuiTransactionBlockResponse> {
        let tx_digest = tx.digest();
        let mut retry_attempts = 3;
        loop {
            match self
                .client
                .quorum_driver_api()
                .execute_transaction_block(
                    tx.clone(),
                    SuiTransactionBlockResponseOptions::new().with_effects(),
                    // TODO: after 1.4.0, we can simply use `WaitForEffectsCert` which is faster.
                    // Some(sui_types::quorum_driver_types::ExecuteTransactionRequestType::WaitForEffectsCert),
                    Some(sui_types::quorum_driver_types::ExecuteTransactionRequestType::WaitForLocalExecution),
                )
                .await {
                Ok(response) => return Ok(response),
                Err(sui_sdk::error::Error::RpcError(err)) => {
                    // jsonrpsee translate every SuiError into jsonrpsee::core::Error, so we need to further distinguish 
                    if err.to_string().contains(NON_RECOVERABLE_ERROR_MSG) {
                        let stale_obj_error = STALE_OBJ_ERROR
                            .get_or_init(||
                                String::from(UserInputError::ObjectVersionUnavailableForConsumption { provided_obj_ref: random_object_ref(), current_version: 0.into() }.as_ref())
                            );
                        if err.to_string().contains(stale_obj_error) {
                            error!(?tx_digest, "Failed to submit tx, it looks like gas object is stale : {:?}", err);
                            let new_ref = get_gas_obj_ref(self.client.read_api(), self.gas_obj_ref.0, self.signer_address).await;
                            self.gas_obj_ref = new_ref;
                            info!("Gas object updated: {:?}", new_ref);
                            anyhow::bail!("Gas object is stale, now updated to {:?}. tx_digest={:?}", new_ref, tx_digest);
                        } else {
                            error!(?tx_digest, "Failed to submit tx, with non recoverable error: {:?}", err);
                            anyhow::bail!("Non-retryable error {:?}. tx_digest={:?}", err, tx_digest);
                        }
                    }
                    // Likely retryable error?
                    error!(?tx_digest, "Failed to submit tx, with (likely) recoverable error: {:?}. Remaining retry times: {}", err, retry_attempts);
                    retry_attempts -= 1;
                    if retry_attempts <= 0 {
                        anyhow::bail!("Too many RPC errors: {}. tx_digest={:?}", err, tx_digest);
                    }
                }
                // All other errors are unexpected
                Err(err) => {
                    error!(?tx_digest, "Failed to submit tx, with unexpected error: {:?}", err);
                    anyhow::bail!("Unexpected error in tx submission {:?}. tx_digest={:?}", err, tx_digest);
                }
            }
        }
    }
}

#[derive(Debug)]
struct DataPoint {
    feed_name: String,
    upload_parameters: UploadParameters,
    value: u64,
    retrieval_timestamp: SystemTime,
    retrieval_instant: Instant,
}

struct OnChainDataReader {
    pub client: Arc<SuiClient>,
    // For now we share one read interval for all reads
    pub read_interval: Duration,
    pub read_configs: HashMap<String, ObjectID>,
    metrics: Arc<OracleMetrics>,
}

impl OnChainDataReader {
    pub async fn start(self, sender: tokio::sync::mpsc::Sender<(String, ObjectID, f64)>) {
        info!(
            "Starting on-chain data reader with interval {:?} and config: {:?}",
            self.read_interval, self.read_configs
        );
        let mut read_interval = tokio::time::interval(self.read_interval);
        read_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            read_interval.tick().await;
            for (feed_name, object_id) in &self.read_configs {
                match self
                    .client
                    .read_api()
                    .get_object_with_options(*object_id, SuiObjectDataOptions::default())
                    .await
                {
                    Ok(SuiObjectResponse {
                        data: Some(_data), ..
                    }) => {
                        // TODO parse value based on returned BCS
                        let value = 5_f64;
                        let _ = sender.send((feed_name.clone(), *object_id, value)).await;
                        self.metrics
                            .downloaded_values
                            .with_label_values(&[feed_name])
                            .observe(value * METRICS_MULTIPLIER);
                        self.metrics
                            .download_successes
                            .with_label_values(&[feed_name, &object_id.to_string()])
                            .inc();
                    }
                    other => {
                        error!(
                            read_feed = feed_name,
                            ?object_id,
                            "Failed to read data from on-chain: {:?}",
                            other
                        );
                        self.metrics
                            .download_data_errors
                            .with_label_values(&[feed_name, &object_id.to_string()])
                            .inc();
                    }
                }
            }
        }
    }
}

async fn get_object_arg(
    read_api: &ReadApi,
    id: ObjectID,
    is_mutable_ref: bool,
) -> anyhow::Result<ObjectArg> {
    let response = read_api
        .get_object_with_options(id, SuiObjectDataOptions::bcs_lossless())
        .await?;

    let obj: Object = response.into_object()?.try_into()?;
    let obj_ref = obj.compute_object_reference();
    let owner = obj.owner.clone();
    Ok(match owner {
        Owner::Shared {
            initial_shared_version,
        }
        | Owner::ConsensusV2 {
            start_version: initial_shared_version,
            authenticator: _,
        } => ObjectArg::SharedObject {
            id,
            initial_shared_version,
            mutable: is_mutable_ref,
        },
        Owner::AddressOwner(_) | Owner::ObjectOwner(_) | Owner::Immutable => {
            ObjectArg::ImmOrOwnedObject(obj_ref)
        }
    })
}
