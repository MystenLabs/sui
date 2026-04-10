// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod auth_channel;
pub mod bitmap_query;
mod channel_pool;

use std::future::Future;
use std::ops::Range;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context as _;
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use gcp_auth::TokenProvider;
use mysten_common::ZipDebugEqIteratorExt;
use prometheus::Registry;
use sui_types::base_types::EpochId;
use sui_types::base_types::ObjectID;
use sui_types::base_types::TransactionDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tonic::transport::Certificate;
use tonic::transport::Channel;
use tonic::transport::ClientTlsConfig;

use auth_channel::AuthChannel;
use channel_pool::ChannelPool;
use channel_pool::ChannelPrimer;
pub use channel_pool::PoolConfig;

use crate::CheckpointData;
use crate::EpochData;
use crate::KeyValueStoreReader;
use crate::PackageData;
use crate::ProtocolConfigData;
use crate::TransactionData;
use crate::TransactionEventsData;
use crate::Watermark;
use crate::bigtable::metrics::KvMetrics;
use crate::bigtable::proto::bigtable::v2::CheckAndMutateRowRequest;
use crate::bigtable::proto::bigtable::v2::MutateRowsRequest;
use crate::bigtable::proto::bigtable::v2::Mutation;
use crate::bigtable::proto::bigtable::v2::PingAndWarmRequest;
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

struct BigtablePrimer {
    instance_name: String,
    policy: String,
    token_provider: Option<Arc<dyn TokenProvider>>,
}

impl ChannelPrimer for BigtablePrimer {
    fn prime<'a>(
        &'a self,
        channel: &'a Channel,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let auth_channel = AuthChannel::new(
                channel.clone(),
                self.policy.clone(),
                self.token_provider.clone(),
            );
            let mut client = BigtableInternalClient::new(auth_channel);
            client
                .ping_and_warm(PingAndWarmRequest {
                    name: self.instance_name.clone(),
                    app_profile_id: String::new(),
                })
                .await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
pub struct BigTableClient {
    table_prefix: String,
    client: BigtableInternalClient<AuthChannel<ChannelPool>>,
    client_name: String,
    metrics: Option<Arc<KvMetrics>>,
    app_profile_id: Option<String>,
}

impl BigTableClient {
    pub async fn new_local(host: String, instance_id: String) -> Result<Self> {
        Self::new_for_host(host, instance_id, "local").await
    }

    /// Create a client connected to a specific host.
    /// Used internally and for testing with mock servers.
    pub(crate) async fn new_for_host(
        host: String,
        instance_id: String,
        client_name: &str,
    ) -> Result<Self> {
        let endpoint = Channel::from_shared(format!("http://{host}"))?;
        let pool =
            ChannelPool::new_connected(endpoint, PoolConfig::singleton(), None, None).await?;
        let auth_channel = AuthChannel::new(
            pool,
            "https://www.googleapis.com/auth/bigtable.data".to_string(),
            None,
        );
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
        pool_config: PoolConfig,
    ) -> Result<Self> {
        Self::new_remote_with_credentials(
            instance_id,
            project_id,
            is_read_only,
            timeout,
            max_decoding_message_size,
            client_name,
            registry,
            app_profile_id,
            pool_config,
            None,
        )
        .await
    }

    pub async fn new_remote_with_credentials(
        instance_id: String,
        project_id: Option<String>,
        is_read_only: bool,
        timeout: Option<Duration>,
        max_decoding_message_size: Option<usize>,
        client_name: String,
        registry: Option<&Registry>,
        app_profile_id: Option<String>,
        pool_config: PoolConfig,
        credentials_path: Option<String>,
    ) -> Result<Self> {
        let config = pool_config;
        let policy = if is_read_only {
            "https://www.googleapis.com/auth/bigtable.data.readonly"
        } else {
            "https://www.googleapis.com/auth/bigtable.data"
        };
        let token_provider: Arc<dyn TokenProvider> = match credentials_path {
            Some(path) => Arc::new(gcp_auth::CustomServiceAccount::from_file(&path)?),
            None => gcp_auth::provider().await?,
        };
        let tls_config = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(include_bytes!("../proto/google.pem")))
            .domain_name("bigtable.googleapis.com");
        let mut endpoint = Channel::from_static("https://bigtable.googleapis.com")
            .http2_keep_alive_interval(Duration::from_secs(30))
            .keep_alive_timeout(Duration::from_secs(10))
            .keep_alive_while_idle(true)
            .tls_config(tls_config)?;
        endpoint = endpoint.timeout(timeout.unwrap_or(DEFAULT_CHANNEL_TIMEOUT));
        let project_id = match project_id {
            Some(p) => p,
            None => token_provider.project_id().await?.to_string(),
        };
        let instance_name = format!("projects/{}/instances/{}", project_id, instance_id);
        let table_prefix = format!("{}/tables/", instance_name);
        let primer = BigtablePrimer {
            instance_name,
            policy: policy.to_string(),
            token_provider: Some(token_provider.clone()),
        };
        let pool =
            ChannelPool::new_connected(endpoint, config, Some(Box::new(primer)), registry).await?;
        let auth_channel = AuthChannel::new(pool, policy.to_string(), Some(token_provider));
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

    /// Fetch transactions with an optional column filter for partial reads.
    /// When `columns` is None, all columns are fetched. When Some, only the
    /// specified column qualifiers are fetched (e.g. `&["td", "ef", "ts", "cn"]`).
    pub async fn get_transactions_filtered(
        &mut self,
        transactions: &[TransactionDigest],
        columns: Option<&[&str]>,
    ) -> Result<Vec<TransactionData>> {
        let keys = transactions
            .iter()
            .map(tables::transactions::encode_key)
            .collect();
        let filter = columns.map(|cols| {
            let pattern = format!("^({})$", cols.join("|"));
            RowFilter {
                filter: Some(Filter::ColumnQualifierRegexFilter(pattern.into())),
            }
        });
        let mut result = vec![];
        for (key, row) in self
            .multi_get(tables::transactions::NAME, keys, filter)
            .await?
        {
            let digest = TransactionDigest::from(
                <[u8; 32]>::try_from(key.as_ref())
                    .context("invalid transaction digest key length")?,
            );
            result.push(tables::transactions::decode(digest, &row)?);
        }
        Ok(result)
    }

    /// Fetch epochs with an optional column filter for partial reads.
    /// When `columns` is None, all columns are fetched. When Some, only the
    /// specified column qualifiers are fetched (e.g. `&["ep", "sc", "pv"]`).
    pub async fn get_epochs_filtered(
        &mut self,
        epoch_ids: &[EpochId],
        columns: Option<&[&str]>,
    ) -> Result<Vec<EpochData>> {
        let keys = epoch_ids
            .iter()
            .map(|id| tables::epochs::encode_key(*id))
            .collect();
        let filter = columns.map(|cols| {
            let pattern = format!("^({})$", cols.join("|"));
            RowFilter {
                filter: Some(Filter::ColumnQualifierRegexFilter(pattern.into())),
            }
        });
        let mut result = vec![];
        for (_, row) in self.multi_get(tables::epochs::NAME, keys, filter).await? {
            result.push(tables::epochs::decode(&row)?);
        }
        Ok(result)
    }

    /// Fetch the latest epoch with an optional column filter for partial reads.
    pub async fn get_latest_epoch_filtered(
        &mut self,
        columns: Option<&[&str]>,
    ) -> Result<Option<EpochData>> {
        let upper_limit = tables::epochs::encode_key_upper_bound();
        let filter = columns.map(|cols| {
            let pattern = format!("^({})$", cols.join("|"));
            RowFilter {
                filter: Some(Filter::ColumnQualifierRegexFilter(pattern.into())),
            }
        });
        match self
            .range_scan(
                tables::epochs::NAME,
                None,
                Some(upper_limit),
                1,
                true,
                filter,
            )
            .await?
            .pop()
        {
            Some((_, row)) => Ok(Some(tables::epochs::decode(&row)?)),
            None => Ok(None),
        }
    }

    /// Fetch checkpoints with an optional column filter for partial reads.
    /// When `columns` is None, all columns are fetched. When Some, only the
    /// specified column qualifiers are fetched (e.g. `&["s", "sg"]`).
    pub async fn get_checkpoints_filtered(
        &mut self,
        sequence_numbers: &[CheckpointSequenceNumber],
        columns: Option<&[&str]>,
    ) -> Result<Vec<CheckpointData>> {
        let keys = sequence_numbers
            .iter()
            .copied()
            .map(tables::checkpoints::encode_key)
            .collect();
        let filter = columns.map(|cols| {
            let pattern = format!("^({})$", cols.join("|"));
            RowFilter {
                filter: Some(Filter::ColumnQualifierRegexFilter(pattern.into())),
            }
        });
        let mut checkpoints = vec![];
        for (_, row) in self
            .multi_get(tables::checkpoints::NAME, keys, filter)
            .await?
        {
            checkpoints.push(tables::checkpoints::decode(&row)?);
        }
        Ok(checkpoints)
    }

    /// Fetch a checkpoint by digest with an optional column filter.
    pub async fn get_checkpoint_by_digest_filtered(
        &mut self,
        digest: CheckpointDigest,
        columns: Option<&[&str]>,
    ) -> Result<Option<CheckpointData>> {
        let key = tables::checkpoints_by_digest::encode_key(&digest);
        let mut response = self
            .multi_get(tables::checkpoints_by_digest::NAME, vec![key], None)
            .await?;
        if let Some((_, row)) = response.pop() {
            let sequence_number = tables::checkpoints_by_digest::decode(&row)?;
            if let Some(chk) = self
                .get_checkpoints_filtered(&[sequence_number], columns)
                .await?
                .pop()
            {
                return Ok(Some(chk));
            }
        }
        Ok(None)
    }

    /// Get the pipeline watermark from the watermarks table.
    pub async fn get_pipeline_watermark(&mut self, pipeline: &str) -> Result<Option<Watermark>> {
        let pipeline_key = tables::watermarks::encode_key(pipeline);

        let rows = self
            .multi_get(tables::watermarks::NAME, vec![pipeline_key.clone()], None)
            .await?;

        for (key, row) in rows {
            if key.as_ref() == pipeline_key.as_slice() {
                return Ok(Some(tables::watermarks::decode(&row)?));
            }
        }

        Ok(None)
    }

    /// Set the pipeline watermark in the watermarks table. Bitmap-index
    /// pipelines pass `Some(bucket_start_cp)` to persist the bucket-start
    /// tracking column alongside the watermark in a single mutation; all
    /// other pipelines pass `None`.
    pub async fn set_pipeline_watermark(
        &mut self,
        pipeline: &str,
        watermark: &Watermark,
        bucket_start_cp: Option<u64>,
    ) -> Result<()> {
        let entry = tables::make_entry(
            tables::watermarks::encode_key(pipeline),
            tables::watermarks::encode(watermark, bucket_start_cp)?,
            Some(watermark.timestamp_ms_hi_inclusive),
        );
        self.write_entries(tables::watermarks::NAME, [entry]).await
    }

    /// Read the `bucket_start_cp` column for a bitmap-index pipeline, if
    /// present. Returns `None` for non-bitmap pipelines and for bitmap
    /// pipelines that haven't yet written the column.
    pub async fn get_bitmap_bucket_start_cp(&mut self, pipeline: &str) -> Result<Option<u64>> {
        let pipeline_key = tables::watermarks::encode_key(pipeline);

        let rows = self
            .multi_get(tables::watermarks::NAME, vec![pipeline_key.clone()], None)
            .await?;

        for (key, row) in rows {
            if key.as_ref() == pipeline_key.as_slice() {
                return tables::watermarks::decode_bucket_start_cp(&row);
            }
        }

        Ok(None)
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

    /// Atomically check a predicate on a row and apply mutations conditionally.
    ///
    /// Returns `true` if the predicate matched (and `true_mutations` were applied),
    /// or `false` if it did not match (and `false_mutations` were applied).
    pub async fn check_and_mutate_row(
        &mut self,
        table: &str,
        row_key: Bytes,
        predicate_filter: Option<RowFilter>,
        true_mutations: Vec<Mutation>,
        false_mutations: Vec<Mutation>,
    ) -> Result<bool> {
        let mut request = CheckAndMutateRowRequest {
            table_name: format!("{}{}", self.table_prefix, table),
            row_key,
            predicate_filter,
            true_mutations,
            false_mutations,
            ..CheckAndMutateRowRequest::default()
        };
        if let Some(ref app_profile_id) = self.app_profile_id {
            request.app_profile_id = app_profile_id.clone();
        }
        let response = self.client.clone().check_and_mutate_row(request).await?;
        Ok(response.into_inner().predicate_matched)
    }

    /// Generate a raw object key from ObjectKey.
    pub fn raw_object_key(object_key: &ObjectKey) -> Vec<u8> {
        tables::objects::encode_key(object_key)
    }

    pub async fn read_rows(
        &mut self,
        request: ReadRowsRequest,
        table_name: &str,
    ) -> Result<Vec<(Bytes, Vec<(Bytes, Bytes)>)>> {
        use futures::StreamExt;
        let stream = self.read_rows_stream(request, table_name).await?;
        futures::pin_mut!(stream);
        let mut result = vec![];
        while let Some(row) = stream.next().await {
            result.push(row?);
        }
        Ok(result)
    }

    /// Streaming variant of `read_rows`. Returns rows as they arrive from the
    /// underlying gRPC stream rather than collecting into a Vec.
    pub async fn read_rows_stream(
        &mut self,
        mut request: ReadRowsRequest,
        table_name: &str,
    ) -> Result<impl futures::Stream<Item = Result<(Bytes, Vec<(Bytes, Bytes)>)>> + use<>> {
        if let Some(ref app_profile_id) = self.app_profile_id {
            request.app_profile_id = app_profile_id.clone();
        }
        let response = self.client.clone().read_rows(request).await?.into_inner();
        let metrics = self.metrics.clone();
        let client_name = self.client_name.clone();
        let table_name = table_name.to_owned();

        Ok(async_stream::try_stream! {
            // Zero-copy accumulator for cell values. BigTable streams cell data
            // in chunks, and prost deserializes each chunk.value as a Bytes view
            // into the gRPC buffer (no allocation).
            //
            // - Single chunk (common): stays as Bytes, no copies at all
            // - Multiple chunks (only for values >1MB): copies into Vec<u8>
            enum CellValue {
                Empty,
                Single(Bytes),
                Multi(Vec<u8>),
            }

            impl CellValue {
                fn extend(&mut self, data: Bytes) {
                    let prev = std::mem::replace(self, CellValue::Empty);
                    *self = match prev {
                        CellValue::Empty => CellValue::Single(data),
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
                        CellValue::Single(b) => b,
                        CellValue::Multi(v) => Bytes::from(v),
                    }
                }
            }

            let mut response = response;
            let mut row_key: Option<Bytes> = None;
            let mut row = vec![];
            let mut cell_value = CellValue::Empty;
            let mut cell_name: Option<Bytes> = None;
            let mut timestamp = 0i64;

            while let Some(message) = response.message().await? {
                if let Some(ref metrics) = metrics {
                    report_bt_stats_inner(metrics, &client_name, &table_name, &message.request_stats);
                }
                for chunk in message.chunks.into_iter() {
                    if !chunk.row_key.is_empty() {
                        row_key = Some(chunk.row_key);
                    }
                    match chunk.qualifier {
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
                                yield (key, std::mem::take(&mut row));
                            }
                        }
                        row_key = None;
                        row = vec![];
                        cell_value = CellValue::Empty;
                        cell_name = None;
                    }
                }
            }
        })
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

    fn build_multi_get_request(
        &self,
        table_name: &str,
        keys: Vec<Vec<u8>>,
        filter: Option<RowFilter>,
    ) -> ReadRowsRequest {
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
        ReadRowsRequest {
            table_name: format!("{}{}", self.table_prefix, table_name),
            rows_limit: keys.len() as i64,
            rows: Some(RowSet {
                row_keys: keys.into_iter().map(Bytes::from).collect(),
                row_ranges: vec![],
            }),
            filter,
            request_stats_view: 2,
            ..ReadRowsRequest::default()
        }
    }

    async fn multi_get_internal(
        &mut self,
        table_name: &str,
        keys: Vec<Vec<u8>>,
        filter: Option<RowFilter>,
    ) -> Result<Vec<(Bytes, Vec<(Bytes, Bytes)>)>> {
        let request = self.build_multi_get_request(table_name, keys, filter);
        self.read_rows(request, table_name).await
    }

    /// Build a `RowFilter` restricting the read to the given column qualifiers.
    /// Intended for callers of streaming APIs that need to construct the filter
    /// ahead of time (so the returned stream doesn't borrow caller scope).
    pub fn column_filter(columns: &[&str]) -> RowFilter {
        let pattern = format!("^({})$", columns.join("|"));
        RowFilter {
            filter: Some(Filter::ColumnQualifierRegexFilter(pattern.into())),
        }
    }

    /// Streaming variant of `multi_get`. Rows arrive on the stream as soon as
    /// BigTable writes them on the wire, so downstream stages in a pipeline
    /// can start work before the full batch completes. Emits rows in arrival
    /// order, which is not necessarily key order — callers that need stable
    /// ordering should sort at the end.
    pub async fn multi_get_stream(
        &mut self,
        table_name: &str,
        keys: Vec<Vec<u8>>,
        filter: Option<RowFilter>,
    ) -> Result<futures::stream::BoxStream<'static, Result<(Bytes, Vec<(Bytes, Bytes)>)>>> {
        use futures::StreamExt;
        let request = self.build_multi_get_request(table_name, keys, filter);
        let stream = self.read_rows_stream(request, table_name).await?;
        Ok(stream.boxed())
    }

    /// Scan a range of rows with optional start/end keys, limit, and direction.
    /// Applies `CellsPerColumnLimitFilter(1)` like `multi_get_internal`.
    /// An optional column filter can be provided to restrict which columns are fetched.
    pub(crate) async fn range_scan(
        &mut self,
        table_name: &str,
        start_key: Option<Bytes>,
        end_key: Option<Bytes>,
        limit: i64,
        reversed: bool,
        filter: Option<RowFilter>,
    ) -> Result<Vec<(Bytes, Vec<(Bytes, Bytes)>)>> {
        let start_time = Instant::now();
        let result = self
            .range_scan_internal(table_name, start_key, end_key, limit, reversed, filter)
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
        filter: Option<RowFilter>,
    ) -> Result<Vec<(Bytes, Vec<(Bytes, Bytes)>)>> {
        let range = RowRange {
            start_key: start_key.map(StartKey::StartKeyClosed),
            end_key: end_key.map(EndKey::EndKeyClosed),
        };
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

    /// Streaming variant of `range_scan`. Returns rows as they arrive from the
    /// underlying gRPC stream.
    async fn range_scan_stream(
        &mut self,
        table_name: &str,
        start_key: Option<Bytes>,
        end_key: Option<Bytes>,
        limit: i64,
        reversed: bool,
        filter: Option<RowFilter>,
    ) -> Result<impl futures::Stream<Item = Result<(Bytes, Vec<(Bytes, Bytes)>)>> + use<>> {
        let range = RowRange {
            start_key: start_key.map(StartKey::StartKeyClosed),
            end_key: end_key.map(EndKey::EndKeyClosed),
        };
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
        self.read_rows_stream(request, table_name).await
    }

    /// Resolve tx_sequence_numbers to `(TransactionDigest, checkpoint_seq, event_count)`
    /// via a single `multi_get` on the `tx_seq_digest` table.
    ///
    /// Returns a vector parallel to the input: `None` for any tx_seq that has
    /// no row (e.g. not yet indexed).
    pub async fn resolve_tx_digests(
        &mut self,
        tx_sequence_numbers: &[u64],
    ) -> Result<Vec<Option<(TransactionDigest, u64, u32)>>> {
        use crate::tables::tx_seq_digest;

        if tx_sequence_numbers.is_empty() {
            return Ok(Vec::new());
        }

        let keys: Vec<Vec<u8>> = tx_sequence_numbers
            .iter()
            .map(|s| tx_seq_digest::encode_key(*s))
            .collect();

        let rows = self.multi_get(tx_seq_digest::NAME, keys, None).await?;

        let mut by_seq: std::collections::HashMap<u64, (TransactionDigest, u64, u32)> =
            std::collections::HashMap::with_capacity(rows.len());
        for (row_key, cells) in &rows {
            let tx_seq = tx_seq_digest::decode_key(row_key.as_ref())?;
            let (digest, cp_seq, event_count) = tx_seq_digest::decode(cells)?;
            by_seq.insert(tx_seq, (digest, cp_seq, event_count));
        }

        Ok(tx_sequence_numbers
            .iter()
            .map(|s| by_seq.get(s).copied())
            .collect())
    }

    /// Streaming variant of `resolve_tx_digests`. Emits each resolved row as
    /// it arrives from BigTable. Rows missing from the table are silently
    /// dropped (same contract as the non-streaming version, minus the
    /// position-preserving `Option` wrapping).
    pub async fn resolve_tx_digests_stream(
        &mut self,
        tx_sequence_numbers: Vec<u64>,
    ) -> Result<impl futures::Stream<Item = Result<(u64, TransactionDigest, u64, u32)>> + use<>>
    {
        use crate::tables::tx_seq_digest;

        let keys: Vec<Vec<u8>> = tx_sequence_numbers
            .into_iter()
            .map(tx_seq_digest::encode_key)
            .collect();

        let rows = self
            .multi_get_stream(tx_seq_digest::NAME, keys, None)
            .await?;

        Ok(async_stream::try_stream! {
            use futures::StreamExt;
            futures::pin_mut!(rows);
            while let Some(row) = rows.next().await {
                let (row_key, cells) = row?;
                let tx_seq = tx_seq_digest::decode_key(row_key.as_ref())?;
                let (digest, cp_seq, event_count) = tx_seq_digest::decode(&cells)?;
                yield (tx_seq, digest, cp_seq, event_count);
            }
        })
    }

    /// Streaming variant of `get_transactions_filtered`. Yields
    /// `(TransactionDigest, TransactionData)` per row as it arrives.
    /// Takes an owned `column_filter` so the returned stream does not borrow
    /// from caller-scoped values (avoids lifetime capture in `impl Stream`).
    pub async fn get_transactions_stream(
        &mut self,
        digests: Vec<TransactionDigest>,
        column_filter: Option<RowFilter>,
    ) -> Result<impl futures::Stream<Item = Result<(TransactionDigest, TransactionData)>> + use<>>
    {
        let keys = digests
            .iter()
            .map(tables::transactions::encode_key)
            .collect();
        let filter = column_filter;
        let rows = self
            .multi_get_stream(tables::transactions::NAME, keys, filter)
            .await?;

        Ok(async_stream::try_stream! {
            use futures::StreamExt;
            futures::pin_mut!(rows);
            while let Some(row) = rows.next().await {
                let (key, cells) = row?;
                let digest = TransactionDigest::from(
                    <[u8; 32]>::try_from(key.as_ref())
                        .context("invalid transaction digest key length")?,
                );
                let tx = tables::transactions::decode(digest, &cells)?;
                yield (digest, tx);
            }
        })
    }

    /// Streaming variant of `get_objects`. Yields each `Object` as it arrives.
    pub async fn get_objects_stream(
        &mut self,
        object_keys: Vec<ObjectKey>,
    ) -> Result<impl futures::Stream<Item = Result<Object>> + use<>> {
        let keys: Vec<Vec<u8>> = object_keys.iter().map(Self::raw_object_key).collect();
        let rows = self
            .multi_get_stream(tables::objects::NAME, keys, None)
            .await?;

        Ok(async_stream::try_stream! {
            use futures::StreamExt;
            futures::pin_mut!(rows);
            while let Some(row) = rows.next().await {
                let (_key, cells) = row?;
                yield tables::objects::decode(&cells)?;
            }
        })
    }

    /// Range-scan `tx_seq_digest` across `tx_range` (half-open) and yield
    /// each row's `(tx_seq, digest, cp_seq, event_count)` in strictly
    /// ascending tx_seq order.
    ///
    /// Because the row key is salt-prefixed (see
    /// `tables::tx_seq_digest::encode_key`), a single range scan would only
    /// cover one of the `SALT_COUNT` shards. We fan out one `range_scan_stream`
    /// per salt bucket, interleave arrivals with `select_all`, and yield as
    /// soon as the expected next tx_seq is available. This relies on the
    /// density invariant that every tx_seq in range has exactly one row
    /// (the indexer writes for every tx in every checkpoint), so the next
    /// row to yield is always `prev + 1`. Rows from fast shards buffer in a
    /// min-heap while we wait for the slow shard that owns the current
    /// expected tx_seq. Dropping the returned stream cancels all underlying
    /// scans.
    pub async fn scan_tx_seq_digest_stream(
        &mut self,
        tx_range: Range<u64>,
    ) -> Result<impl futures::Stream<Item = Result<(u64, TransactionDigest, u64, u32)>>> {
        use crate::tables::tx_seq_digest;
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;

        // range_scan_stream uses EndKeyClosed, so use an inclusive upper bound
        // by shifting the exclusive `tx_range.end` down by 1. Within a bucket,
        // keys stay ordered by tx_seq; only keys written satisfy
        // `salt == tx_seq % SALT_COUNT`, so each shard's range yields exactly
        // the rows for its salt in ascending tx_seq order.
        let inclusive_end = tx_range.end - 1;
        let mut streams = Vec::with_capacity(tx_seq_digest::SALT_COUNT as usize);
        for salt in 0..tx_seq_digest::SALT_COUNT {
            let mut start = Vec::with_capacity(9);
            start.push(salt as u8);
            start.extend_from_slice(&tx_range.start.to_be_bytes());
            let mut end = Vec::with_capacity(9);
            end.push(salt as u8);
            end.extend_from_slice(&inclusive_end.to_be_bytes());

            let s = self
                .range_scan_stream(
                    tx_seq_digest::NAME,
                    Some(Bytes::from(start)),
                    Some(Bytes::from(end)),
                    // i64::MAX — stream stops when caller drops or range ends.
                    i64::MAX,
                    false,
                    None,
                )
                .await?;
            streams.push(Box::pin(s));
        }
        let merged = futures::stream::select_all(streams);
        let start = tx_range.start;

        Ok(async_stream::try_stream! {
            use futures::StreamExt;

            let mut heap: BinaryHeap<Reverse<(u64, TransactionDigest, u64, u32)>> =
                BinaryHeap::new();
            let mut next_expected = start;

            futures::pin_mut!(merged);
            while let Some(row) = merged.next().await {
                let (key, cells) = row?;
                let tx_seq = tx_seq_digest::decode_key(key.as_ref())?;
                let (digest, cp_seq, event_count) = tx_seq_digest::decode(&cells)?;
                heap.push(Reverse((tx_seq, digest, cp_seq, event_count)));

                while heap.peek().is_some_and(|Reverse((ts, ..))| *ts == next_expected) {
                    let Reverse((ts, d, cp, ec)) = heap.pop().unwrap();
                    yield (ts, d, cp, ec);
                    next_expected += 1;
                }
            }

            // Tail drain: if the table has any hole in the range (shouldn't
            // happen under the density invariant, but don't stall forever if
            // it does), flush whatever's left in ascending order.
            while let Some(Reverse((ts, d, cp, ec))) = heap.pop() {
                yield (ts, d, cp, ec);
            }
        })
    }

    /// Resolve inclusive checkpoint bounds to a tx_sequence_number range.
    ///
    /// Returns `[start_tx, end_tx)` where `start_tx` is the first tx in
    /// `start_checkpoint` and `end_tx` is one past the last tx in `end_checkpoint`.
    /// Reads checkpoint summaries from the checkpoints table.
    pub async fn checkpoint_to_tx_range(
        &mut self,
        checkpoint_range: Range<u64>,
    ) -> Result<Range<u64>> {
        use crate::tables::checkpoints;

        let start_checkpoint = checkpoint_range.start;
        let end_checkpoint = checkpoint_range.end.saturating_sub(1);

        let start_tx = if start_checkpoint == 0 {
            0u64
        } else {
            let prev = self
                .get_checkpoints_filtered(
                    &[start_checkpoint - 1],
                    Some(&[checkpoints::col::SUMMARY]),
                )
                .await?;
            let summary = prev
                .first()
                .and_then(|cp| cp.summary.as_ref())
                .context("checkpoint summary not found for start bound")?;
            summary.network_total_transactions
        };

        let end_cps = self
            .get_checkpoints_filtered(&[end_checkpoint], Some(&[checkpoints::col::SUMMARY]))
            .await?;
        let end_summary = end_cps
            .first()
            .and_then(|cp| cp.summary.as_ref())
            .context("checkpoint summary not found for end bound")?;
        let end_tx = end_summary.network_total_transactions;

        Ok(start_tx..end_tx)
    }

    /// Resolve `tx_sequence_number`s to fully-populated transaction rows in
    /// two multi_gets: one against `tx_seq_digest` for the digest+cp_seq, one
    /// against `transactions` for the row bodies. Tx_seqs that don't resolve
    /// (not yet indexed) or whose row is missing are silently dropped.
    ///
    /// Output ordering is not tx_seq order — callers that need a sorted page
    /// should sort by `tx_seq` at the end. Callers should pass a bounded
    /// slice (typically `page_size + 1`) so the multi_gets stay well-sized.
    pub async fn get_transactions_for_seqs(
        &mut self,
        seqs: Vec<u64>,
        columns: Option<&[&str]>,
    ) -> Result<Vec<(u64, u64, crate::TransactionData)>> {
        if seqs.is_empty() {
            return Ok(Vec::new());
        }
        let resolved = self.resolve_tx_digests(&seqs).await?;
        let mut pairs: Vec<(u64, TransactionDigest, u64)> = Vec::with_capacity(resolved.len());
        let mut digests: Vec<TransactionDigest> = Vec::with_capacity(resolved.len());
        for (seq, r) in seqs.iter().zip_debug_eq(resolved) {
            if let Some((digest, cp_seq, _)) = r {
                pairs.push((*seq, digest, cp_seq));
                digests.push(digest);
            }
        }
        if digests.is_empty() {
            return Ok(Vec::new());
        }
        let tx_data_vec = self.get_transactions_filtered(&digests, columns).await?;
        let mut by_digest: std::collections::HashMap<TransactionDigest, crate::TransactionData> =
            tx_data_vec.into_iter().map(|tx| (tx.digest, tx)).collect();
        Ok(pairs
            .into_iter()
            .filter_map(|(seq, digest, cp_seq)| {
                by_digest.remove(&digest).map(|tx| (seq, cp_seq, tx))
            })
            .collect())
    }
}

fn report_bt_stats_inner(
    metrics: &KvMetrics,
    client_name: &str,
    table_name: &str,
    request_stats: &Option<RequestStats>,
) {
    let labels = [client_name, table_name];
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
        self.get_transactions_filtered(transactions, None).await
    }

    async fn get_checkpoints(
        &mut self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> Result<Vec<CheckpointData>> {
        self.get_checkpoints_filtered(sequence_numbers, None).await
    }

    async fn get_checkpoint_by_digest(
        &mut self,
        digest: CheckpointDigest,
    ) -> Result<Option<CheckpointData>> {
        self.get_checkpoint_by_digest_filtered(digest, None).await
    }

    async fn get_watermark_for_pipelines(
        &mut self,
        pipelines: &[&str],
    ) -> Result<Option<Watermark>> {
        let keys: Vec<Vec<u8>> = pipelines
            .iter()
            .map(|name| tables::watermarks::encode_key(name))
            .collect();

        let rows = self.multi_get(tables::watermarks::NAME, keys, None).await?;

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
            .range_scan(
                tables::objects::NAME,
                None,
                Some(upper_limit),
                1,
                true,
                None,
            )
            .await?
            .pop()
        {
            return Ok(Some(tables::objects::decode(&row)?));
        }
        Ok(None)
    }

    async fn get_epoch(&mut self, epoch_id: EpochId) -> Result<Option<EpochData>> {
        Ok(self.get_epochs_filtered(&[epoch_id], None).await?.pop())
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
        self.get_latest_epoch_filtered(None).await
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
                None,
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
                None,
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
                None,
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
                None,
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
