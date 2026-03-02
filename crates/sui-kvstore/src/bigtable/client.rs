// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::RwLock;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;
use std::time::Instant;

use crate::EpochData;
use anyhow::Context as _;
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use gcp_auth::Token;
use gcp_auth::TokenProvider;
use http::HeaderValue;
use http::Request;
use http::Response;
use prometheus::Registry;
use sui_types::base_types::EpochId;
use sui_types::base_types::ObjectID;
use sui_types::base_types::TransactionDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tonic::body::Body;
use tonic::codegen::Service;
use tonic::transport::Certificate;
use tonic::transport::Channel;
use tonic::transport::ClientTlsConfig;

use crate::CheckpointData;
use crate::KeyValueStoreReader;
use crate::PackageData;
use crate::ProtocolConfigData;
use crate::TransactionData;
use crate::TransactionEventsData;
use crate::Watermark;
use crate::bigtable::metrics::KvMetrics;
use crate::bigtable::proto::bigtable::v2::MutateRowsRequest;
use crate::bigtable::proto::bigtable::v2::ReadRowsRequest;
use crate::bigtable::proto::bigtable::v2::RequestStats;
use crate::bigtable::proto::bigtable::v2::RowFilter;
use crate::bigtable::proto::bigtable::v2::RowRange;
use crate::bigtable::proto::bigtable::v2::RowSet;
use crate::bigtable::proto::bigtable::v2::bigtable_client::BigtableClient as BigtableInternalClient;
use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::bigtable::proto::bigtable::v2::read_rows_response::cell_chunk::RowStatus;
use crate::bigtable::proto::bigtable::v2::request_stats::StatsView;
use crate::bigtable::proto::bigtable::v2::row_filter::Chain;
use crate::bigtable::proto::bigtable::v2::row_filter::Filter;
use crate::bigtable::proto::bigtable::v2::row_range::EndKey;
use crate::bigtable::proto::bigtable::v2::row_range::StartKey;
use crate::tables;

const DEFAULT_MAX_DECODING_MESSAGE_SIZE: usize = 32 * 1024 * 1024;
// TODO: Add per-method timeouts (e.g. separate write vs read) via tonic::Request::set_timeout().
const DEFAULT_CHANNEL_TIMEOUT: Duration = Duration::from_secs(60);

/// Default number of gRPC channels in the pool. Each channel is an independent HTTP/2
/// connection, allowing concurrent RPCs to spread across multiple TCP sockets instead
/// of multiplexing on a single one. Matches the Google java-bigtable client default.
const DEFAULT_CHANNEL_POOL_SIZE: usize = 10;

/// Error returned when a batch write has per-entry failures.
/// Contains the keys and error details for each failed mutation.
#[derive(Debug)]
pub struct PartialWriteError {
    pub failed_keys: Vec<MutationError>,
}

#[derive(Debug)]
pub struct MutationError {
    pub key: Bytes,
    pub code: i32,
    pub message: String,
}

#[derive(Clone)]
pub struct BigTableClient {
    table_prefix: String,
    client: BigtableInternalClient<AuthChannel>,
    client_name: String,
    metrics: Option<Arc<KvMetrics>>,
    app_profile_id: Option<String>,
}

#[derive(Clone)]
struct AuthChannel {
    channel: Channel,
    policy: String,
    token_provider: Option<Arc<dyn TokenProvider>>,
    token: Arc<RwLock<Option<Arc<Token>>>>,
}

impl BigTableClient {
    pub async fn new_local(host: String, instance_id: String) -> Result<Self> {
        Self::new_for_host(host, instance_id, "local")
    }

    /// Create a client connected to a specific host.
    /// Used internally and for testing with mock servers.
    pub(crate) fn new_for_host(
        host: String,
        instance_id: String,
        client_name: &str,
    ) -> Result<Self> {
        let auth_channel = AuthChannel {
            channel: Channel::from_shared(format!("http://{host}"))?.connect_lazy(),
            policy: "https://www.googleapis.com/auth/bigtable.data".to_string(),
            token_provider: None,
            token: Arc::new(RwLock::new(None)),
        };
        Ok(Self {
            table_prefix: format!("projects/emulator/instances/{}/tables/", instance_id),
            client: BigtableInternalClient::new(auth_channel),
            client_name: client_name.to_string(),
            metrics: None,
            app_profile_id: None,
        })
    }

    pub async fn new_remote(
        instance_id: String,
        project_id: Option<String>,
        is_read_only: bool,
        timeout: Option<Duration>,
        max_decoding_message_size: Option<usize>,
        client_name: String,
        registry: Option<&Registry>,
        app_profile_id: Option<String>,
        channel_pool_size: Option<usize>,
    ) -> Result<Self> {
        let pool_size = channel_pool_size
            .unwrap_or(DEFAULT_CHANNEL_POOL_SIZE)
            .max(1);
        let policy = if is_read_only {
            "https://www.googleapis.com/auth/bigtable.data.readonly"
        } else {
            "https://www.googleapis.com/auth/bigtable.data"
        };
        let token_provider = gcp_auth::provider().await?;
        let tls_config = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(include_bytes!("./proto/google.pem")))
            .domain_name("bigtable.googleapis.com");
        let mut endpoint = Channel::from_static("https://bigtable.googleapis.com")
            .http2_keep_alive_interval(Duration::from_secs(60))
            .keep_alive_while_idle(true)
            .tls_config(tls_config)?;
        endpoint = endpoint.timeout(timeout.unwrap_or(DEFAULT_CHANNEL_TIMEOUT));
        let project_id = match project_id {
            Some(p) => p,
            None => token_provider.project_id().await?.to_string(),
        };
        let table_prefix = format!("projects/{}/instances/{}/tables/", project_id, instance_id);
        let channel = if pool_size > 1 {
            let (channel, tx) = Channel::balance_channel::<usize>(64);
            for i in 0..pool_size {
                tx.try_send(tonic::transport::channel::Change::Insert(
                    i,
                    endpoint.clone(),
                ))
                .expect("channel balancer dropped");
            }
            channel
        } else {
            endpoint.connect_lazy()
        };
        let auth_channel = AuthChannel {
            channel,
            policy: policy.to_string(),
            token_provider: Some(token_provider),
            token: Arc::new(RwLock::new(None)),
        };
        let client = BigtableInternalClient::new(auth_channel).max_decoding_message_size(
            max_decoding_message_size.unwrap_or(DEFAULT_MAX_DECODING_MESSAGE_SIZE),
        );
        Ok(Self {
            table_prefix,
            client,
            client_name,
            metrics: registry.map(KvMetrics::new),
            app_profile_id,
        })
    }

    /// Get the pipeline watermark from the watermarks table.
    /// Falls back to the legacy `[0]` row if the pipeline-specific row is missing.
    // TODO(migration): Remove legacy fallback once all pipelines have their own watermarks.
    pub async fn get_pipeline_watermark(&mut self, pipeline: &str) -> Result<Option<Watermark>> {
        let pipeline_key = tables::watermarks::encode_key(pipeline);
        let legacy_key = vec![0u8];

        let rows = self
            .multi_get(
                tables::watermark_alt_legacy::NAME,
                vec![pipeline_key.clone(), legacy_key.clone()],
                None,
            )
            .await?;

        let mut pipeline_wm = None;
        let mut legacy_checkpoint = None;

        for (key, row) in rows {
            if key.as_ref() == pipeline_key.as_slice() {
                pipeline_wm = Some(tables::watermarks::decode(&row)?);
            } else if key.as_ref() == legacy_key.as_slice()
                && let Some((_, value_bytes)) = row.last()
            {
                let next = u64::from_be_bytes(value_bytes.as_ref().try_into()?);
                if next > 0 {
                    legacy_checkpoint = Some(next - 1);
                }
            }
        }

        if let Some(wm) = pipeline_wm {
            return Ok(Some(wm));
        }

        // Don't fall back to legacy watermark when legacy mode is disabled.
        // This prevents tasked backfill pipelines from inheriting the main
        // pipeline's watermark from the legacy [0] row.
        if !crate::write_legacy_data() {
            return Ok(None);
        }

        Ok(legacy_checkpoint.map(|cp| Watermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: cp,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
        }))
    }

    /// Set the pipeline watermark in the watermarks table.
    pub async fn set_pipeline_watermark(
        &mut self,
        pipeline: &str,
        watermark: &Watermark,
    ) -> Result<()> {
        let entry = tables::make_entry(
            tables::watermarks::encode_key(pipeline),
            tables::watermarks::encode(watermark)?,
            Some(watermark.timestamp_ms_hi_inclusive),
        );
        self.write_entries(tables::watermarks::NAME, [entry]).await
    }

    /// Write pre-built entries to BigTable.
    ///
    /// On partial failure (some entries succeed, some fail), returns a `PartialWriteError`
    /// containing the keys that failed. Callers can retain only the failed keys in their
    /// batch before retrying.
    pub async fn write_entries(
        &mut self,
        table: &str,
        entries: impl IntoIterator<Item = Entry>,
    ) -> Result<()> {
        let entries: Vec<Entry> = entries.into_iter().collect();
        if entries.is_empty() {
            return Ok(());
        }

        let row_keys: Vec<Bytes> = entries.iter().map(|e| e.row_key.clone()).collect();

        let mut request = MutateRowsRequest {
            table_name: format!("{}{}", self.table_prefix, table),
            entries,
            ..MutateRowsRequest::default()
        };
        if let Some(ref app_profile_id) = self.app_profile_id {
            request.app_profile_id = app_profile_id.clone();
        }
        let mut response = self.client.clone().mutate_rows(request).await?.into_inner();
        let mut failed_keys: Vec<MutationError> = Vec::new();

        while let Some(part) = response.message().await? {
            for entry in part.entries {
                if let Some(status) = entry.status
                    && status.code != 0
                    && let Some(key) = row_keys.get(entry.index as usize)
                {
                    failed_keys.push(MutationError {
                        key: key.clone(),
                        code: status.code,
                        message: status.message,
                    });
                }
            }
        }

        if !failed_keys.is_empty() {
            return Err(PartialWriteError { failed_keys }.into());
        }

        Ok(())
    }

    /// Generate a raw object key from ObjectKey.
    pub fn raw_object_key(object_key: &ObjectKey) -> Vec<u8> {
        tables::objects::encode_key(object_key)
    }

    pub async fn read_rows(
        &mut self,
        mut request: ReadRowsRequest,
        table_name: &str,
    ) -> Result<Vec<(Bytes, Vec<(Bytes, Bytes)>)>> {
        // Zero-copy accumulator for cell values. BigTable streams cell data in chunks,
        // and prost deserializes each chunk.value as a Bytes view into the gRPC buffer
        // (no allocation). This enum preserves that zero-copy benefit:
        //
        // - Single chunk (common): stays as Bytes, no copies at all
        // - Multiple chunks (only for values >1MB): copies into Vec<u8>
        #[derive(Default)]
        enum CellValue {
            #[default]
            Empty,
            Single(Bytes),
            Multi(Vec<u8>),
        }

        impl CellValue {
            fn extend(&mut self, data: Bytes) {
                *self = match std::mem::take(self) {
                    CellValue::Empty => CellValue::Single(data),
                    // Second chunk arrives - must allocate and copy
                    CellValue::Single(existing) => {
                        let mut vec = existing.to_vec();
                        vec.extend_from_slice(&data);
                        CellValue::Multi(vec)
                    }
                    CellValue::Multi(mut vec) => {
                        vec.extend_from_slice(&data);
                        CellValue::Multi(vec)
                    }
                };
            }

            fn replace(&mut self, data: Bytes) {
                *self = CellValue::Single(data);
            }

            fn into_bytes(self) -> Bytes {
                match self {
                    CellValue::Empty => Bytes::new(),
                    CellValue::Single(b) => b, // zero-copy: return the original Bytes
                    CellValue::Multi(v) => Bytes::from(v),
                }
            }
        }

        if let Some(ref app_profile_id) = self.app_profile_id {
            request.app_profile_id = app_profile_id.clone();
        }
        let mut result = vec![];
        let mut response = self.client.clone().read_rows(request).await?.into_inner();

        let mut row_key: Option<Bytes> = None;
        let mut row = vec![];
        let mut cell_value = CellValue::Empty;
        let mut cell_name: Option<Bytes> = None;
        let mut timestamp = 0;

        while let Some(message) = response.message().await? {
            self.report_bt_stats(&message.request_stats, table_name);
            for chunk in message.chunks.into_iter() {
                // new row check
                if !chunk.row_key.is_empty() {
                    row_key = Some(chunk.row_key);
                }
                match chunk.qualifier {
                    // new cell started
                    Some(qualifier) => {
                        if let Some(name) = cell_name.take() {
                            row.push((name, cell_value.into_bytes()));
                            cell_value = CellValue::Empty;
                        }
                        cell_name = Some(Bytes::from(qualifier));
                        timestamp = chunk.timestamp_micros;
                        cell_value.extend(chunk.value);
                    }
                    None => {
                        if chunk.timestamp_micros == 0 {
                            cell_value.extend(chunk.value);
                        } else if chunk.timestamp_micros >= timestamp {
                            // newer version of cell is available
                            timestamp = chunk.timestamp_micros;
                            cell_value.replace(chunk.value);
                        }
                    }
                }
                if chunk.row_status.is_some() {
                    if let Some(RowStatus::CommitRow(_)) = chunk.row_status {
                        if let Some(name) = cell_name.take() {
                            row.push((name, cell_value.into_bytes()));
                        }
                        if let Some(key) = row_key.take() {
                            result.push((key, row));
                        }
                    }
                    row_key = None;
                    row = vec![];
                    cell_value = CellValue::Empty;
                    cell_name = None;
                }
            }
        }
        Ok(result)
    }

    pub async fn multi_get(
        &mut self,
        table_name: &str,
        keys: Vec<Vec<u8>>,
        filter: Option<RowFilter>,
    ) -> Result<Vec<(Bytes, Vec<(Bytes, Bytes)>)>> {
        let start_time = Instant::now();
        let num_keys_requested = keys.len();
        let result = self.multi_get_internal(table_name, keys, filter).await;
        let elapsed_ms = start_time.elapsed().as_millis() as f64;

        let Some(metrics) = &self.metrics else {
            return result;
        };

        let labels = [&self.client_name, table_name];
        let Ok(rows) = &result else {
            metrics.kv_get_errors.with_label_values(&labels).inc();
            return result;
        };

        metrics
            .kv_get_batch_size
            .with_label_values(&labels)
            .observe(num_keys_requested as f64);

        if num_keys_requested > rows.len() {
            metrics
                .kv_get_not_found
                .with_label_values(&labels)
                .inc_by((num_keys_requested - rows.len()) as u64);
        }

        metrics
            .kv_get_success
            .with_label_values(&labels)
            .inc_by(rows.len() as u64);

        metrics
            .kv_get_latency_ms
            .with_label_values(&labels)
            .observe(elapsed_ms);

        if num_keys_requested > 0 {
            metrics
                .kv_get_latency_ms_per_key
                .with_label_values(&labels)
                .observe(elapsed_ms / num_keys_requested as f64);
        }

        result
    }

    fn report_bt_stats(&self, request_stats: &Option<RequestStats>, table_name: &str) {
        let Some(metrics) = &self.metrics else {
            return;
        };
        let labels = [&self.client_name, table_name];
        if let Some(StatsView::FullReadStatsView(view)) =
            request_stats.as_ref().and_then(|r| r.stats_view.as_ref())
        {
            if let Some(latency) = view
                .request_latency_stats
                .as_ref()
                .and_then(|s| s.frontend_server_latency)
            {
                if latency.seconds < 0 || latency.nanos < 0 {
                    return;
                }
                let duration = Duration::new(latency.seconds as u64, latency.nanos as u32);
                metrics
                    .kv_bt_chunk_latency_ms
                    .with_label_values(&labels)
                    .observe(duration.as_millis() as f64);
            }
            if let Some(iteration_stats) = &view.read_iteration_stats {
                metrics
                    .kv_bt_chunk_rows_returned_count
                    .with_label_values(&labels)
                    .inc_by(iteration_stats.rows_returned_count as u64);
                metrics
                    .kv_bt_chunk_rows_seen_count
                    .with_label_values(&labels)
                    .inc_by(iteration_stats.rows_seen_count as u64);
            }
        }
    }

    async fn multi_get_internal(
        &mut self,
        table_name: &str,
        keys: Vec<Vec<u8>>,
        filter: Option<RowFilter>,
    ) -> Result<Vec<(Bytes, Vec<(Bytes, Bytes)>)>> {
        let version_filter = RowFilter {
            filter: Some(Filter::CellsPerColumnLimitFilter(1)),
        };
        let filter = Some(match filter {
            Some(filter) => RowFilter {
                filter: Some(Filter::Chain(Chain {
                    filters: vec![filter, version_filter],
                })),
            },
            None => version_filter,
        });
        let request = ReadRowsRequest {
            table_name: format!("{}{}", self.table_prefix, table_name),
            rows_limit: keys.len() as i64,
            rows: Some(RowSet {
                row_keys: keys.into_iter().map(Bytes::from).collect(),
                row_ranges: vec![],
            }),
            filter,
            request_stats_view: 2,
            ..ReadRowsRequest::default()
        };
        let mut result = vec![];
        for (key, cells) in self.read_rows(request, table_name).await? {
            result.push((key, cells));
        }
        Ok(result)
    }

    /// Scan a range of rows with optional start/end keys, limit, and direction.
    /// Applies `CellsPerColumnLimitFilter(1)` like `multi_get_internal`.
    pub(crate) async fn range_scan(
        &mut self,
        table_name: &str,
        start_key: Option<Bytes>,
        end_key: Option<Bytes>,
        limit: i64,
        reversed: bool,
    ) -> Result<Vec<(Bytes, Vec<(Bytes, Bytes)>)>> {
        let start_time = Instant::now();
        let result = self
            .range_scan_internal(table_name, start_key, end_key, limit, reversed)
            .await;
        let elapsed_ms = start_time.elapsed().as_millis() as f64;
        let labels = [&self.client_name, table_name];
        match &self.metrics {
            Some(metrics) => match result {
                Ok(result) => {
                    metrics.kv_scan_success.with_label_values(&labels).inc();
                    if result.is_empty() {
                        metrics.kv_scan_not_found.with_label_values(&labels).inc();
                    }
                    metrics
                        .kv_scan_latency_ms
                        .with_label_values(&labels)
                        .observe(elapsed_ms);
                    Ok(result)
                }
                Err(e) => {
                    metrics.kv_scan_error.with_label_values(&labels).inc();
                    Err(e)
                }
            },
            None => result,
        }
    }

    async fn range_scan_internal(
        &mut self,
        table_name: &str,
        start_key: Option<Bytes>,
        end_key: Option<Bytes>,
        limit: i64,
        reversed: bool,
    ) -> Result<Vec<(Bytes, Vec<(Bytes, Bytes)>)>> {
        let range = RowRange {
            start_key: start_key.map(StartKey::StartKeyClosed),
            end_key: end_key.map(EndKey::EndKeyClosed),
        };
        let filter = Some(RowFilter {
            filter: Some(Filter::CellsPerColumnLimitFilter(1)),
        });
        let request = ReadRowsRequest {
            table_name: format!("{}{}", self.table_prefix, table_name),
            rows_limit: limit,
            rows: Some(RowSet {
                row_keys: vec![],
                row_ranges: vec![range],
            }),
            filter,
            reversed,
            request_stats_view: 2,
            ..ReadRowsRequest::default()
        };
        self.read_rows(request, table_name).await
    }
}

impl std::fmt::Display for PartialWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "partial write: {} entries failed",
            self.failed_keys.len()
        )?;
        for failed in &self.failed_keys {
            write!(f, "\n  code {}: {}", failed.code, failed.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for PartialWriteError {}

#[async_trait]
impl KeyValueStoreReader for BigTableClient {
    async fn get_objects(&mut self, object_keys: &[ObjectKey]) -> Result<Vec<Object>> {
        let keys: Vec<Vec<u8>> = object_keys.iter().map(Self::raw_object_key).collect();
        let mut objects = vec![];
        for (_, row) in self.multi_get(tables::objects::NAME, keys, None).await? {
            objects.push(tables::objects::decode(&row)?);
        }
        Ok(objects)
    }

    async fn get_transactions(
        &mut self,
        transactions: &[TransactionDigest],
    ) -> Result<Vec<TransactionData>> {
        let keys = transactions
            .iter()
            .map(tables::transactions::encode_key)
            .collect();
        let mut result = vec![];
        for (_, row) in self
            .multi_get(tables::transactions::NAME, keys, None)
            .await?
        {
            result.push(tables::transactions::decode(&row)?);
        }
        Ok(result)
    }

    async fn get_checkpoints(
        &mut self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> Result<Vec<CheckpointData>> {
        let keys = sequence_numbers
            .iter()
            .copied()
            .map(tables::checkpoints::encode_key)
            .collect();
        let mut checkpoints = vec![];
        for (_, row) in self
            .multi_get(tables::checkpoints::NAME, keys, None)
            .await?
        {
            checkpoints.push(tables::checkpoints::decode(&row)?);
        }
        Ok(checkpoints)
    }

    async fn get_checkpoint_by_digest(
        &mut self,
        digest: CheckpointDigest,
    ) -> Result<Option<CheckpointData>> {
        let key = tables::checkpoints_by_digest::encode_key(&digest);
        let mut response = self
            .multi_get(tables::checkpoints_by_digest::NAME, vec![key], None)
            .await?;
        if let Some((_, row)) = response.pop() {
            let sequence_number = tables::checkpoints_by_digest::decode(&row)?;
            if let Some(chk) = self.get_checkpoints(&[sequence_number]).await?.pop() {
                return Ok(Some(chk));
            }
        }
        Ok(None)
    }

    async fn get_watermark_for_pipelines(
        &mut self,
        pipelines: &[&str],
    ) -> Result<Option<Watermark>> {
        let keys: Vec<Vec<u8>> = pipelines
            .iter()
            .map(|name| tables::watermarks::encode_key(name))
            .collect();

        let rows = self
            .multi_get(tables::watermark_alt_legacy::NAME, keys, None)
            .await?;

        if rows.len() != pipelines.len() {
            return Ok(None);
        }

        let mut min_wm: Option<Watermark> = None;
        for (_, row) in &rows {
            let wm = tables::watermarks::decode(row)?;
            min_wm = Some(match min_wm {
                Some(prev) if prev.checkpoint_hi_inclusive <= wm.checkpoint_hi_inclusive => prev,
                _ => wm,
            });
        }

        Ok(min_wm)
    }

    async fn get_latest_object(&mut self, object_id: &ObjectID) -> Result<Option<Object>> {
        let upper_limit = Bytes::from(Self::raw_object_key(&ObjectKey::max_for_id(object_id)));
        if let Some((_, row)) = self
            .range_scan(tables::objects::NAME, None, Some(upper_limit), 1, true)
            .await?
            .pop()
        {
            return Ok(Some(tables::objects::decode(&row)?));
        }
        Ok(None)
    }

    async fn get_epoch(&mut self, epoch_id: EpochId) -> Result<Option<EpochData>> {
        let key = tables::epochs::encode_key(epoch_id);
        match self
            .multi_get(tables::epochs::NAME, vec![key], None)
            .await?
            .pop()
        {
            Some((_, row)) => Ok(Some(tables::epochs::decode(&row)?)),
            None => Ok(None),
        }
    }

    async fn get_protocol_configs(
        &mut self,
        protocol_version: u64,
    ) -> Result<Option<ProtocolConfigData>> {
        let key = tables::protocol_configs::encode_key(protocol_version);
        match self
            .multi_get(tables::protocol_configs::NAME, vec![key], None)
            .await?
            .pop()
        {
            Some((_, row)) => Ok(Some(tables::protocol_configs::decode(&row)?)),
            None => Ok(None),
        }
    }

    async fn get_latest_epoch(&mut self) -> Result<Option<EpochData>> {
        let upper_limit = tables::epochs::encode_key_upper_bound();
        match self
            .range_scan(tables::epochs::NAME, None, Some(upper_limit), 1, true)
            .await?
            .pop()
        {
            Some((_, row)) => Ok(Some(tables::epochs::decode(&row)?)),
            None => Ok(None),
        }
    }

    async fn get_events_for_transactions(
        &mut self,
        transaction_digests: &[TransactionDigest],
    ) -> Result<Vec<(TransactionDigest, TransactionEventsData)>> {
        let query = self.multi_get(
            tables::transactions::NAME,
            transaction_digests
                .iter()
                .map(tables::transactions::encode_key)
                .collect(),
            Some(RowFilter {
                filter: Some(Filter::ColumnQualifierRegexFilter(
                    format!(
                        "^({}|{})$",
                        tables::transactions::col::EVENTS,
                        tables::transactions::col::TIMESTAMP
                    )
                    .into(),
                )),
            }),
        );
        let mut results = vec![];

        for (key, row) in query.await? {
            let events_data = tables::transactions::decode_events(&row)?;

            let key_array: [u8; 32] = key
                .as_ref()
                .try_into()
                .context("Failed to deserialize transaction digest")?;
            let transaction_digest = TransactionDigest::from(key_array);

            results.push((transaction_digest, events_data));
        }

        Ok(results)
    }

    async fn get_package_original_ids(
        &mut self,
        package_ids: &[ObjectID],
    ) -> Result<Vec<(ObjectID, ObjectID)>> {
        let keys: Vec<Vec<u8>> = package_ids
            .iter()
            .map(|id| tables::packages_by_id::encode_key(id.as_ref()))
            .collect();
        let mut results = vec![];
        for (key, row) in self
            .multi_get(tables::packages_by_id::NAME, keys, None)
            .await?
        {
            let original_id_bytes = tables::packages_by_id::decode(&row)?;
            let pkg_id = ObjectID::from_bytes(key.as_ref())?;
            let original_id = ObjectID::from_bytes(&original_id_bytes)?;
            results.push((pkg_id, original_id));
        }
        Ok(results)
    }

    async fn get_packages_by_version(
        &mut self,
        keys: &[(ObjectID, u64)],
    ) -> Result<Vec<PackageData>> {
        let raw_keys: Vec<Vec<u8>> = keys
            .iter()
            .map(|(original_id, version)| {
                tables::packages::encode_key(original_id.as_ref(), *version)
            })
            .collect();
        let mut results = vec![];
        for (key, row) in self
            .multi_get(tables::packages::NAME, raw_keys, None)
            .await?
        {
            results.push(tables::packages::decode(key.as_ref(), &row)?);
        }
        Ok(results)
    }

    async fn get_package_latest(
        &mut self,
        original_id: ObjectID,
        cp_bound: u64,
    ) -> Result<Option<PackageData>> {
        // Over-fetch up to 50 versions in reverse order, then filter by cp_bound.
        // Packages rarely have 50+ upgrades.
        let start_key = Bytes::from(tables::packages::encode_key(original_id.as_ref(), 0));
        let end_key = Bytes::from(tables::packages::encode_key_upper_bound(
            original_id.as_ref(),
        ));

        let rows = self
            .range_scan(
                tables::packages::NAME,
                Some(start_key),
                Some(end_key),
                50,
                true,
            )
            .await?;

        for (key, row) in rows {
            let pkg = tables::packages::decode(key.as_ref(), &row)?;
            if pkg.cp_sequence_number <= cp_bound {
                return Ok(Some(pkg));
            }
        }
        Ok(None)
    }

    async fn get_package_versions(
        &mut self,
        original_id: ObjectID,
        cp_bound: u64,
        after_version: Option<u64>,
        before_version: Option<u64>,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<PackageData>> {
        let start_version = after_version.map(|v| v + 1).unwrap_or(0);
        let end_version = before_version.map(|v| v - 1).unwrap_or(u64::MAX);

        let start_key = Bytes::from(tables::packages::encode_key(
            original_id.as_ref(),
            start_version,
        ));
        let end_key = Bytes::from(tables::packages::encode_key(
            original_id.as_ref(),
            end_version,
        ));

        // Over-fetch to account for versions beyond cp_bound that need filtering out.
        let fetch_limit = (limit as i64).saturating_mul(2).min(200);
        let rows = self
            .range_scan(
                tables::packages::NAME,
                Some(start_key),
                Some(end_key),
                fetch_limit,
                descending,
            )
            .await?;

        let mut results = Vec::with_capacity(limit);
        for (key, row) in rows {
            if results.len() >= limit {
                break;
            }
            let pkg = tables::packages::decode(key.as_ref(), &row)?;
            if pkg.cp_sequence_number <= cp_bound {
                results.push(pkg);
            }
        }
        Ok(results)
    }

    async fn get_packages_by_checkpoint_range(
        &mut self,
        cp_after: Option<u64>,
        cp_before: Option<u64>,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<PackageData>> {
        let start_cp = cp_after.map(|c| c + 1).unwrap_or(0);
        let end_cp = cp_before.map(|c| c - 1).unwrap_or(u64::MAX);

        let start_key = Bytes::from(tables::packages_by_checkpoint::encode_key(
            start_cp, &[0u8; 32], 0,
        ));
        let end_key = Bytes::from(tables::packages_by_checkpoint::encode_key(
            end_cp,
            &[0xff; 32],
            u64::MAX,
        ));

        let rows = self
            .range_scan(
                tables::packages_by_checkpoint::NAME,
                Some(start_key),
                Some(end_key),
                limit as i64,
                descending,
            )
            .await?;

        // Extract (original_id, version) from index keys, then batch-fetch from packages table.
        let lookup_keys: Vec<(ObjectID, u64)> = rows
            .iter()
            .map(|(key, _)| {
                let (_, original_id, version) =
                    tables::packages_by_checkpoint::decode_key(key.as_ref())?;
                Ok((ObjectID::from_bytes(&original_id)?, version))
            })
            .collect::<Result<Vec<_>>>()?;

        self.get_packages_by_version(&lookup_keys).await
    }

    async fn get_system_packages(
        &mut self,
        cp_bound: u64,
        after_original_id: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<PackageData>> {
        let start_key = after_original_id.map(|id| {
            // Start just after the given original_id by appending a byte.
            let mut key = tables::system_packages::encode_key(id.as_ref());
            key.push(0);
            Bytes::from(key)
        });
        let end_key = Some(Bytes::from(tables::system_packages::encode_key(
            &[0xff; 32],
        )));

        let rows = self
            .range_scan(
                tables::system_packages::NAME,
                start_key,
                end_key,
                limit as i64,
                false,
            )
            .await?;

        let mut results = Vec::with_capacity(rows.len());
        for (key, row) in &rows {
            let first_cp = tables::system_packages::decode(row)?;
            if first_cp > cp_bound {
                continue;
            }
            let original_id = ObjectID::from_bytes(key.as_ref())?;
            if let Some(pkg) = self.get_package_latest(original_id, cp_bound).await? {
                results.push(pkg);
            }
        }
        Ok(results)
    }
}

impl Service<Request<Body>> for AuthChannel {
    type Response = Response<Body>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.channel.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let cloned_channel = self.channel.clone();
        let cloned_token = self.token.clone();
        let mut inner = std::mem::replace(&mut self.channel, cloned_channel);
        let policy = self.policy.clone();
        let token_provider = self.token_provider.clone();

        let mut auth_token = None;
        if token_provider.is_some() {
            let guard = self.token.read().expect("failed to acquire a read lock");
            if let Some(token) = &*guard
                && !token.has_expired()
            {
                auth_token = Some(token.clone());
            }
        }

        Box::pin(async move {
            if let Some(ref provider) = token_provider {
                let token = match auth_token {
                    None => {
                        let new_token = provider.token(&[policy.as_ref()]).await?;
                        let mut guard = cloned_token.write().unwrap();
                        *guard = Some(new_token.clone());
                        new_token
                    }
                    Some(token) => token,
                };
                let token_string = token.as_str().parse::<String>()?;
                let header =
                    HeaderValue::from_str(format!("Bearer {}", token_string.as_str()).as_str())?;
                request.headers_mut().insert("authorization", header);
            }
            // enable reverse scan
            let header = HeaderValue::from_static("CAE=");
            request.headers_mut().insert("bigtable-features", header);
            Ok(inner.call(request).await?)
        })
    }
}
