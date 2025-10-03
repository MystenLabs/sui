// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context as _, Result};
use async_trait::async_trait;
use gcp_auth::{Token, TokenProvider};
use http::{HeaderValue, Request, Response};
use prometheus::Registry;
use sui_types::{
    base_types::{EpochId, ObjectID, TransactionDigest},
    digests::CheckpointDigest,
    effects::TransactionEvents,
    full_checkpoint_content::CheckpointData,
    messages_checkpoint::{CheckpointSequenceNumber, CheckpointSummary},
    messages_consensus::TimestampMs,
    object::Object,
    storage::{EpochInfo, ObjectKey},
};
use tonic::{
    body::BoxBody,
    codegen::Service,
    transport::{Certificate, Channel, ClientTlsConfig},
    Streaming,
};
use tracing::error;

use super::proto::bigtable::v2::{
    row_filter::{Chain, Filter},
    RowFilter,
};
use crate::bigtable::metrics::KvMetrics;
use crate::bigtable::proto::bigtable::v2::{
    bigtable_client::BigtableClient as BigtableInternalClient, mutate_rows_request::Entry,
    mutation, mutation::SetCell, read_rows_response::cell_chunk::RowStatus,
    request_stats::StatsView, row_range::EndKey, MutateRowsRequest, MutateRowsResponse, Mutation,
    ReadRowsRequest, RequestStats, RowRange, RowSet,
};
use crate::{
    Checkpoint, KeyValueStoreReader, KeyValueStoreWriter, TransactionData, TransactionEventsData,
};

const OBJECTS_TABLE: &str = "objects";
const TRANSACTIONS_TABLE: &str = "transactions";
const CHECKPOINTS_TABLE: &str = "checkpoints";
const CHECKPOINTS_BY_DIGEST_TABLE: &str = "checkpoints_by_digest";
const WATERMARK_TABLE: &str = "watermark";
const WATERMARK_ALT_TABLE: &str = "watermark_alt";
const EPOCHS_TABLE: &str = "epochs";

const COLUMN_FAMILY_NAME: &str = "sui";
const DEFAULT_COLUMN_QUALIFIER: &str = "";
const CHECKPOINT_SUMMARY_COLUMN_QUALIFIER: &str = "s";
const CHECKPOINT_SIGNATURES_COLUMN_QUALIFIER: &str = "sg";
const CHECKPOINT_CONTENTS_COLUMN_QUALIFIER: &str = "c";
const TRANSACTION_COLUMN_QUALIFIER: &str = "tx";
const EFFECTS_COLUMN_QUALIFIER: &str = "ef";
const EVENTS_COLUMN_QUALIFIER: &str = "ev";
const TIMESTAMP_COLUMN_QUALIFIER: &str = "ts";
const CHECKPOINT_NUMBER_COLUMN_QUALIFIER: &str = "cn";

type Bytes = Vec<u8>;

#[derive(Clone)]
struct AuthChannel {
    channel: Channel,
    policy: String,
    token_provider: Option<Arc<dyn TokenProvider>>,
    token: Arc<RwLock<Option<Arc<Token>>>>,
}

#[derive(Clone)]
pub struct BigTableClient {
    table_prefix: String,
    client: BigtableInternalClient<AuthChannel>,
    client_name: String,
    metrics: Option<Arc<KvMetrics>>,
    app_profile_id: Option<String>,
}

#[async_trait]
impl KeyValueStoreWriter for BigTableClient {
    async fn save_objects(&mut self, objects: &[&Object], timestamp_ms: TimestampMs) -> Result<()> {
        let mut items = Vec::with_capacity(objects.len());
        for object in objects {
            let object_key = ObjectKey(object.id(), object.version());
            items.push((
                Self::raw_object_key(&object_key)?,
                vec![(DEFAULT_COLUMN_QUALIFIER, bcs::to_bytes(object)?)],
            ));
        }
        self.multi_set(OBJECTS_TABLE, items, Some(timestamp_ms))
            .await
    }

    async fn save_transactions(&mut self, transactions: &[TransactionData]) -> Result<()> {
        let mut items = Vec::with_capacity(transactions.len());
        let mut timestamp_ms = None;
        for transaction in transactions {
            timestamp_ms = Some(transaction.timestamp);
            let cells = vec![
                (
                    TRANSACTION_COLUMN_QUALIFIER,
                    bcs::to_bytes(&transaction.transaction)?,
                ),
                (
                    EFFECTS_COLUMN_QUALIFIER,
                    bcs::to_bytes(&transaction.effects)?,
                ),
                (EVENTS_COLUMN_QUALIFIER, bcs::to_bytes(&transaction.events)?),
                (
                    TIMESTAMP_COLUMN_QUALIFIER,
                    bcs::to_bytes(&transaction.timestamp)?,
                ),
                (
                    CHECKPOINT_NUMBER_COLUMN_QUALIFIER,
                    bcs::to_bytes(&transaction.checkpoint_number)?,
                ),
            ];
            items.push((transaction.transaction.digest().inner().to_vec(), cells));
        }
        self.multi_set(TRANSACTIONS_TABLE, items, timestamp_ms)
            .await
    }

    async fn save_checkpoint(&mut self, checkpoint: &CheckpointData) -> Result<()> {
        let summary = &checkpoint.checkpoint_summary.data();
        let timestamp = summary.timestamp_ms;
        let contents = &checkpoint.checkpoint_contents;
        let signatures = &checkpoint.checkpoint_summary.auth_sig();
        let key = summary.sequence_number.to_be_bytes().to_vec();
        let cells = vec![
            (CHECKPOINT_SUMMARY_COLUMN_QUALIFIER, bcs::to_bytes(summary)?),
            (
                CHECKPOINT_SIGNATURES_COLUMN_QUALIFIER,
                bcs::to_bytes(signatures)?,
            ),
            (
                CHECKPOINT_CONTENTS_COLUMN_QUALIFIER,
                bcs::to_bytes(contents)?,
            ),
        ];
        self.multi_set(CHECKPOINTS_TABLE, [(key.clone(), cells)], Some(timestamp))
            .await?;
        self.multi_set(
            CHECKPOINTS_BY_DIGEST_TABLE,
            [(
                checkpoint.checkpoint_summary.digest().inner().to_vec(),
                vec![(DEFAULT_COLUMN_QUALIFIER, key)],
            )],
            Some(timestamp),
        )
        .await
    }

    async fn save_watermark(&mut self, watermark: CheckpointSequenceNumber) -> Result<()> {
        let watermark_bytes = watermark.to_be_bytes().to_vec();
        self.multi_set(
            WATERMARK_ALT_TABLE,
            [(
                vec![0],
                vec![(DEFAULT_COLUMN_QUALIFIER, watermark_bytes.clone())],
            )],
            Some(watermark),
        )
        .await?;
        self.multi_set(
            WATERMARK_TABLE,
            [(watermark_bytes, vec![(DEFAULT_COLUMN_QUALIFIER, vec![])])],
            None,
        )
        .await
    }

    async fn save_epoch(&mut self, epoch: EpochInfo) -> Result<()> {
        let key = epoch.epoch.to_be_bytes().to_vec();
        self.multi_set(
            EPOCHS_TABLE,
            [(
                key,
                vec![(DEFAULT_COLUMN_QUALIFIER, bcs::to_bytes(&epoch)?)],
            )],
            epoch.end_timestamp_ms.or(epoch.start_timestamp_ms),
        )
        .await
    }
}

#[async_trait]
impl KeyValueStoreReader for BigTableClient {
    async fn get_objects(&mut self, object_keys: &[ObjectKey]) -> Result<Vec<Object>> {
        let keys: Result<_, _> = object_keys.iter().map(Self::raw_object_key).collect();
        let mut objects = vec![];
        for (_, row) in self.multi_get(OBJECTS_TABLE, keys?, None).await? {
            for (_, value) in row {
                objects.push(bcs::from_bytes(&value)?);
            }
        }
        Ok(objects)
    }

    async fn get_transactions(
        &mut self,
        transactions: &[TransactionDigest],
    ) -> Result<Vec<TransactionData>> {
        let keys = transactions.iter().map(|tx| tx.inner().to_vec()).collect();
        let mut result = vec![];
        for (_, row) in self.multi_get(TRANSACTIONS_TABLE, keys, None).await? {
            let mut transaction = None;
            let mut effects = None;
            let mut events = None;
            let mut timestamp = 0;
            let mut checkpoint_number = 0;

            for (column, value) in row {
                match std::str::from_utf8(&column)? {
                    TRANSACTION_COLUMN_QUALIFIER => transaction = Some(bcs::from_bytes(&value)?),
                    EFFECTS_COLUMN_QUALIFIER => effects = Some(bcs::from_bytes(&value)?),
                    EVENTS_COLUMN_QUALIFIER => events = Some(bcs::from_bytes(&value)?),
                    TIMESTAMP_COLUMN_QUALIFIER => timestamp = bcs::from_bytes(&value)?,
                    CHECKPOINT_NUMBER_COLUMN_QUALIFIER => {
                        checkpoint_number = bcs::from_bytes(&value)?
                    }
                    _ => error!("unexpected column {:?} in transactions table", column),
                }
            }
            result.push(TransactionData {
                transaction: transaction.context("transaction field is missing")?,
                effects: effects.context("effects field is missing")?,
                events: events.context("events field is missing")?,
                timestamp,
                checkpoint_number,
            })
        }
        Ok(result)
    }

    async fn get_checkpoints(
        &mut self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> Result<Vec<Checkpoint>> {
        let keys = sequence_numbers
            .iter()
            .map(|sq| sq.to_be_bytes().to_vec())
            .collect();
        let mut checkpoints = vec![];
        for (_, row) in self.multi_get(CHECKPOINTS_TABLE, keys, None).await? {
            let mut summary = None;
            let mut contents = None;
            let mut signatures = None;
            for (column, value) in row {
                match std::str::from_utf8(&column)? {
                    CHECKPOINT_SUMMARY_COLUMN_QUALIFIER => summary = Some(bcs::from_bytes(&value)?),
                    CHECKPOINT_CONTENTS_COLUMN_QUALIFIER => {
                        contents = Some(bcs::from_bytes(&value)?)
                    }
                    CHECKPOINT_SIGNATURES_COLUMN_QUALIFIER => {
                        signatures = Some(bcs::from_bytes(&value)?)
                    }
                    _ => error!("unexpected column {:?} in checkpoints table", column),
                }
            }
            let checkpoint = Checkpoint {
                summary: summary.context("summary field is missing")?,
                contents: contents.context("contents field is missing")?,
                signatures: signatures.context("signatures field is missing")?,
            };
            checkpoints.push(checkpoint);
        }
        Ok(checkpoints)
    }

    async fn get_checkpoint_by_digest(
        &mut self,
        digest: CheckpointDigest,
    ) -> Result<Option<Checkpoint>> {
        let key = digest.inner().to_vec();
        let mut response = self
            .multi_get(CHECKPOINTS_BY_DIGEST_TABLE, vec![key], None)
            .await?;
        if let Some((_, row)) = response.pop() {
            if let Some((_, value)) = row.into_iter().next() {
                let sequence_number = u64::from_be_bytes(value.as_slice().try_into()?);
                if let Some(chk) = self.get_checkpoints(&[sequence_number]).await?.pop() {
                    return Ok(Some(chk));
                }
            }
        }
        Ok(None)
    }

    async fn get_latest_checkpoint(&mut self) -> Result<CheckpointSequenceNumber> {
        match self
            .multi_get(WATERMARK_ALT_TABLE, vec![vec![0]], None)
            .await?
            .pop()
            .and_then(|(_, mut row)| row.pop())
        {
            Some((_, value_bytes)) => Ok(u64::from_be_bytes(value_bytes.as_slice().try_into()?)),
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
                CHECKPOINTS_TABLE,
                vec![(sequence_number - 1).to_be_bytes().to_vec()],
                Some(RowFilter {
                    filter: Some(Filter::ColumnQualifierRegexFilter(
                        format!("^({CHECKPOINT_SUMMARY_COLUMN_QUALIFIER})$").into(),
                    )),
                }),
            )
            .await?;

        let Some((_, row)) = response.pop() else {
            return Ok(None);
        };

        let mut summary: Option<CheckpointSummary> = None;
        for (column, value) in row {
            match std::str::from_utf8(&column)? {
                CHECKPOINT_SUMMARY_COLUMN_QUALIFIER => summary = Some(bcs::from_bytes(&value)?),
                _ => error!("unexpected column {:?} in checkpoints table", column),
            }
        }

        Ok(summary)
    }

    async fn get_latest_object(&mut self, object_id: &ObjectID) -> Result<Option<Object>> {
        let upper_limit = Self::raw_object_key(&ObjectKey::max_for_id(object_id))?;
        if let Some((_, row)) = self.reversed_scan(OBJECTS_TABLE, upper_limit).await?.pop() {
            if let Some((_, value)) = row.into_iter().next() {
                return Ok(Some(bcs::from_bytes(&value)?));
            }
        }
        Ok(None)
    }

    async fn get_epoch(&mut self, epoch_id: EpochId) -> Result<Option<EpochInfo>> {
        let key = epoch_id.to_be_bytes().to_vec();
        Ok(
            match self.multi_get(EPOCHS_TABLE, vec![key], None).await?.pop() {
                Some((_, mut row)) => row
                    .pop()
                    .map(|value| bcs::from_bytes(&value.1))
                    .transpose()?,
                None => None,
            },
        )
    }

    async fn get_latest_epoch(&mut self) -> Result<Option<EpochInfo>> {
        let upper_limit = u64::MAX.to_be_bytes().to_vec();
        Ok(
            match self.reversed_scan(EPOCHS_TABLE, upper_limit).await?.pop() {
                Some((_, mut row)) => row
                    .pop()
                    .map(|value| bcs::from_bytes(&value.1))
                    .transpose()?,
                None => None,
            },
        )
    }

    // Multi-get transactions, selecting columns relevant to events.
    async fn get_events_for_transactions(
        &mut self,
        transaction_digests: &[TransactionDigest],
    ) -> Result<Vec<(TransactionDigest, TransactionEventsData)>> {
        let query = self.multi_get(
            TRANSACTIONS_TABLE,
            transaction_digests
                .iter()
                .map(|tx| tx.inner().to_vec())
                .collect(),
            Some(RowFilter {
                filter: Some(Filter::ColumnQualifierRegexFilter(
                    format!("^({EVENTS_COLUMN_QUALIFIER}|{TIMESTAMP_COLUMN_QUALIFIER})$").into(),
                )),
            }),
        );
        let mut results = vec![];

        for (key, row) in query.await? {
            let mut transaction_events: Option<Option<TransactionEvents>> = None;
            let mut timestamp_ms = 0;
            for (column, value) in row {
                match std::str::from_utf8(&column)? {
                    EVENTS_COLUMN_QUALIFIER => transaction_events = Some(bcs::from_bytes(&value)?),
                    TIMESTAMP_COLUMN_QUALIFIER => timestamp_ms = bcs::from_bytes(&value)?,
                    _ => error!("unexpected column {:?} in transactions table", column),
                }
            }
            let events = transaction_events
                .context("events field is missing")?
                .map(|e| e.data)
                .unwrap_or_default();

            let transaction_digest = TransactionDigest::try_from(key)
                .context("Failed to deserialize transaction digest")?;

            results.push((
                transaction_digest,
                TransactionEventsData {
                    events,
                    timestamp_ms,
                },
            ));
        }

        Ok(results)
    }
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

    pub async fn mutate_rows(
        &mut self,
        request: MutateRowsRequest,
    ) -> Result<Streaming<MutateRowsResponse>> {
        Ok(self.client.mutate_rows(request).await?.into_inner())
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

    pub async fn read_rows(
        &mut self,
        mut request: ReadRowsRequest,
        table_name: &str,
    ) -> Result<Vec<(Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>)>> {
        if let Some(ref app_profile_id) = self.app_profile_id {
            request.app_profile_id = app_profile_id.clone();
        }
        let mut result = vec![];
        let mut response = self.client.read_rows(request).await?.into_inner();

        let mut row_key = None;
        let mut row = vec![];
        let mut cell_value = vec![];
        let mut cell_name = None;
        let mut timestamp = 0;

        while let Some(message) = response.message().await? {
            self.report_bt_stats(&message.request_stats, table_name);
            for mut chunk in message.chunks.into_iter() {
                // new row check
                if !chunk.row_key.is_empty() {
                    row_key = Some(chunk.row_key);
                }
                match chunk.qualifier {
                    // new cell started
                    Some(qualifier) => {
                        if let Some(cell_name) = cell_name {
                            row.push((cell_name, cell_value));
                            cell_value = vec![];
                        }
                        cell_name = Some(qualifier);
                        timestamp = chunk.timestamp_micros;
                        cell_value.append(&mut chunk.value);
                    }
                    None => {
                        if chunk.timestamp_micros == 0 {
                            cell_value.append(&mut chunk.value);
                        } else if chunk.timestamp_micros >= timestamp {
                            // newer version of cell is available
                            timestamp = chunk.timestamp_micros;
                            cell_value = chunk.value;
                        }
                    }
                }
                if chunk.row_status.is_some() {
                    if let Some(RowStatus::CommitRow(_)) = chunk.row_status {
                        if let Some(cell_name) = cell_name {
                            row.push((cell_name, cell_value));
                        }
                        if let Some(row_key) = row_key {
                            result.push((row_key, row));
                        }
                    }
                    row_key = None;
                    row = vec![];
                    cell_value = vec![];
                    cell_name = None;
                }
            }
        }
        Ok(result)
    }

    async fn multi_set(
        &mut self,
        table_name: &str,
        values: impl IntoIterator<Item = (Bytes, Vec<(&str, Bytes)>)> + std::marker::Send,
        timestamp_ms: Option<TimestampMs>,
    ) -> Result<()> {
        for chunk in values.into_iter().collect::<Vec<_>>().chunks(50_000) {
            self.multi_set_internal(table_name, chunk.iter().cloned(), timestamp_ms)
                .await?;
        }
        Ok(())
    }

    async fn multi_set_internal(
        &mut self,
        table_name: &str,
        values: impl IntoIterator<Item = (Bytes, Vec<(&str, Bytes)>)> + std::marker::Send,
        timestamp_ms: Option<TimestampMs>,
    ) -> Result<()> {
        let mut entries = vec![];
        let timestamp_micros = timestamp_ms
            .map(|tst| {
                tst.checked_mul(1000)
                    .expect("timestamp multiplication overflow") as i64
            })
            // default to -1 for current Bigtable server time
            .unwrap_or(-1);
        for (row_key, cells) in values {
            let mutations = cells
                .into_iter()
                .map(|(column_name, value)| Mutation {
                    mutation: Some(mutation::Mutation::SetCell(SetCell {
                        family_name: COLUMN_FAMILY_NAME.to_string(),
                        column_qualifier: column_name.to_owned().into_bytes(),
                        // The timestamp of the cell into which new data should be written.
                        timestamp_micros,
                        value,
                    })),
                })
                .collect();
            entries.push(Entry { row_key, mutations });
        }
        let request = MutateRowsRequest {
            table_name: format!("{}{}", self.table_prefix, table_name),
            entries,
            ..MutateRowsRequest::default()
        };
        let mut response = self.mutate_rows(request).await?;
        while let Some(part) = response.message().await? {
            for entry in part.entries {
                if let Some(status) = entry.status {
                    if status.code != 0 {
                        return Err(anyhow!(
                            "bigtable write failed {} {}",
                            status.code,
                            status.message
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn multi_get(
        &mut self,
        table_name: &str,
        keys: Vec<Vec<u8>>,
        filter: Option<RowFilter>,
    ) -> Result<Vec<(Vec<u8>, Vec<(Bytes, Bytes)>)>> {
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

    pub async fn multi_get_internal(
        &mut self,
        table_name: &str,
        keys: Vec<Vec<u8>>,
        filter: Option<RowFilter>,
    ) -> Result<Vec<(Vec<u8>, Vec<(Bytes, Bytes)>)>> {
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
                row_keys: keys,
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

    fn raw_object_key(object_key: &ObjectKey) -> Result<Vec<u8>> {
        let mut raw_key = object_key.0.to_vec();
        raw_key.extend(object_key.1.value().to_be_bytes());
        Ok(raw_key)
    }
}

impl Service<Request<BoxBody>> for AuthChannel {
    type Response = Response<BoxBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.channel.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, mut request: Request<BoxBody>) -> Self::Future {
        let cloned_channel = self.channel.clone();
        let cloned_token = self.token.clone();
        let mut inner = std::mem::replace(&mut self.channel, cloned_channel);
        let policy = self.policy.clone();
        let token_provider = self.token_provider.clone();

        let mut auth_token = None;
        if token_provider.is_some() {
            let guard = self.token.read().expect("failed to acquire a read lock");
            if let Some(token) = &*guard {
                if !token.has_expired() {
                    auth_token = Some(token.clone());
                }
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
