// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::{DownloadFeedConfigs, UploadFeedConfig};
use metrics::OracleMetrics;
use move_core_types::language_storage::TypeTag;
use move_core_types::ident_str;
use mysten_metrics::monitored_scope;
use sui_json_rpc_types::SuiTypeTag;
use sui_json::SuiJsonValue;
use serde_json::json;
use prometheus::Registry;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockResponseOptions,
};
use sui_sdk::rpc_types::SuiObjectResponse;
use sui_sdk::SuiClient;
use sui_types::{base_types::SuiAddress, transaction::{TransactionData, CallArg}};

use sui_sdk::wallet_context::WalletContext;
use sui_types::base_types::ObjectID;
use tracing::{error, info};
pub mod config;
mod metrics;

const METRICS_MULTIPLIER: f64 = 1_000_000.0;

pub struct OracleNode {
    upload_feeds: HashMap<String, HashMap<String, UploadFeedConfig>>,
    download_feeds: DownloadFeedConfigs,
    wallet_ctx: WalletContext,
    metrics: Arc<OracleMetrics>,
}

impl OracleNode {
    pub fn new(
        upload_feeds: HashMap<String, HashMap<String, UploadFeedConfig>>,
        download_feeds: DownloadFeedConfigs,
        wallet_ctx: WalletContext,
        registry: Registry,
    ) -> Self {
        Self {
            upload_feeds,
            download_feeds,
            wallet_ctx,
            metrics: Arc::new(OracleMetrics::new(&registry)),
        }
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        info!("Starting sui-oracle...");
        let signer_address = self.wallet_ctx.active_address()?;
        let client = Arc::new(self.wallet_ctx.get_client().await?);

        // TODO sanity check, such as objects are good, etc
        let wallet_ctx = Arc::new(self.wallet_ctx);
        DataProviderRunner::new(
            self.upload_feeds,
            wallet_ctx,
            client.clone(),
            signer_address,
            self.metrics.clone(),
        )
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
    pub data_providers: Vec<Arc<DataProvider>>,
}

impl DataProviderRunner {
    pub fn new(
        upload_feeds: HashMap<String, HashMap<String, UploadFeedConfig>>,
        wallet_ctx: Arc<WalletContext>,
        client: Arc<SuiClient>,
        signer_address: SuiAddress,
        metrics: Arc<OracleMetrics>,
    ) -> Self {
        let mut data_providers = vec![];
        for (feed_name, upload_feed) in upload_feeds {
            for (source_name, data_feed) in upload_feed {
                let data_provider = DataProvider {
                    feed_name: feed_name.clone(),
                    source_name: source_name.clone(),
                    upload_feed: Arc::new(data_feed),
                    wallet_ctx: wallet_ctx.clone(),
                    client: client.clone(),
                    signer_address,
                    metrics: metrics.clone(),
                };
                data_providers.push(Arc::new(data_provider));
            }
        }
        Self { data_providers }
    }

    pub fn spawn(self) {
        for data_provider in self.data_providers {
            tokio::spawn(async move {
                data_provider.run().await;
            });
        }
    }
}

struct DataProvider {
    pub feed_name: String,
    pub source_name: String,
    pub upload_feed: Arc<UploadFeedConfig>,
    pub wallet_ctx: Arc<WalletContext>,
    pub client: Arc<SuiClient>,
    pub signer_address: SuiAddress,
    metrics: Arc<OracleMetrics>,
}

impl DataProvider {
    pub async fn run(&self) {
        info!(
            feed_name = self.feed_name,
            source_name = self.source_name,
            "Starting data provider."
        );
        let mut interval = tokio::time::interval(self.upload_feed.submission_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            self.run_once().await;
        }
    }

    async fn run_once(&self) {
        info!(
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

        let value = (value.unwrap() * METRICS_MULTIPLIER) as u64;
        match self.submit(value).await {
            Ok(effects) => {
                info!(
                    feed_name = self.feed_name,
                    source_name = self.source_name,
                    "Submitted value: {value}."
                );
                self.metrics
                    .uploaded_values
                    .with_label_values(&[&self.feed_name])
                    .observe((value) as u64);
                self.metrics
                    .upload_successes
                    .with_label_values(&[&self.feed_name, &self.source_name])
                    .inc();
                let gas_usage = effects.gas_cost_summary().gas_used();
                self.metrics
                    .gas_used
                    .with_label_values(&[&self.feed_name, &self.source_name])
                    .observe(gas_usage);
                self.metrics
                    .total_gas_used
                    .with_label_values(&[&self.feed_name, &self.source_name])
                    .inc_by(gas_usage);
            }
            Err(_) => {
                error!(
                    feed_name = self.feed_name,
                    source_name = self.source_name,
                    "Failed to submit value: {value}"
                );
                self.metrics
                    .upload_data_errors
                    .with_label_values(&[&self.feed_name, &self.source_name])
                    .inc();
            }
        }
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

    async fn submit(&self, value: u64) -> anyhow::Result<SuiTransactionBlockEffects> {
        let _scope = monitored_scope("Oracle::DataProvider::submit");
        // TODO add error handling & polling perhaps

        // let data = TransactionData::new_move_call(
        //     self.signer_address,
        //     self.upload_feed.write_package_id,
        //     ident_str!(&self.upload_feed.write_module_name).to_owned(),
        //     ident_str!(&self.upload_feed.write_function_name).to_owned(),
        //     // FIXME
        //     vec![TypeTag::U64],
        //     gas1,
        //     vec![
        //         CallArg::Object(ObjectArg::SharedObject {})
        // //             SuiJsonValue::new(json!(self.upload_feed.write_data_provider_object_id)).unwrap(),
        //         CallArg::CLOCK_IMM
        //     ],
        //     TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
        //     rgp,
        // )
        // .unwrap();

        let tx = self
            .client
            .transaction_builder()
            .move_call(
                self.signer_address,
                self.upload_feed.write_package_id,
                &self.upload_feed.write_module_name,
                &self.upload_feed.write_function_name,
                vec![SuiTypeTag::try_from(TypeTag::U64).unwrap()],
                vec![
                    SuiJsonValue::new(json!(self.upload_feed.write_data_provider_object_id)).unwrap(),
                    SuiJsonValue::new(json!(ObjectID::from_hex_literal("0x06").unwrap())).unwrap(),
                    SuiJsonValue::new(json!("SUIUSD")).unwrap(),
                    SuiJsonValue::new(json!(value.to_string())).unwrap(),
                    // SuiJsonValue::new(convert_number_to_string(value.to_json_value())))
                    SuiJsonValue::new(json!("tests")).unwrap(),
                ],
                None,
                100_000_000,
            )
            .await?;

        let signed_tx = self.wallet_ctx.sign_transaction(&tx);
        // TODO: maybe don't wait for local execution but instead keep a local cache of gas objects?
        let response = self
            .client
            .quorum_driver_api()
            .execute_transaction_block(
                signed_tx,
                SuiTransactionBlockResponseOptions::new().with_effects(),
                Some(sui_types::quorum_driver_types::ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;

        match response.status_ok() {
            // If `status_ok`, `effects` must be `Some`
            Some(true) => Ok(response.effects.unwrap()),
            _other => anyhow::bail!(
                "Failed to submit data on chain or cannot find status in effects: {:?}",
                response.errors
            ),
        }
    }
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
                        // FIXME parse value based on returned BCS
                        let value = 5_f64;
                        let _ = sender.send((feed_name.clone(), *object_id, value)).await;
                        self.metrics
                            .downloaded_values
                            .with_label_values(&[feed_name])
                            .observe((value * METRICS_MULTIPLIER) as u64);
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
