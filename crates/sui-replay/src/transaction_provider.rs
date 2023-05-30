// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_fetcher::{DataFetcher, RemoteFetcher},
    types::{ReplayEngineError, MAX_CONCURRENT_REQUESTS, RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD},
};
use std::{collections::VecDeque, fmt::Formatter};
use std::{fmt::Debug, str::FromStr};
use sui_sdk::SuiClientBuilder;
use sui_types::digests::TransactionDigest;
use tracing::info;

const VALID_CHECKPOINT_START: u64 = 1;

#[derive(Clone, Debug)]
pub enum TransactionSource {
    /// Fetch a random transaction from the network
    Random,
    /// Fetch a transaction from the network with a specific checkpoint ID
    FromCheckpoint(u64),
    /// Use the latest transaction from the network
    TailLatest { start: Option<FuzzStartPoint> },
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

#[derive(Eq, PartialEq, Clone, Copy, PartialOrd, Ord, Hash, Debug)]
pub enum FuzzStartPoint {
    Checkpoint(u64),
    TxDigest(TransactionDigest),
}

impl FromStr for FuzzStartPoint {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<u64>() {
            Ok(n) => Ok(FuzzStartPoint::Checkpoint(n)),
            Err(u64_err) => match TransactionDigest::from_str(s) {
                Ok(d) => Ok(FuzzStartPoint::TxDigest(d)),
                Err(tx_err) => {
                    info!("{} is not a valid checkpoint (err: {:?}) or transaction digest (err: {:?})", s, u64_err, tx_err);
                    Err(tx_err)
                }
            },
        }
    }
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
        let tx = match self.source {
            TransactionSource::Random => {
                let tx = self.fetcher.fetch_random_transaction(None, None).await?;
                Some(tx)
            }
            TransactionSource::FromCheckpoint(checkpoint_id) => {
                let tx = self
                    .fetcher
                    .fetch_random_transaction(Some(checkpoint_id), Some(checkpoint_id))
                    .await?;
                Some(tx)
            }
            TransactionSource::TailLatest { start } => {
                if let Some(tx) = self.transactions_left.pop_front() {
                    Some(tx)
                } else {
                    let next_checkpoint = match start {
                        Some(x) => match x {
                            // Checkpoint specified
                            FuzzStartPoint::Checkpoint(checkpoint_id) => {
                                self.source = TransactionSource::TailLatest {
                                    start: Some(FuzzStartPoint::Checkpoint(checkpoint_id + 1)),
                                };
                                Some(checkpoint_id)
                            }
                            // Digest specified. Find the checkpoint for the digest
                            FuzzStartPoint::TxDigest(tx_digest) => {
                                let ch = self
                                    .fetcher
                                    .get_transaction(&tx_digest)
                                    .await?
                                    .checkpoint
                                    .expect("Transaction must have a checkpoint");
                                // For the next round
                                self.source = TransactionSource::TailLatest {
                                    start: Some(FuzzStartPoint::Checkpoint(ch + 1)),
                                };
                                Some(ch)
                            }
                        },
                        // Advance to next checkpoint if available
                        None => self.last_checkpoint.map(|c| c + 1),
                    }
                    .unwrap_or(VALID_CHECKPOINT_START);

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
                    .fetch_random_transaction(Some(checkpoint_start), Some(checkpoint_end))
                    .await?;
                Some(tx)
            }
        };

        Ok(tx)
    }
}
