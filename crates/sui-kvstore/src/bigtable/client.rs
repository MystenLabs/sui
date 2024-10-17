// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::bigtable::proto::bigtable::v2::bigtable_client::BigtableClient as BigtableInternalClient;
use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::bigtable::proto::bigtable::v2::mutation::SetCell;
use crate::bigtable::proto::bigtable::v2::read_rows_response::cell_chunk::RowStatus;
use crate::bigtable::proto::bigtable::v2::{
    mutation, MutateRowsRequest, MutateRowsResponse, Mutation, ReadRowsRequest, RowSet,
};
use crate::{KeyValueStore, TransactionData};
use anyhow::Result;
use async_trait::async_trait;
use gcp_auth::{Token, TokenProvider};
use http::{HeaderValue, Request, Response};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use std::time::Duration;
use sui_types::base_types::{SequenceNumber, TransactionDigest};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tonic::body::BoxBody;
use tonic::codegen::Service;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};
use tonic::Streaming;

const COLUMN_FAMILY_NAME: &str = "sui";
const COLUMN_QUALIFIER: &str = "";
const OBJECTS_TABLE: &str = "objects";
const CHECKPOINTS_TABLE: &str = "checkpoints";
const TRANSACTIONS_TABLE: &str = "transactions";

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
impl KeyValueStore for BigTableClient {
    async fn get_objects(&mut self, object_keys: &[ObjectKey]) -> Result<Vec<Object>> {
        let keys: Result<_, _> = object_keys.iter().map(|key| bcs::to_bytes(&key)).collect();
        self.multi_get(OBJECTS_TABLE, keys?).await
    }

    async fn get_transactions(
        &mut self,
        transactions: &[TransactionDigest],
    ) -> Result<Vec<TransactionData>> {
        let keys = transactions.iter().map(|tx| tx.inner().to_vec()).collect();
        self.multi_get(TRANSACTIONS_TABLE, keys).await
    }

    async fn get_checkpoint(
        &mut self,
        sequence_number: SequenceNumber,
    ) -> Result<Option<CheckpointData>> {
        let key = sequence_number.value().to_be_bytes().to_vec();
        let mut result = self.multi_get(CHECKPOINTS_TABLE, vec![key]).await?;
        Ok(result.pop())
    }

    async fn save_objects(&mut self, objects: &[&Object]) -> Result<()> {
        let mut items = Vec::with_capacity(objects.len());
        for object in objects {
            let object_key = ObjectKey(object.id(), object.version());
            items.push((bcs::to_bytes(&object_key)?, object));
        }
        self.multi_set(OBJECTS_TABLE, items).await
    }

    async fn save_transactions(&mut self, transactions: &[TransactionData]) -> Result<()> {
        let items = transactions.iter().map(|transaction| {
            (
                transaction.transaction.digest().inner().to_vec(),
                transaction,
            )
        });
        self.multi_set(TRANSACTIONS_TABLE, items).await
    }

    async fn save_checkpoint(&mut self, checkpoint: &CheckpointData) -> Result<()> {
        let sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let key = sequence_number.to_be_bytes().to_vec();
        self.multi_set(CHECKPOINTS_TABLE, [(key, checkpoint)]).await
    }
}

impl BigTableClient {
    pub async fn new(
        instance_id: String,
        is_read_only: bool,
        timeout: Option<Duration>,
    ) -> Result<Self> {
        let policy = if is_read_only {
            "https://www.googleapis.com/auth/bigtable.data.readonly"
        } else {
            "https://www.googleapis.com/auth/bigtable.data"
        };
        Ok(match std::env::var("BIGTABLE_EMULATOR_HOST") {
            Ok(emulator_host) => {
                let auth_channel = AuthChannel {
                    channel: Channel::from_shared(format!("http://{emulator_host}"))?
                        .connect_lazy(),
                    policy: policy.to_string(),
                    token_provider: None,
                    token: Arc::new(RwLock::new(None)),
                };
                Self {
                    table_prefix: format!("projects/emulator/instances/{}/tables/", instance_id),
                    client: BigtableInternalClient::new(auth_channel),
                }
            }
            Err(_) => {
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
                Self {
                    table_prefix,
                    client: BigtableInternalClient::new(auth_channel),
                }
            }
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

    async fn multi_set<V: Serialize>(
        &mut self,
        table_name: &str,
        values: impl IntoIterator<Item = (Vec<u8>, V)> + std::marker::Send,
    ) -> Result<()> {
        let mut entries = vec![];
        for (key, value) in values {
            let entry = Entry {
                row_key: key,
                mutations: vec![Mutation {
                    mutation: Some(mutation::Mutation::SetCell(SetCell {
                        family_name: COLUMN_FAMILY_NAME.to_string(),
                        column_qualifier: COLUMN_QUALIFIER.to_owned().into_bytes(),
                        timestamp_micros: -1,
                        value: bcs::to_bytes(&value)?,
                    })),
                }],
            };
            entries.push(entry);
        }
        let request = MutateRowsRequest {
            table_name: format!("{}{}", self.table_prefix, table_name),
            entries,
            ..MutateRowsRequest::default()
        };
        self.mutate_rows(request).await?;
        Ok(())
    }

    pub async fn multi_get<V>(&mut self, table_name: &str, keys: Vec<Vec<u8>>) -> Result<Vec<V>>
    where
        V: DeserializeOwned,
    {
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
            for (_, cell_value) in cells {
                result.push(bcs::from_bytes(&cell_value)?);
            }
        }
        Ok(result)
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
            Ok(inner.call(request).await?)
        })
    }
}
