// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_fetcher::{DataFetcher, RemoteFetcher},
    types::{ReplayEngineError, MAX_CONCURRENT_REQUESTS, RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD},
};
use std::fmt::Debug;
use std::{collections::VecDeque, fmt::Formatter};
use sui_sdk::SuiClientBuilder;
use sui_types::digests::TransactionDigest;

#[derive(Clone, Debug)]
pub enum TransactionSource {
    /// Fetch a random transaction from the network
    Random,
    /// Fetch a transaction from the network with a specific checkpoint ID
    FromCheckpoint(u64),
    /// Use the latest transaction from the network
    TailLatest { start_checkpoint: Option<u64> },
    /// Use a random transaction from an inclusive range of checkpoint IDs
    FromInclusiveCheckpointRange {
        checkpoint_start: u64,
        checkpoint_end: u64,
    },
}

#[derive(Clone)]
pub struct TransactionProvider {
    pub fetcher: RemoteFetcher,
    pub source: TransactionSource,
    pub last_checkpoint: Option<u64>,
    pub transactions_left: VecDeque<TransactionDigest>,
}

impl Debug for TransactionProvider {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionProvider")
            // TODO: impl Debug for fetcher
            //.field("fetcher", &self.fetcher)
            .field("source", &self.source)
            .field("last_checkpoint", &self.last_checkpoint)
            .field("transactions_left", &self.transactions_left)
            .finish()
    }
}

impl TransactionProvider {
    pub async fn new(http_url: &str, source: TransactionSource) -> Result<Self, ReplayEngineError> {
        Ok(Self {
            fetcher: RemoteFetcher::new(
                SuiClientBuilder::default()
                    .request_timeout(RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD)
                    .max_concurrent_requests(MAX_CONCURRENT_REQUESTS)
                    .build(http_url)
                    .await?,
            ),
            source,
            last_checkpoint: None,
            transactions_left: VecDeque::new(),
        })
    }

    pub async fn next(&mut self) -> Result<Option<TransactionDigest>, ReplayEngineError> {
        let tx = match &self.source {
            TransactionSource::Random => {
                let tx = self.fetcher.fetch_random_transaction(None, None).await?;
                Some(tx)
            }
            TransactionSource::FromCheckpoint(checkpoint_id) => {
                let tx = self
                    .fetcher
                    .fetch_random_transaction(Some(*checkpoint_id), Some(*checkpoint_id))
                    .await?;
                Some(tx)
            }
            TransactionSource::TailLatest { start_checkpoint } => {
                if let Some(tx) = self.transactions_left.pop_front() {
                    Some(tx)
                } else {
                    let next_checkpoint =
                    // Advance to next checkpoint
                    self.last_checkpoint.map(|c| c + 1).unwrap_or(start_checkpoint.unwrap_or(1));
                    self.transactions_left = self
                        .fetcher
                        .get_checkpoint_txs(next_checkpoint)
                        .await?
                        .into();
                    self.last_checkpoint = Some(next_checkpoint);
                    self.transactions_left.pop_front()
                }
            }
            TransactionSource::FromInclusiveCheckpointRange {
                checkpoint_start,
                checkpoint_end,
            } => {
                let tx = self
                    .fetcher
                    .fetch_random_transaction(Some(*checkpoint_start), Some(*checkpoint_end))
                    .await?;
                Some(tx)
            }
        };

        Ok(tx)
    }
}
