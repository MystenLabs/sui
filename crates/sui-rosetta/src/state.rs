// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use sha3::{Digest, Sha3_256};
use tokio::sync::RwLock;
use tracing::{debug, error};

use sui_config::genesis::Genesis;
use sui_core::authority::AuthorityState;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_quorum_driver::QuorumDriver;
use sui_types::base_types::{
    SequenceNumber, SuiAddress, TransactionDigest, TRANSACTION_DIGEST_LENGTH,
};
use sui_types::gas_coin::GasCoin;

use crate::operations::Operation;
use crate::types::{
    AccountIdentifier, Amount, Block, BlockHash, BlockIdentifier, BlockResponse, CoinAction,
    CoinChange, CoinID, CoinIdentifier, OperationIdentifier, OperationStatus, OperationType,
    SignedValue, Transaction, TransactionIdentifier,
};
use crate::ErrorType::{BlockNotFound, InternalError};
use crate::{Error, NetworkIdentifier, SuiEnv, UnsupportedBlockchain, UnsupportedNetwork, SUI};

pub struct ServerContext {
    pub env: SuiEnv,
    pub state: Arc<AuthorityState>,
    pub quorum_driver: Arc<QuorumDriver<NetworkAuthorityClient>>,
    block_provider: Arc<dyn BlockProvider + Send + Sync>,
    book_keeper: BookKeeper,
}

impl ServerContext {
    pub fn new(
        env: SuiEnv,
        state: Arc<AuthorityState>,
        quorum_driver: Arc<QuorumDriver<NetworkAuthorityClient>>,
        block_provider: Arc<dyn BlockProvider + Send + Sync>,
        book_keeper: BookKeeper,
    ) -> Self {
        Self {
            env,
            state,
            quorum_driver,
            block_provider,
            book_keeper,
        }
    }
}

pub struct BookKeeper {
    pub state: Arc<AuthorityState>,
    pub blocks: Arc<dyn BlockProvider + Send + Sync>,
}

impl BookKeeper {
    pub async fn get_balance(&self, address: &SuiAddress, block_height: u64) -> Result<u64, Error> {
        let current_block = self.blocks.current_block().await?;

        Ok(0)
    }

    async fn get_current_balance(&self, address: &SuiAddress) -> Result<u64, Error> {
        let new_txs = self.get_uncheckpointed_tx().await?;
        //let txs = self.get_balance_changing_txs(new_txs, address).await?;

        Ok(0)
    }

    async fn get_uncheckpointed_tx(&self) -> Result<Vec<TransactionDigest>, Error> {
        let all_tx = self.state.get_total_transaction_number()?;
        let current_block = self.blocks.current_block().await?;

        Ok(if current_block.block.block_identifier.seq < all_tx {
            self.state
                .get_transactions_in_range(current_block.block.block_identifier.index, all_tx)?
                .into_iter()
                .map(|(_, digest)| digest)
                .collect()
        } else {
            vec![]
        })
    }
}

impl ServerContext {
    pub fn checks_network_identifier(
        &self,
        network_identifier: &NetworkIdentifier,
    ) -> Result<(), Error> {
        if &network_identifier.blockchain != "sui" {
            return Err(Error::new(UnsupportedBlockchain));
        }
        if self.env != network_identifier.network {
            return Err(Error::new(UnsupportedNetwork));
        }
        Ok(())
    }

    pub fn blocks(&self) -> &(dyn BlockProvider + Sync + Send) {
        &*self.block_provider
    }

    pub fn book_keeper(&self) -> &BookKeeper {
        &self.book_keeper
    }
}
#[async_trait]
pub trait BlockProvider {
    async fn get_block_by_index(&self, index: u64) -> Result<BlockResponse, Error>;
    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error>;
    async fn current_block(&self) -> Result<BlockResponse, Error>;
    fn genesis_block_identifier(&self) -> BlockIdentifier;
    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error>;
    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error>;
}

#[derive(Clone)]
pub struct PseudoBlockProvider {
    blocks: Arc<RwLock<Vec<BlockResponse>>>,
}

#[async_trait]
impl BlockProvider for PseudoBlockProvider {
    async fn get_block_by_index(&self, index: u64) -> Result<BlockResponse, Error> {
        self.blocks
            .read()
            .await
            .iter()
            .find(|b| b.block.block_identifier.index == index)
            .cloned()
            .ok_or_else(|| Error::new(BlockNotFound))
    }

    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error> {
        self.blocks
            .read()
            .await
            .iter()
            .find(|b| b.block.block_identifier.hash == hash)
            .cloned()
            .ok_or_else(|| Error::new(BlockNotFound))
    }

    async fn current_block(&self) -> Result<BlockResponse, Error> {
        self.blocks
            .read()
            .await
            .last()
            .ok_or_else(|| {
                Error::new_with_msg(
                    BlockNotFound,
                    "Unexpected error, cannot find the latest block.",
                )
            })
            .cloned()
    }

    fn genesis_block_identifier(&self) -> BlockIdentifier {
        BlockIdentifier {
            index: 0,
            seq: 0,
            hash: BlockHash([0u8; TRANSACTION_DIGEST_LENGTH]),
        }
    }

    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.blocks
            .read()
            .await
            .first()
            .map(|b| b.block.block_identifier.clone())
            .ok_or_else(|| {
                Error::new_with_msg(
                    BlockNotFound,
                    "Unexpected error, cannot find the oldest block.",
                )
            })
    }

    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error> {
        self.current_block().await.map(|b| b.block.block_identifier)
    }
}

impl PseudoBlockProvider {
    pub fn spawn(state: Arc<AuthorityState>, genesis: &Genesis) -> Self {
        let blocks = Self {
            blocks: Arc::new(RwLock::new(vec![genesis_block(genesis)])),
        };

        let block_interval = option_env!("SUI_BLOCK_INTERVAL")
            .map(|i| u64::from_str(i).ok())
            .flatten()
            .unwrap_or(10000);
        let block_interval = Duration::from_millis(block_interval);

        let f = blocks.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = f.create_next_block(state.clone()).await {
                    error!("Error creating block, cause: {e:?}")
                }
                tokio::time::sleep(block_interval).await;
            }
        });

        blocks
    }

    async fn create_next_block(&self, state: Arc<AuthorityState>) -> Result<(), Error> {
        let current_block = self.current_block_identifier().await?;
        let total_tx = state.get_total_transaction_number()?;

        if current_block.seq < total_tx {
            let tx_digests = state.get_transactions_in_range(current_block.seq, total_tx)?;

            // Create block hash using all transaction hashes
            let hasher = tx_digests
                .iter()
                .fold(Sha3_256::default(), |mut hasher, (_, digest)| {
                    hasher.update(digest.as_ref());
                    hasher
                });
            let hash = hasher.finalize();
            let hash = BlockHash(hash.into());

            let block_identifier = BlockIdentifier {
                index: current_block.index + 1,
                seq: total_tx,
                hash,
            };

            let other_transactions = tx_digests
                .iter()
                .map(|(_, digest)| TransactionIdentifier { hash: *digest })
                .collect();

            let new_block = BlockResponse {
                block: Block {
                    block_identifier,
                    parent_block_identifier: current_block,
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|e| Error::new_with_cause(InternalError, e))?
                        .as_millis()
                        .try_into()?,
                    transactions: vec![],
                    metadata: None,
                },
                other_transactions,
            };
            self.blocks.write().await.push(new_block);
        } else {
            debug!("No new transactions.")
        };

        Ok(())
    }
}

fn genesis_block(genesis: &Genesis) -> BlockResponse {
    let id = BlockIdentifier {
        index: 0,
        seq: 0,
        hash: BlockHash([0u8; TRANSACTION_DIGEST_LENGTH]),
    };

    let operations = genesis
        .objects()
        .iter()
        .flat_map(|o| {
            GasCoin::try_from(o)
                .ok()
                .and_then(|coin| o.owner.get_owner_address().ok().map(|addr| (addr, coin)))
        })
        .enumerate()
        .map(|(index, (address, coin))| Operation {
            operation_identifier: OperationIdentifier {
                index: index.try_into().unwrap(),
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::Genesis,
            status: Some(OperationStatus::Success.to_string()),
            account: Some(AccountIdentifier { address }),
            amount: Some(Amount {
                value: SignedValue::from(coin.value()),
                currency: SUI.clone(),
            }),
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier {
                    identifier: CoinID {
                        id: *coin.id(),
                        version: Some(SequenceNumber::new()),
                    },
                },
                coin_action: CoinAction::CoinCreated,
            }),
            metadata: None,
        })
        .collect();

    let transaction = Transaction {
        transaction_identifier: TransactionIdentifier {
            hash: TransactionDigest::new([0; 32]),
        },
        operations,
        related_transactions: vec![],
        metadata: None,
    };

    BlockResponse {
        block: Block {
            block_identifier: id.clone(),
            parent_block_identifier: id,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
                .try_into()
                .unwrap(),
            transactions: vec![transaction],
            metadata: None,
        },
        other_transactions: vec![],
    }
}
