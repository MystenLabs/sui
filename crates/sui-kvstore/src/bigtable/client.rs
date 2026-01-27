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

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
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
use sui_types::base_types::ObjectType;
use sui_types::base_types::TransactionDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::CheckpointSummary;
use sui_types::object::Object;
use sui_types::storage::EpochInfo;
use sui_types::storage::ObjectKey;
use tonic::body::Body;
use tonic::codegen::Service;
use tonic::transport::Certificate;
use tonic::transport::Channel;
use tonic::transport::ClientTlsConfig;

use crate::Checkpoint;
use crate::KeyValueStoreReader;
use crate::PipelineWatermark;
use crate::TransactionData;
use crate::TransactionEventsData;
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
use crate::tables;

/// Error returned when a batch write partially succeeds before encountering a failure.
#[derive(Debug)]
pub struct PartialWriteError {
    pub error: anyhow::Error,
    pub succeeded_keys: Vec<Bytes>,
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
    pub async fn new_local(instance_id: String) -> Result<Self> {
        let emulator_host = std::env::var("BIGTABLE_EMULATOR_HOST")?;
        let auth_channel = AuthChannel {
            channel: Channel::from_shared(format!("http://{emulator_host}"))?.connect_lazy(),
            policy: "https://www.googleapis.com/auth/bigtable.data".to_string(),
            token_provider: None,
            token: Arc::new(RwLock::new(None)),
        };
        Ok(Self {
            table_prefix: format!("projects/emulator/instances/{}/tables/", instance_id),
            client: BigtableInternalClient::new(auth_channel),
            client_name: "local".to_string(),
            metrics: None,
            app_profile_id: None,
        })
    }

    pub async fn new_remote(
        instance_id: String,
        is_read_only: bool,
        timeout: Option<Duration>,
        client_name: String,
        registry: Option<&Registry>,
        app_profile_id: Option<String>,
    ) -> Result<Self> {
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
        if let Some(timeout) = timeout {
            endpoint = endpoint.timeout(timeout);
        }
        let table_prefix = format!(
            "projects/{}/instances/{}/tables/",
            token_provider.project_id().await?,
            instance_id
        );
        let auth_channel = AuthChannel {
            channel: endpoint.connect_lazy(),
            policy: policy.to_string(),
            token_provider: Some(token_provider),
            token: Arc::new(RwLock::new(None)),
        };
        Ok(Self {
            table_prefix,
            client: BigtableInternalClient::new(auth_channel),
            client_name,
            metrics: registry.map(KvMetrics::new),
            app_profile_id,
        })
    }

    /// Get the pipeline watermark from the watermarks table.
    pub async fn get_pipeline_watermark(
        &mut self,
        pipeline: &str,
    ) -> Result<Option<PipelineWatermark>> {
        let key = tables::watermarks::encode_key(pipeline);
        match self
            .multi_get(tables::watermarks::NAME, vec![key], None)
            .await?
            .pop()
        {
            Some((_, row)) => Ok(Some(tables::watermarks::decode(&row)?)),
            None => Ok(None),
        }
    }

    /// Set the pipeline watermark in the watermarks table.
    pub async fn set_pipeline_watermark(
        &mut self,
        pipeline: &str,
        watermark: &PipelineWatermark,
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
    /// containing the keys that succeeded. Callers can remove succeeded keys from their
    /// batch before retrying to avoid re-writing already committed data.
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
        let mut succeeded_indices: Vec<usize> = Vec::new();

        let mut request = MutateRowsRequest {
            table_name: format!("{}{}", self.table_prefix, table),
            entries,
            ..MutateRowsRequest::default()
        };
        if let Some(ref app_profile_id) = self.app_profile_id {
            request.app_profile_id = app_profile_id.clone();
        }
        let mut response = self.client.mutate_rows(request).await?.into_inner();
        while let Some(part) = response.message().await? {
            for entry in part.entries {
                if let Some(status) = entry.status {
                    if status.code == 0 {
                        succeeded_indices.push(entry.index as usize);
                    } else {
                        let succeeded_keys = succeeded_indices
                            .iter()
                            .filter_map(|&i| row_keys.get(i).cloned())
                            .collect();
                        return Err(PartialWriteError {
                            error: anyhow!(
                                "bigtable write failed {} {}",
                                status.code,
                                status.message
                            ),
                            succeeded_keys,
                        }
                        .into());
                    }
                }
            }
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
        let mut response = self.client.read_rows(request).await?.into_inner();

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

    async fn reversed_scan(
        &mut self,
        table_name: &str,
        upper_limit: Bytes,
    ) -> Result<Vec<(Bytes, Vec<(Bytes, Bytes)>)>> {
        let start_time = Instant::now();
        let result = self.reversed_scan_internal(table_name, upper_limit).await;
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

    async fn reversed_scan_internal(
        &mut self,
        table_name: &str,
        upper_limit: Bytes,
    ) -> Result<Vec<(Bytes, Vec<(Bytes, Bytes)>)>> {
        let range = RowRange {
            start_key: None,
            end_key: Some(EndKey::EndKeyClosed(upper_limit)),
        };
        let request = ReadRowsRequest {
            table_name: format!("{}{}", self.table_prefix, table_name),
            rows_limit: 1,
            rows: Some(RowSet {
                row_keys: vec![],
                row_ranges: vec![range],
            }),
            reversed: true,
            ..ReadRowsRequest::default()
        };
        self.read_rows(request, table_name).await
    }
}

impl std::fmt::Display for PartialWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "partial write: {} succeeded, error: {}",
            self.succeeded_keys.len(),
            self.error
        )
    }
}

impl std::error::Error for PartialWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.source()
    }
}

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
    ) -> Result<Vec<Checkpoint>> {
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
    ) -> Result<Option<Checkpoint>> {
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

    async fn get_latest_checkpoint(&mut self) -> Result<CheckpointSequenceNumber> {
        match self
            .multi_get(tables::watermark_alt_legacy::NAME, vec![vec![0]], None)
            .await?
            .pop()
            .and_then(|(_, mut row)| row.pop())
        {
            Some((_, value_bytes)) => Ok(u64::from_be_bytes(value_bytes.as_ref().try_into()?)),
            None => Ok(0),
        }
    }

    async fn get_latest_checkpoint_summary(&mut self) -> Result<Option<CheckpointSummary>> {
        let sequence_number = self.get_latest_checkpoint().await?;
        if sequence_number == 0 {
            return Ok(None);
        }

        // Fetch just the summary for the latest checkpoint sequence number.
        let mut response = self
            .multi_get(
                tables::checkpoints::NAME,
                vec![tables::checkpoints::encode_key(sequence_number - 1)],
                Some(RowFilter {
                    filter: Some(Filter::ColumnQualifierRegexFilter(
                        format!("^({})$", tables::checkpoints::col::SUMMARY).into(),
                    )),
                }),
            )
            .await?;

        let Some((_, row)) = response.pop() else {
            return Ok(None);
        };

        for (column, value) in row {
            if column.as_ref() == tables::checkpoints::col::SUMMARY.as_bytes() {
                return Ok(Some(bcs::from_bytes(&value)?));
            }
        }

        Ok(None)
    }

    async fn get_latest_object(&mut self, object_id: &ObjectID) -> Result<Option<Object>> {
        let upper_limit = Self::raw_object_key(&ObjectKey::max_for_id(object_id));
        if let Some((_, row)) = self
            .reversed_scan(tables::objects::NAME, upper_limit.into())
            .await?
            .pop()
        {
            return Ok(Some(tables::objects::decode(&row)?));
        }
        Ok(None)
    }

    async fn get_epoch(&mut self, epoch_id: EpochId) -> Result<Option<EpochInfo>> {
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

    async fn get_latest_epoch(&mut self) -> Result<Option<EpochInfo>> {
        let upper_limit = tables::epochs::encode_key_upper_bound();
        match self
            .reversed_scan(tables::epochs::NAME, upper_limit)
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

    async fn get_object_types(&mut self, object_ids: &[ObjectID]) -> Result<Vec<ObjectType>> {
        let keys = object_ids
            .iter()
            .map(tables::object_types::encode_key)
            .collect();
        let mut result = Vec::with_capacity(object_ids.len());
        for (_, row) in self
            .multi_get(tables::object_types::NAME, keys, None)
            .await?
        {
            result.push(tables::object_types::decode(&row)?);
        }
        Ok(result)
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
