// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Parser;
use csv::Writer;
use dashmap::DashMap;
use futures::{stream, StreamExt};
use object_store::{
    gcp::GoogleCloudStorageBuilder, path::Path, Error as ObjectStoreError, ObjectStore,
};
use prometheus::Registry;
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sui_indexer_alt_framework::ingestion::client::IngestionClient;
use sui_indexer_alt_framework::metrics::IndexerMetrics;
use sui_types::display::DisplayVersionUpdatedEvent;
use sui_types::event::Event;
use telemetry_subscribers::TelemetryConfig;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use url::Url;

const DEFAULT_FULLNODE_URL: &str = "https://fullnode.mainnet.sui.io:443";

#[cfg(test)]
mod tests;

/// Checkpoint data with associated display entries
#[derive(Clone, Debug)]
struct CheckpointDisplayData {
    epoch: u64,
    display_entries: Vec<DisplayEntry>,
    is_end_of_epoch: bool,
}

/// Map of checkpoint number to its display data
type DisplayUpdateMap = DashMap<u64, CheckpointDisplayData>;

/// Configuration for the display upload tool
#[derive(Parser, Debug, Clone)]
#[command(name = "sui-upload-display", about = "Upload display data to GCS")]
pub struct Config {
    /// Path to the GCS credentials file
    #[arg(long)]
    gcs_cred_path: Option<String>,

    /// Name of the GCS bucket to upload display data to
    #[arg(long)]
    gcs_display_bucket: Option<String>,

    /// URL of the fullnode to fetch checkpoint data from
    #[arg(long, default_value = DEFAULT_FULLNODE_URL)]
    remote_url: String,

    /// Number of concurrent checkpoints to process
    #[arg(long, default_value = "10")]
    concurrency_limit: usize,

    /// Number of checkpoints to process in one batch
    #[arg(long, default_value = "100")]
    batch_size: u64,
}

// Define a DisplayEntry struct similar to StoredDisplay
#[derive(Debug, Clone)]
struct DisplayEntry {
    object_type: Vec<u8>,
    display_id: Vec<u8>,
    display_version: i16,
    display: Vec<u8>,
}

impl DisplayEntry {
    fn try_from_event(event: &Event) -> Option<Self> {
        let (type_info, display_update) = DisplayVersionUpdatedEvent::try_from_event(event)?;
        let type_bytes = bcs::to_bytes(&type_info).ok()?;
        Some(Self {
            object_type: type_bytes,
            display_id: display_update.id.bytes.to_vec(),
            display_version: display_update.version as i16,
            display: event.contents.clone(),
        })
    }
}

// Struct to track accumulated display data across all epochs
#[derive(Debug)]
struct AccumulatedDisplayData {
    epoch: u64,
    last_checkpoint: u64,
    displays: BTreeMap<Vec<u8>, DisplayEntry>,
}

impl AccumulatedDisplayData {
    fn new(epoch: u64) -> Self {
        Self {
            epoch,
            last_checkpoint: 0,
            displays: BTreeMap::new(),
        }
    }

    fn update_displays(&mut self, new_displays: Vec<DisplayEntry>) {
        for display in new_displays {
            self.displays.insert(display.object_type.clone(), display);
        }
    }
}

/// Uploads bytes data to an object store at the specified path
async fn put(
    store: &dyn ObjectStore,
    path: &Path,
    data: bytes::Bytes,
) -> Result<(), ObjectStoreError> {
    store.put(path, data.into()).await?;
    Ok(())
}

async fn find_last_processed_checkpoint(config: &Config) -> Result<(u64, Option<(u64, String)>)> {
    let (Some(cred_path), Some(bucket)) = (&config.gcs_cred_path, &config.gcs_display_bucket)
    else {
        info!("GCS credentials or bucket not set, starting from checkpoint 0");
        return Ok((0, None));
    };

    info!(
        "Looking for uploaded files in GCS bucket '{}' using credentials from '{}'",
        bucket, cred_path
    );

    let gcs_builder = GoogleCloudStorageBuilder::new()
        .with_service_account_path(cred_path.clone())
        .with_bucket_name(bucket.clone());
    let store = gcs_builder.build().context("Failed to create GCS client")?;
    let file_pattern =
        Regex::new(r"displays_(\d+)_(\d+)\.csv").context("Failed to create regex pattern")?;
    let mut list_stream = store.list(None);
    let mut max_checkpoint = 0;
    let mut max_epoch = 0;
    let mut latest_file_path = None;

    while let Some(result) = list_stream.next().await {
        let object = result?;
        let path = object.location.to_string();
        if let Some(captures) = file_pattern.captures(&path) {
            if let (Some(epoch_capture), Some(checkpoint_capture)) =
                (captures.get(1), captures.get(2))
            {
                if let (Ok(epoch), Ok(checkpoint)) = (
                    epoch_capture.as_str().parse::<u64>(),
                    checkpoint_capture.as_str().parse::<u64>(),
                ) {
                    if checkpoint > max_checkpoint {
                        max_checkpoint = checkpoint;
                        max_epoch = epoch;
                        latest_file_path = Some(path);
                    }
                }
            }
        }
    }

    let starting_checkpoint = max_checkpoint.checked_add(1).unwrap_or(max_checkpoint);
    let latest_file_info = if let Some(path) = latest_file_path {
        info!(
            "Found file with max checkpoint {}: {}",
            max_checkpoint, path
        );
        Some((max_epoch, path))
    } else {
        info!("No previous display files found");
        None
    };
    info!("Starting from checkpoint {}", starting_checkpoint);

    Ok((starting_checkpoint, latest_file_info))
}

async fn load_display_entries_from_file(
    config: &Config,
    file_path: &str,
    epoch: u64,
) -> Result<AccumulatedDisplayData> {
    let (Some(cred_path), Some(bucket)) = (&config.gcs_cred_path, &config.gcs_display_bucket)
    else {
        return Err(anyhow::Error::msg("GCS credentials or bucket not set"));
    };

    let gcs_builder = GoogleCloudStorageBuilder::new()
        .with_service_account_path(cred_path.clone())
        .with_bucket_name(bucket.clone());
    let store = gcs_builder.build().context("Failed to create GCS client")?;

    let file_name = file_path.split('/').last().unwrap_or(file_path);
    let path = Path::from(file_name);

    info!("Loading display entries from file: {}", file_path);

    let data = store.get(&path).await?;
    let reader = data.bytes().await?;
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader.as_ref());
    let mut display_data = AccumulatedDisplayData::new(epoch);
    let regex = Regex::new(r"displays_\d+_(\d+)\.csv").context("Failed to create regex pattern")?;
    if let Some(captures) = regex.captures(file_name) {
        if let Some(checkpoint_str) = captures.get(1) {
            if let Ok(checkpoint) = checkpoint_str.as_str().parse::<u64>() {
                display_data.last_checkpoint = checkpoint;
            }
        }
    }

    for result in csv_reader.records() {
        let record = result.context("Failed to read CSV record")?;
        if record.len() < 4 {
            return Err(anyhow::anyhow!("Invalid record with fewer than 4 fields"));
        }
        let parse_hex = |hex: &str| -> Result<Vec<u8>> {
            let hex = hex.trim_start_matches("\\x");
            hex::decode(hex).context("Failed to decode hex string")
        };

        let object_type = parse_hex(&record[0])?;
        let display_id = parse_hex(&record[1])?;
        let display_version = record[2]
            .parse::<i16>()
            .context("Failed to parse display version")?;
        let display = parse_hex(&record[3])?;

        let entry = DisplayEntry {
            object_type,
            display_id: display_id.clone(),
            display_version,
            display,
        };
        display_data.displays.insert(display_id, entry);
    }

    info!(
        "Loaded {} display entries from file",
        display_data.displays.len()
    );
    Ok(display_data)
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = TelemetryConfig::new().with_env().init();
    info!("Starting sui-upload-display service");
    let config = Config::parse();

    let remote_store: Option<Arc<dyn ObjectStore>> =
        match (&config.gcs_cred_path, &config.gcs_display_bucket) {
            (Some(cred_path), Some(bucket)) => {
                info!(
                    "Initializing GCS with bucket '{}' using credentials from '{}'",
                    bucket, cred_path
                );

                let gcs_builder = GoogleCloudStorageBuilder::new()
                    .with_service_account_path(cred_path.clone())
                    .with_bucket_name(bucket.clone());

                match gcs_builder.build() {
                    Ok(store) => {
                        info!("Successfully initialized GCS client");
                        Some(Arc::new(store))
                    }
                    Err(e) => {
                        warn!("Failed to initialize GCS client: {}", e);
                        None
                    }
                }
            }
            _ => {
                warn!(
                    "Either GCS cred path or bucket is not set, display data will not be uploaded"
                );
                None
            }
        };

    let registry = Registry::new();
    let metrics = IndexerMetrics::new(&registry);

    let remote_url = Url::parse(&config.remote_url).context("Failed to parse remote URL")?;
    let client = IngestionClient::new_remote(remote_url, metrics)?;
    info!("Initialized remote client with URL: {}", config.remote_url);

    let cancellation_token = CancellationToken::new();
    let mut accumulated_display_data = None;

    let (starting_checkpoint, latest_file_info) = find_last_processed_checkpoint(&config).await?;
    if let Some((epoch, file_path)) = latest_file_info {
        match load_display_entries_from_file(&config, &file_path, epoch).await {
            Ok(loaded_data) => {
                info!(
                    "Successfully loaded display data from previous epoch {} with {} entries",
                    epoch,
                    loaded_data.displays.len()
                );
                accumulated_display_data = Some(loaded_data);
            }
            Err(e) => {
                warn!("Failed to load display entries from file: {}", e);
                // Continue without previous data
            }
        }
    }

    info!(
        "Service initialized. Processing checkpoints concurrently with concurrency limit {} and batch size {}", 
        config.concurrency_limit,
        config.batch_size
    );
    let display_updates = Arc::new(DisplayUpdateMap::new());
    let config = Arc::new(config);
    let client = Arc::new(client);

    let mut checkpoint_processed = starting_checkpoint;

    loop {
        let end_checkpoint = checkpoint_processed + config.batch_size;
        let checkpoints: Vec<u64> = (checkpoint_processed..end_checkpoint).collect();
        let client_ref = client.as_ref();
        let display_updates_ref = &display_updates;

        let has_errors = Arc::new(AtomicBool::new(false));
        stream::iter(checkpoints)
            .for_each_concurrent(config.concurrency_limit, |checkpoint| {
                let cancel_clone = cancellation_token.clone();
                let display_updates_clone = display_updates_ref.clone();
                let has_errors_clone = has_errors.clone();

                async move {
                    if let Err(e) = process_checkpoint_concurrent(
                        client_ref,
                        cancel_clone,
                        display_updates_clone,
                        checkpoint,
                    )
                    .await
                    {
                        error!("Error processing checkpoint {}: {}", checkpoint, e);
                        has_errors_clone.store(true, Ordering::SeqCst);
                    }
                }
            })
            .await;

        if has_errors.load(Ordering::SeqCst) {
            error!("Some checkpoints failed to process, retrying in 1 second");
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }

        if let Err(e) = apply_display_updates(
            &config,
            &display_updates,
            checkpoint_processed,
            end_checkpoint,
            &mut accumulated_display_data,
            &remote_store,
        )
        .await
        {
            error!("Error applying display updates: {}", e);
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }
        for checkpoint in checkpoint_processed..end_checkpoint {
            display_updates.remove(&checkpoint);
        }

        checkpoint_processed = end_checkpoint;
        info!(
            "Successfully completed batch ending at checkpoint {}",
            end_checkpoint - 1
        );
    }
}

async fn process_checkpoint_concurrent(
    client: &IngestionClient,
    cancel: CancellationToken,
    display_updates: Arc<DisplayUpdateMap>,
    checkpoint: u64,
) -> Result<()> {
    let checkpoint_data = match client.fetch(checkpoint, &cancel).await {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to fetch checkpoint {}: {}", checkpoint, e);
            display_updates.insert(
                checkpoint,
                CheckpointDisplayData {
                    epoch: 0,
                    display_entries: Vec::new(),
                    is_end_of_epoch: false,
                },
            );
            return Err(e.into());
        }
    };

    let summary = &checkpoint_data.checkpoint_summary;
    let epoch = summary.epoch;
    let new_displays: Vec<DisplayEntry> = checkpoint_data
        .transactions
        .iter()
        .filter_map(|tx| tx.events.as_ref())
        .flat_map(|events| events.data.iter())
        .filter_map(DisplayEntry::try_from_event)
        .collect();

    if !new_displays.is_empty() {
        info!(
            "Found {} display updates in checkpoint {} of epoch {}",
            new_displays.len(),
            checkpoint,
            epoch
        );
    }

    let is_end_of_epoch = summary.end_of_epoch_data.is_some();
    display_updates.insert(
        checkpoint,
        CheckpointDisplayData {
            epoch,
            display_entries: new_displays,
            is_end_of_epoch,
        },
    );
    Ok(())
}

async fn apply_display_updates(
    config: &Arc<Config>,
    display_updates: &Arc<DisplayUpdateMap>,
    start_checkpoint: u64,
    end_checkpoint: u64,
    display_data: &mut Option<AccumulatedDisplayData>,
    remote_store: &Option<Arc<dyn ObjectStore>>,
) -> Result<()> {
    let mut end_of_epoch_detected = false;
    let mut last_checkpoint = 0;

    for checkpoint in start_checkpoint..end_checkpoint {
        if let Some(entry) = display_updates.get(&checkpoint) {
            let CheckpointDisplayData {
                epoch,
                display_entries,
                is_end_of_epoch,
            } = entry.clone();

            if display_data.as_ref().is_none_or(|data| data.epoch != epoch) {
                if end_of_epoch_detected && display_data.is_some() {
                    let data = display_data
                        .as_ref()
                        .expect("Display data should exist based on the previous check");

                    info!(
                        "End of epoch {} detected at checkpoint {}, uploading display data",
                        data.epoch, last_checkpoint
                    );
                    upload_display_data(
                        data.epoch,
                        last_checkpoint,
                        data.displays.values().cloned().collect(),
                        config,
                        remote_store,
                    )
                    .await?;
                }

                info!("Starting new epoch {}", epoch);
                *display_data = Some(AccumulatedDisplayData::new(epoch));
                end_of_epoch_detected = false;
            }
            let epoch_data = display_data
                .as_mut()
                .expect("Display data should exist at this point");

            epoch_data.last_checkpoint = checkpoint;
            last_checkpoint = checkpoint;

            if !display_entries.is_empty() {
                epoch_data.update_displays(display_entries);
            }

            if is_end_of_epoch {
                end_of_epoch_detected = true;
            }
        }
    }

    if end_of_epoch_detected && display_data.is_some() {
        let data = display_data
            .as_ref()
            .expect("Display data should exist based on the previous check");

        info!(
            "End of epoch {} detected at checkpoint {}, uploading display data",
            data.epoch, last_checkpoint
        );
        upload_display_data(
            data.epoch,
            last_checkpoint,
            data.displays.values().cloned().collect(),
            config,
            remote_store,
        )
        .await?;
    }

    Ok(())
}

async fn upload_display_data(
    epoch: u64,
    checkpoint: u64,
    displays: Vec<DisplayEntry>,
    _config: &Arc<Config>,
    remote_store: &Option<Arc<dyn ObjectStore>>,
) -> anyhow::Result<()> {
    match displays.len() {
        0 => info!(
            "No display updates for epoch {}, but still uploading empty file",
            epoch
        ),
        count => info!("Uploading {} display entries for epoch {}", count, epoch),
    }

    let filename = format!("displays_{}_{}.csv", epoch, checkpoint);
    let buffer = {
        let mut buffer = Vec::new();
        {
            let mut writer = Writer::from_writer(&mut buffer);
            writer
                .write_record(["object_type", "id", "version", "bcs"])
                .context("Failed to write CSV header")?;
            for display in &displays {
                let record = [
                    format!("\\x{}", hex::encode(&display.object_type)),
                    format!("\\x{}", hex::encode(&display.display_id)),
                    display.display_version.to_string(),
                    format!("\\x{}", hex::encode(&display.display)),
                ];

                writer
                    .write_record(record)
                    .context("Failed to write display entry to CSV")?;
            }

            writer.flush().context("Failed to flush CSV writer")?;
        }
        buffer
    };

    if let Some(store) = remote_store {
        let filename_clone = filename.clone(); // Clone the filename
        let path = Path::from(filename);
        info!("Uploading {} entries to {}", displays.len(), filename_clone);
        let bytes_data = bytes::Bytes::from(buffer);
        put(store, &path, bytes_data).await?;
    } else {
        warn!("GCS not configured, skipping upload");
    }

    Ok(())
}
