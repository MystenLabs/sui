// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::bigtable::proto::bigtable::v2::bigtable_client::BigtableClient as BigtableInternalClient;
use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::bigtable::proto::bigtable::v2::mutation::SetCell;
use crate::bigtable::proto::bigtable::v2::read_rows_response::cell_chunk::RowStatus;
use crate::bigtable::proto::bigtable::v2::row_range::EndKey;
use crate::bigtable::proto::bigtable::v2::{
    mutation, MutateRowsRequest, MutateRowsResponse, Mutation, ReadRowsRequest, RowRange, RowSet,
};
use crate::{Checkpoint, KeyValueStoreReader, KeyValueStoreWriter, TransactionData};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use gcp_auth::{Token, TokenProvider};
use http::{HeaderValue, Request, Response};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use std::time::Duration;
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::digests::CheckpointDigest;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tonic::body::BoxBody;
use tonic::codegen::Service;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};
use tonic::Streaming;
use tracing::error;

const OBJECTS_TABLE: &str = "objects";
const TRANSACTIONS_TABLE: &str = "transactions";
const CHECKPOINTS_TABLE: &str = "checkpoints";
const CHECKPOINTS_BY_DIGEST_TABLE: &str = "checkpoints_by_digest";
const WATERMARK_TABLE: &str = "watermark";

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
}

#[async_trait]
impl KeyValueStoreWriter for BigTableClient {
    async fn save_objects(&mut self, objects: &[&Object]) -> Result<()> {
        let mut items = Vec::with_capacity(objects.len());
        for object in objects {
            let object_key = ObjectKey(object.id(), object.version());
            items.push((
                Self::raw_object_key(&object_key)?,
                vec![(DEFAULT_COLUMN_QUALIFIER, bcs::to_bytes(object)?)],
            ));
        }
        self.multi_set(OBJECTS_TABLE, items).await
    }

    async fn save_transactions(&mut self, transactions: &[TransactionData]) -> Result<()> {
        let mut items = Vec::with_capacity(transactions.len());
        for transaction in transactions {
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
        self.multi_set(TRANSACTIONS_TABLE, items).await
    }

    async fn save_checkpoint(&mut self, checkpoint: &CheckpointData) -> Result<()> {
        let summary = &checkpoint.checkpoint_summary.data();
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
        self.multi_set(CHECKPOINTS_TABLE, [(key.clone(), cells)])
            .await?;
        self.multi_set(
            CHECKPOINTS_BY_DIGEST_TABLE,
            [(
                checkpoint.checkpoint_summary.digest().inner().to_vec(),
                vec![(DEFAULT_COLUMN_QUALIFIER, key)],
            )],
        )
        .await
    }

    async fn save_watermark(&mut self, watermark: CheckpointSequenceNumber) -> Result<()> {
        let key = watermark.to_be_bytes().to_vec();
        self.multi_set(
            WATERMARK_TABLE,
            [(key, vec![(DEFAULT_COLUMN_QUALIFIER, vec![])])],
        )
        .await
    }
}

#[async_trait]
impl KeyValueStoreReader for BigTableClient {
    async fn get_objects(&mut self, object_keys: &[ObjectKey]) -> Result<Vec<Object>> {
        let keys: Result<_, _> = object_keys.iter().map(Self::raw_object_key).collect();
        let mut objects = vec![];
        for row in self.multi_get(OBJECTS_TABLE, keys?).await? {
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
        for row in self.multi_get(TRANSACTIONS_TABLE, keys).await? {
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
                transaction: transaction.ok_or_else(|| anyhow!("transaction field is missing"))?,
                effects: effects.ok_or_else(|| anyhow!("effects field is missing"))?,
                events: events.ok_or_else(|| anyhow!("events field is missing"))?,
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
        for row in self.multi_get(CHECKPOINTS_TABLE, keys).await? {
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
                summary: summary.ok_or_else(|| anyhow!("summary field is missing"))?,
                contents: contents.ok_or_else(|| anyhow!("contents field is missing"))?,
                signatures: signatures.ok_or_else(|| anyhow!("signatures field is missing"))?,
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
            .multi_get(CHECKPOINTS_BY_DIGEST_TABLE, vec![key])
            .await?;
        if let Some(row) = response.pop() {
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
        let upper_limit = u64::MAX.to_be_bytes().to_vec();
        match self
            .reversed_scan(WATERMARK_TABLE, upper_limit)
            .await?
            .pop()
        {
            Some((key_bytes, _)) => Ok(u64::from_be_bytes(key_bytes.as_slice().try_into()?)),
            None => Ok(0),
        }
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
        })
    }

    pub async fn new_remote(
        instance_id: String,
        is_read_only: bool,
        timeout: Option<Duration>,
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
        })
    }

    pub async fn mutate_rows(
        &mut self,
        request: MutateRowsRequest,
    ) -> Result<Streaming<MutateRowsResponse>> {
        Ok(self.client.mutate_rows(request).await?.into_inner())
    }

    pub async fn read_rows(
        &mut self,
        request: ReadRowsRequest,
    ) -> Result<Vec<(Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>)>> {
        let mut result = vec![];
        let mut response = self.client.read_rows(request).await?.into_inner();

        let mut row_key = None;
        let mut row = vec![];
        let mut cell_value = vec![];
        let mut cell_name = None;
        let mut timestamp = 0;

        while let Some(message) = response.message().await? {
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
    ) -> Result<()> {
        let mut entries = vec![];
        for (row_key, cells) in values {
            let mutations = cells
                .into_iter()
                .map(|(column_name, value)| Mutation {
                    mutation: Some(mutation::Mutation::SetCell(SetCell {
                        family_name: COLUMN_FAMILY_NAME.to_string(),
                        column_qualifier: column_name.to_owned().into_bytes(),
                        // The timestamp of the cell into which new data should be written.
                        // Use -1 for current Bigtable server time.
                        timestamp_micros: -1,
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
        self.mutate_rows(request).await?;
        Ok(())
    }

    pub async fn multi_get(
        &mut self,
        table_name: &str,
        keys: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<(Bytes, Bytes)>>> {
        let request = ReadRowsRequest {
            table_name: format!("{}{}", self.table_prefix, table_name),
            rows_limit: keys.len() as i64,
            rows: Some(RowSet {
                row_keys: keys,
                row_ranges: vec![],
            }),
            ..ReadRowsRequest::default()
        };
        let mut result = vec![];
        for (_, cells) in self.read_rows(request).await? {
            result.push(cells);
        }
        Ok(result)
    }

    async fn reversed_scan(
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
        self.read_rows(request).await
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
