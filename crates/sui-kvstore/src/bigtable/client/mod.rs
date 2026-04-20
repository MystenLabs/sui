// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod auth_channel;
mod channel_pool;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context as _;
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use gcp_auth::TokenProvider;
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
use crate::WatermarkV0;
use crate::WatermarkV1;
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
use crate::bigtable::proto::bigtable::v2::ValueRange;
use crate::bigtable::proto::bigtable::v2::bigtable_client::BigtableClient as BigtableInternalClient;
use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::bigtable::proto::bigtable::v2::mutation;
use crate::bigtable::proto::bigtable::v2::mutation::SetCell;
use crate::bigtable::proto::bigtable::v2::read_rows_response::cell_chunk::RowStatus;
use crate::bigtable::proto::bigtable::v2::request_stats::StatsView;
use crate::bigtable::proto::bigtable::v2::row_filter::Chain;
use crate::bigtable::proto::bigtable::v2::row_filter::Filter;
use crate::bigtable::proto::bigtable::v2::row_range::EndKey;
use crate::bigtable::proto::bigtable::v2::row_range::StartKey;
use crate::bigtable::proto::bigtable::v2::value_range;
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

    /// Read the raw cells of a pipeline's watermark row. Returns an empty vec when the row
    /// does not exist. Callers decode whichever schema variant they need (e.g.
    /// [`tables::watermarks::decode_v1`] / [`tables::watermarks::decode_v0`]).
    pub async fn get_pipeline_watermark_rows(
        &mut self,
        pipeline: &str,
    ) -> Result<Vec<(Bytes, Bytes)>> {
        let pipeline_key = tables::watermarks::encode_key(pipeline);
        let rows = self
            .multi_get(tables::watermarks::NAME, vec![pipeline_key.clone()], None)
            .await?;
        for (key, row) in rows {
            if key.as_ref() == pipeline_key.as_slice() {
                return Ok(row);
            }
        }
        Ok(Vec::new())
    }

    /// CAS write to a watermarks row: write `cells` only when the existing `column` cell is
    /// absent or has a u64 BE value strictly less than `new_value`. Returns `true` iff the
    /// write happened (i.e. the new value strictly increased the guarded column). Cells not
    /// included here retain their existing values in BigTable.
    pub async fn cas_write_pipeline_watermark_cells(
        &mut self,
        pipeline: &str,
        column: &'static str,
        new_value: u64,
        cells: Vec<(&'static str, Bytes)>,
    ) -> Result<bool> {
        let mutations = build_set_cell_mutations(cells);
        let predicate = column_value_at_least_filter(column, u64_be(new_value));
        // Predicate is "guarded column has a value >= new" → predicate_matched = true means
        // the existing value blocks the write, so false_mutations are what we want to run.
        let predicate_matched = self
            .check_and_mutate_row(
                tables::watermarks::NAME,
                tables::watermarks::encode_key(pipeline),
                Some(predicate),
                Vec::new(),
                mutations,
            )
            .await?;
        Ok(!predicate_matched)
    }

    /// Returns `true` iff the supplied `chain_id` matches the chain_id stored for `pipeline`.
    /// On the first call (no chain_id cell yet) writes `chain_id` and returns `true`. The
    /// chain_id cell is independent of the v1 watermark cells, so this can be invoked before
    /// `init_watermark`.
    pub async fn accepts_chain_id(&mut self, pipeline: &str, chain_id: [u8; 32]) -> Result<bool> {
        use tables::watermarks::col;
        let mutations =
            build_set_cell_mutations([(col::CHAIN_ID, Bytes::copy_from_slice(&chain_id))]);
        let predicate = column_exists_filter(col::CHAIN_ID);
        // Predicate is "row already has a chain_id" → false_mutations write the new chain_id
        // when nothing is stored yet.
        let predicate_matched = self
            .check_and_mutate_row(
                tables::watermarks::NAME,
                tables::watermarks::encode_key(pipeline),
                Some(predicate),
                Vec::new(),
                mutations,
            )
            .await?;
        if !predicate_matched {
            return Ok(true);
        }
        let row = self.get_pipeline_watermark_rows(pipeline).await?;
        let cell = row
            .iter()
            .find_map(|(c, v)| (c.as_ref() == col::CHAIN_ID.as_bytes()).then_some(v))
            .context("chain_id missing after CAS reported it present")?;
        let stored: [u8; 32] = cell.as_ref().try_into().map_err(|_| {
            anyhow::anyhow!(
                "`{}` column has unexpected length {} (expected 32)",
                col::CHAIN_ID,
                cell.len()
            )
        })?;
        Ok(stored == chain_id)
    }

    /// Create the row for a pipeline iff no schema-version cell exists yet. Used by
    /// `init_watermark` for fresh rows and the v0 → v1 bootstrap. Returns `true` iff the
    /// write happened.
    pub async fn create_pipeline_watermark_if_absent(
        &mut self,
        pipeline: &str,
        new: &WatermarkV1,
    ) -> Result<bool> {
        use tables::watermarks::col;
        let mut cells = vec![
            (col::SCHEMA_VERSION, u64_be(tables::watermarks::SCHEMA_V1)),
            (col::EPOCH_HI, u64_be(new.epoch_hi_inclusive)),
            (col::TX_HI, u64_be(new.tx_hi)),
            (col::TIMESTAMP_MS_HI, u64_be(new.timestamp_ms_hi_inclusive)),
            (col::READER_LO, u64_be(new.reader_lo)),
            (col::PRUNER_HI, u64_be(new.pruner_hi)),
            (col::PRUNER_TIMESTAMP_MS, u64_be(new.pruner_timestamp_ms)),
        ];
        if let Some(checkpoint) = new.checkpoint_hi_inclusive {
            cells.push((col::CHECKPOINT_HI, u64_be(checkpoint)));
            let v0 = WatermarkV0 {
                epoch_hi_inclusive: new.epoch_hi_inclusive,
                checkpoint_hi_inclusive: checkpoint,
                tx_hi: new.tx_hi,
                timestamp_ms_hi_inclusive: new.timestamp_ms_hi_inclusive,
            };
            cells.push((col::WATERMARK_V0, Bytes::from(bcs::to_bytes(&v0)?)));
        }
        let mutations = build_set_cell_mutations(cells);
        let predicate = column_exists_filter(tables::watermarks::col::SCHEMA_VERSION);
        // Predicate is "row has any schema-version cell" → false_mutations write the new row.
        let predicate_matched = self
            .check_and_mutate_row(
                tables::watermarks::NAME,
                tables::watermarks::encode_key(pipeline),
                Some(predicate),
                Vec::new(),
                mutations,
            )
            .await?;
        Ok(!predicate_matched)
    }

    /// Issue a `CheckAndMutateRow` request and return whether the predicate matched.
    async fn check_and_mutate_row(
        &mut self,
        table: &str,
        row_key: Vec<u8>,
        predicate_filter: Option<RowFilter>,
        true_mutations: Vec<Mutation>,
        false_mutations: Vec<Mutation>,
    ) -> Result<bool> {
        let mut request = CheckAndMutateRowRequest {
            table_name: format!("{}{}", self.table_prefix, table),
            row_key: row_key.into(),
            predicate_filter,
            true_mutations,
            false_mutations,
            ..CheckAndMutateRowRequest::default()
        };
        if let Some(ref app_profile_id) = self.app_profile_id {
            request.app_profile_id = app_profile_id.clone();
        }
        let response = self
            .client
            .clone()
            .check_and_mutate_row(request)
            .await?
            .into_inner();
        Ok(response.predicate_matched)
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
    ) -> Result<Option<WatermarkV1>> {
        let keys: Vec<Vec<u8>> = pipelines
            .iter()
            .map(|name| tables::watermarks::encode_key(name))
            .collect();

        let rows = self.multi_get(tables::watermarks::NAME, keys, None).await?;

        if rows.len() != pipelines.len() {
            return Ok(None);
        }

        // A row is hidden if `checkpoint_hi_inclusive == None` or
        // `checkpoint_hi_inclusive < reader_lo`. If any pipeline is hidden, every consumer
        // of this method (RPC/graphql) treats the whole result as missing, resulting in `Ok(None)`.
        let mut min_wm: Option<(u64, WatermarkV1)> = None;
        for (_, row) in &rows {
            let Some(wm) = tables::watermarks::decode_v1(row)? else {
                return Ok(None);
            };
            let Some(cp) = wm.checkpoint_hi_inclusive.filter(|cp| *cp >= wm.reader_lo) else {
                return Ok(None);
            };
            if min_wm.as_ref().is_none_or(|(prev, _)| cp < *prev) {
                min_wm = Some((cp, wm));
            }
        }

        Ok(min_wm.map(|(_, wm)| wm))
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

fn u64_be(v: u64) -> Bytes {
    Bytes::copy_from_slice(&v.to_be_bytes())
}

/// Build `Mutation::SetCell` entries for the given `(column, value)` cells, all in the `sui`
/// column family with server-assigned timestamps.
fn build_set_cell_mutations(
    cells: impl IntoIterator<Item = (&'static str, Bytes)>,
) -> Vec<Mutation> {
    cells
        .into_iter()
        .map(|(col, val)| Mutation {
            mutation: Some(mutation::Mutation::SetCell(SetCell {
                family_name: tables::FAMILY.to_string(),
                column_qualifier: Bytes::from(col),
                timestamp_micros: -1,
                value: val,
            })),
        })
        .collect()
}

/// Build a `RowFilter` matching cells in `sui:<column>` whose value is `>= value` (interpreted
/// as raw cell bytes; callers pass the u64 BE encoding for watermark cells). Used as the
/// CAS predicate for the monotonic-increase setters: predicate matches iff the existing value
/// would block the write.
fn column_value_at_least_filter(column: &str, value: Bytes) -> RowFilter {
    RowFilter {
        filter: Some(Filter::Chain(Chain {
            filters: vec![
                RowFilter {
                    filter: Some(Filter::FamilyNameRegexFilter(tables::FAMILY.to_string())),
                },
                RowFilter {
                    filter: Some(Filter::ColumnQualifierRegexFilter(Bytes::from(format!(
                        "^{}$",
                        column
                    )))),
                },
                RowFilter {
                    filter: Some(Filter::ValueRangeFilter(ValueRange {
                        start_value: Some(value_range::StartValue::StartValueClosed(value)),
                        end_value: None,
                    })),
                },
            ],
        })),
    }
}

/// Build a `RowFilter` matching any cell in the `sui:<column>` column. Used as a CAS predicate
/// for "create-if-absent" paths: if the predicate matches, the cell already exists.
fn column_exists_filter(column: &str) -> RowFilter {
    RowFilter {
        filter: Some(Filter::Chain(Chain {
            filters: vec![
                RowFilter {
                    filter: Some(Filter::FamilyNameRegexFilter(tables::FAMILY.to_string())),
                },
                RowFilter {
                    filter: Some(Filter::ColumnQualifierRegexFilter(Bytes::from(format!(
                        "^{}$",
                        column
                    )))),
                },
            ],
        })),
    }
}
