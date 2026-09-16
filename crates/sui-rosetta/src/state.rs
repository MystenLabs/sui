// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::DateTime;
use futures::stream::{self, StreamExt, TryStreamExt};
use prost_types::FieldMask;
use std::str::FromStr;
use std::sync::Arc;
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    Checkpoint, GetCheckpointRequest, GetServiceInfoRequest, get_checkpoint_request,
};
use sui_types::base_types::TransactionDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use sui_types::digests::ChainIdentifier;

use crate::operations::Operations;
use crate::types::{
    Block, BlockHash, BlockIdentifier, BlockResponse, Transaction, TransactionIdentifier,
};
use crate::{CoinMetadataCache, Error};

#[derive(Clone)]
pub struct OnlineServerContext {
    pub client: GrpcClient,
    pub coin_metadata_cache: CoinMetadataCache,
    pub chain_id: ChainIdentifier,
    block_provider: Arc<dyn BlockProvider + Send + Sync>,
}

impl OnlineServerContext {
    pub fn new(
        client: GrpcClient,
        block_provider: Arc<dyn BlockProvider + Send + Sync>,
        coin_metadata_cache: CoinMetadataCache,
        chain_id: ChainIdentifier,
    ) -> Self {
        Self {
            client,
            block_provider,
            coin_metadata_cache,
            chain_id,
        }
    }

    pub fn blocks(&self) -> &(dyn BlockProvider + Sync + Send) {
        &*self.block_provider
    }
}

#[async_trait]
pub trait BlockProvider {
    async fn get_block_by_index(&self, index: u64) -> Result<BlockResponse, Error>;
    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error>;
    async fn current_block(&self) -> Result<BlockResponse, Error>;
    async fn genesis_block_identifier(&self) -> Result<BlockIdentifier, Error>;
    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error>;
    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error>;
}

#[derive(Clone)]
pub struct CheckpointBlockProvider {
    client: GrpcClient,
    coin_metadata_cache: CoinMetadataCache,
}

#[async_trait]
impl BlockProvider for CheckpointBlockProvider {
    async fn get_block_by_index(&self, index: u64) -> Result<BlockResponse, Error> {
        let request = GetCheckpointRequest::by_sequence_number(index).with_read_mask(
            FieldMask::from_paths([
                "sequence_number",
                "digest",
                "summary.sequence_number",
                "summary.previous_digest",
                "summary.timestamp",
                "transactions.digest",
                "transactions.transaction.sender",
                "transactions.transaction.gas_payment",
                "transactions.transaction.kind",
                "transactions.effects.gas_object",
                "transactions.effects.gas_used",
                "transactions.effects.status",
                "transactions.balance_changes",
                "transactions.events.events.event_type",
                "transactions.events.events.json",
            ]),
        );

        let mut client = self.client.clone();
        let response = client
            .ledger_client()
            .get_checkpoint(request)
            .await
            .map_err(|e| Error::from(anyhow::anyhow!("Failed to get checkpoint: {}", e)))?
            .into_inner();

        let checkpoint = response
            .checkpoint
            .ok_or_else(|| Error::DataError("Checkpoint not found".to_string()))?;

        self.create_block_response(checkpoint).await
    }

    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error> {
        let mut request = GetCheckpointRequest::default().with_read_mask(FieldMask::from_paths([
            "sequence_number",
            "digest",
            "summary.sequence_number",
            "summary.previous_digest",
            "summary.timestamp",
            "transactions.digest",
            "transactions.transaction.sender",
            "transactions.transaction.gas_payment",
            "transactions.transaction.kind",
            "transactions.effects.gas_object",
            "transactions.effects.gas_used",
            "transactions.effects.status",
            "transactions.balance_changes",
            "transactions.events.events.event_type",
            "transactions.events.events.json",
        ]));
        request.checkpoint_id = Some(get_checkpoint_request::CheckpointId::Digest(
            hash.to_string(),
        ));

        let mut client = self.client.clone();
        let response = client
            .ledger_client()
            .get_checkpoint(request)
            .await?
            .into_inner();
        let checkpoint = response
            .checkpoint
            .ok_or_else(|| Error::DataError("Checkpoint not found".to_string()))?;

        self.create_block_response(checkpoint).await
    }

    async fn current_block(&self) -> Result<BlockResponse, Error> {
        let request = GetCheckpointRequest::latest().with_read_mask(FieldMask::from_paths([
            "sequence_number",
            "digest",
            "summary.sequence_number",
            "summary.previous_digest",
            "summary.timestamp",
            "transactions.digest",
            "transactions.transaction.sender",
            "transactions.transaction.gas_payment",
            "transactions.transaction.kind",
            "transactions.effects.gas_object",
            "transactions.effects.gas_used",
            "transactions.effects.status",
            "transactions.balance_changes",
            "transactions.events.events.event_type",
            "transactions.events.events.json",
        ]));

        let mut client = self.client.clone();
        let response = client
            .ledger_client()
            .get_checkpoint(request)
            .await?
            .into_inner();

        let checkpoint = response
            .checkpoint
            .ok_or_else(|| Error::DataError("Checkpoint not found".to_string()))?;

        self.create_block_response(checkpoint).await
    }

    async fn genesis_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        let response = self
            .client
            .clone()
            .ledger_client()
            .get_service_info(GetServiceInfoRequest::default())
            .await?
            .into_inner();
        let chain_id = response
            .chain_id
            .ok_or_else(|| Error::DataError("Missing chain_id".to_string()))?;
        let hash = CheckpointDigest::from_str(&chain_id)?;
        Ok(BlockIdentifier { index: 0, hash })
    }

    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        let response = self
            .client
            .clone()
            .ledger_client()
            .get_service_info(GetServiceInfoRequest::default())
            .await?
            .into_inner();
        let lowest = response
            .lowest_available_checkpoint
            .ok_or_else(|| Error::DataError("Missing lowest_available_checkpoint".to_string()))?;
        self.create_block_identifier(lowest).await
    }

    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        let request = GetCheckpointRequest::latest()
            .with_read_mask(FieldMask::from_paths(["sequence_number", "digest"]));

        let response = self
            .client
            .clone()
            .ledger_client()
            .get_checkpoint(request)
            .await?
            .into_inner();

        let checkpoint = response
            .checkpoint
            .ok_or_else(|| Error::DataError("Missing checkpoint".to_string()))?;

        Ok(BlockIdentifier {
            index: checkpoint.sequence_number(),
            hash: CheckpointDigest::from_str(checkpoint.digest())?,
        })
    }
}

impl CheckpointBlockProvider {
    pub fn new(client: GrpcClient, coin_metadata_cache: CoinMetadataCache) -> Self {
        Self {
            client,
            coin_metadata_cache,
        }
    }

    async fn create_block_response(&self, checkpoint: Checkpoint) -> Result<BlockResponse, Error> {
        let summary = checkpoint.summary();
        let index = summary.sequence_number();
        let hash = CheckpointDigest::from_str(checkpoint.digest())?;
        // Genesis checkpoint (index 0) has no previous digest
        let previous_hash = if index == 0 {
            hash
        } else {
            CheckpointDigest::from_str(summary.previous_digest())?
        };
        let timestamp_ms = summary
            .timestamp
            .ok_or_else(|| Error::DataError("Checkpoint timestamp is missing".to_string()))
            .and_then(|ts| {
                DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                    .ok_or_else(|| Error::DataError(format!("Invalid timestamp: {}", ts)))
            })?
            .timestamp_millis() as u64;

        let transactions: Vec<Transaction> = stream::iter(checkpoint.transactions)
            .map(|executed_tx| async move {
                let digest = TransactionDigest::from_str(executed_tx.digest())?;
                Ok::<_, Error>(Transaction {
                    transaction_identifier: TransactionIdentifier { hash: digest },
                    // This is async because it makes a GetCoinMetadata call if the coin metadata
                    // isn't already cached.
                    operations: Operations::try_from_executed_transaction(
                        executed_tx,
                        &self.coin_metadata_cache,
                    )
                    .await?,
                    related_transactions: vec![],
                    metadata: None,
                })
            })
            // A checkpoint can have thousands of transactions so
            // limit the amount of work a single rosetta request can generate
            // concurrently to prevent resource starvation issues.
            .buffer_unordered(10)
            .try_collect()
            .await?;

        let parent_block_identifier = if index == 0 {
            // Genesis block is its own parent
            BlockIdentifier { index, hash }
        } else {
            BlockIdentifier {
                index: index - 1,
                hash: previous_hash,
            }
        };

        Ok(BlockResponse {
            block: Block {
                block_identifier: BlockIdentifier { index, hash },
                parent_block_identifier,
                timestamp: timestamp_ms,
                transactions,
                metadata: None,
            },
            other_transactions: vec![],
        })
    }

    async fn create_block_identifier(
        &self,
        seq_number: CheckpointSequenceNumber,
    ) -> Result<BlockIdentifier, Error> {
        let grpc_request = GetCheckpointRequest::by_sequence_number(seq_number)
            .with_read_mask(FieldMask::from_paths(["sequence_number", "digest"]));
        let mut client = self.client.clone();
        let response = client
            .ledger_client()
            .get_checkpoint(grpc_request)
            .await?
            .into_inner();

        let checkpoint = response.checkpoint();
        let index = checkpoint.sequence_number();
        let hash = checkpoint.digest();

        Ok(BlockIdentifier {
            index,
            hash: CheckpointDigest::from_str(hash)?,
        })
    }
}

#[cfg(test)]
mod checkpoint_race_tests {
    use super::*;
    use crate::types::{
        AccountBalanceRequest, AccountIdentifier, Currencies, Currency, NetworkIdentifier, SuiEnv,
    };
    use axum::extract::State;
    use axum::{Extension, Json};
    use axum_extra::extract::WithRejection;
    use std::marker::PhantomData;
    use std::num::NonZeroUsize;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use sui_rpc::proto::sui::rpc::v2::ledger_service_server::{LedgerService, LedgerServiceServer};
    use sui_rpc::proto::sui::rpc::v2::state_service_server::{StateService, StateServiceServer};
    use sui_rpc::proto::sui::rpc::v2::{
        Balance, CheckpointSummary, GetBalanceRequest, GetBalanceResponse, GetCheckpointResponse,
        get_checkpoint_request,
    };
    use sui_types::base_types::SuiAddress;
    use tonic::{Request, Response, Status};

    const AHEAD_CHECKPOINT: u64 = 42;

    #[derive(Clone)]
    struct SplitHeightLedger {
        latest_requests: Arc<AtomicUsize>,
        sequence_requests: Arc<AtomicUsize>,
    }

    fn ahead_checkpoint() -> Checkpoint {
        let mut summary = CheckpointSummary::default();
        summary.sequence_number = Some(AHEAD_CHECKPOINT);
        summary.previous_digest = Some(CheckpointDigest::new([1; 32]).to_string());
        summary.timestamp = Some(prost_types::Timestamp {
            seconds: 1,
            nanos: 0,
        });

        let mut checkpoint = Checkpoint::default();
        checkpoint.sequence_number = Some(AHEAD_CHECKPOINT);
        checkpoint.digest = Some(CheckpointDigest::new([2; 32]).to_string());
        checkpoint.summary = Some(summary);
        checkpoint
    }

    #[tonic::async_trait]
    impl LedgerService for SplitHeightLedger {
        async fn get_checkpoint(
            &self,
            request: Request<GetCheckpointRequest>,
        ) -> Result<Response<GetCheckpointResponse>, Status> {
            match request.into_inner().checkpoint_id {
                None => {
                    self.latest_requests.fetch_add(1, Ordering::SeqCst);
                    let mut response = GetCheckpointResponse::default();
                    response.checkpoint = Some(ahead_checkpoint());
                    Ok(Response::new(response))
                }
                Some(get_checkpoint_request::CheckpointId::SequenceNumber(AHEAD_CHECKPOINT)) => {
                    self.sequence_requests.fetch_add(1, Ordering::SeqCst);
                    Err(Status::not_found(format!(
                        "Checkpoint {AHEAD_CHECKPOINT} not found"
                    )))
                }
                Some(checkpoint_id) => Err(Status::invalid_argument(format!(
                    "unexpected checkpoint request: {checkpoint_id:?}"
                ))),
            }
        }
    }

    #[tonic::async_trait]
    impl StateService for SplitHeightLedger {
        async fn get_balance(
            &self,
            _request: Request<GetBalanceRequest>,
        ) -> Result<Response<GetBalanceResponse>, Status> {
            let mut balance = Balance::default();
            balance.balance = Some(100);
            let mut response = GetBalanceResponse::default();
            response.balance = Some(balance);
            Ok(Response::new(response))
        }
    }

    async fn start_split_height_ledger(
        ledger: SplitHeightLedger,
    ) -> (GrpcClient, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let incoming = stream::unfold(listener, |listener| async move {
            let result = listener.accept().await.map(|(stream, _)| stream);
            Some((result, listener))
        });
        let handle = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(LedgerServiceServer::new(ledger.clone()))
                .add_service(StateServiceServer::new(ledger))
                .serve_with_incoming(incoming)
                .await
                .unwrap();
        });
        let client = GrpcClient::new(format!("http://{address}")).unwrap();
        (client, handle)
    }

    #[tokio::test]
    async fn current_block_uses_single_latest_request() {
        let latest_requests = Arc::new(AtomicUsize::new(0));
        let sequence_requests = Arc::new(AtomicUsize::new(0));
        let ledger = SplitHeightLedger {
            latest_requests: latest_requests.clone(),
            sequence_requests: sequence_requests.clone(),
        };
        let (client, server) = start_split_height_ledger(ledger).await;
        let coin_metadata_cache =
            CoinMetadataCache::new(client.clone(), NonZeroUsize::new(1).unwrap());
        let provider = CheckpointBlockProvider::new(client, coin_metadata_cache);

        let response = provider.current_block().await.unwrap();

        assert_eq!(response.block.block_identifier.index, AHEAD_CHECKPOINT);
        assert_eq!(latest_requests.load(Ordering::SeqCst), 1);
        assert_eq!(sequence_requests.load(Ordering::SeqCst), 0);

        server.abort();
    }

    #[tokio::test]
    async fn current_block_identifier_uses_single_latest_request() {
        let latest_requests = Arc::new(AtomicUsize::new(0));
        let sequence_requests = Arc::new(AtomicUsize::new(0));
        let ledger = SplitHeightLedger {
            latest_requests: latest_requests.clone(),
            sequence_requests: sequence_requests.clone(),
        };
        let (client, server) = start_split_height_ledger(ledger).await;
        let coin_metadata_cache =
            CoinMetadataCache::new(client.clone(), NonZeroUsize::new(1).unwrap());
        let provider = CheckpointBlockProvider::new(client, coin_metadata_cache);

        let identifier = provider.current_block_identifier().await.unwrap();

        assert_eq!(identifier.index, AHEAD_CHECKPOINT);
        assert_eq!(latest_requests.load(Ordering::SeqCst), 1);
        assert_eq!(sequence_requests.load(Ordering::SeqCst), 0);

        server.abort();
    }

    #[tokio::test]
    async fn account_balance_uses_single_latest_request() {
        let latest_requests = Arc::new(AtomicUsize::new(0));
        let sequence_requests = Arc::new(AtomicUsize::new(0));
        let ledger = SplitHeightLedger {
            latest_requests: latest_requests.clone(),
            sequence_requests: sequence_requests.clone(),
        };
        let (client, server) = start_split_height_ledger(ledger).await;
        let coin_metadata_cache =
            CoinMetadataCache::new(client.clone(), NonZeroUsize::new(1).unwrap());
        let block_provider = Arc::new(CheckpointBlockProvider::new(
            client.clone(),
            coin_metadata_cache.clone(),
        ));
        let context = OnlineServerContext::new(
            client,
            block_provider,
            coin_metadata_cache,
            ChainIdentifier::from(CheckpointDigest::new([3; 32])),
        );
        let request = AccountBalanceRequest {
            network_identifier: NetworkIdentifier {
                blockchain: "sui".to_string(),
                network: SuiEnv::LocalNet,
            },
            account_identifier: AccountIdentifier {
                address: SuiAddress::ZERO,
                sub_account: None,
            },
            block_identifier: Default::default(),
            currencies: Currencies(vec![Currency::default()]),
        };

        let response = crate::account::balance(
            State(context),
            Extension(SuiEnv::LocalNet),
            WithRejection(Json(request), PhantomData),
        )
        .await
        .unwrap();

        assert_eq!(response.block_identifier.index, AHEAD_CHECKPOINT);
        assert_eq!(response.balances[0].value, 100);
        assert_eq!(latest_requests.load(Ordering::SeqCst), 1);
        assert_eq!(sequence_requests.load(Ordering::SeqCst), 0);

        server.abort();
    }
}
