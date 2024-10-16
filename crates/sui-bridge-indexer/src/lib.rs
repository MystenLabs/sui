// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{Display, Formatter};
use strum_macros::Display;

use sui_types::base_types::{SuiAddress, TransactionDigest};

use crate::models::GovernanceAction as DBGovernanceAction;
use crate::models::TokenTransferData as DBTokenTransferData;
use crate::models::{SuiErrorTransactions, TokenTransfer as DBTokenTransfer};

pub mod config;
pub mod metrics;
pub mod models;
pub mod postgres_manager;
pub mod schema;
pub mod storage;
pub mod sui_transaction_handler;
pub mod sui_transaction_queries;
pub mod types;

pub mod eth_bridge_indexer;
pub mod sui_bridge_indexer;
pub mod sui_datasource;

#[derive(Clone)]
pub enum ProcessedTxnData {
    TokenTransfer(TokenTransfer),
    GovernanceAction(GovernanceAction),
    Error(SuiTxnError),
}

#[derive(Clone)]
pub struct SuiTxnError {
    tx_digest: TransactionDigest,
    sender: SuiAddress,
    timestamp_ms: u64,
    failure_status: String,
    cmd_idx: Option<u64>,
}

#[derive(Clone)]
pub struct TokenTransfer {
    chain_id: u8,
    nonce: u64,
    block_height: u64,
    timestamp_ms: u64,
    txn_hash: Vec<u8>,
    txn_sender: Vec<u8>,
    status: TokenTransferStatus,
    gas_usage: i64,
    data_source: BridgeDataSource,
    data: Option<TokenTransferData>,
    is_finalized: bool,
}

#[derive(Clone)]
pub struct GovernanceAction {
    nonce: Option<u64>,
    data_source: BridgeDataSource,
    tx_digest: Vec<u8>,
    sender: Vec<u8>,
    timestamp_ms: u64,
    action: GovernanceActionType,
    data: serde_json::Value,
}

#[derive(Clone)]
pub struct TokenTransferData {
    sender_address: Vec<u8>,
    destination_chain: u8,
    recipient_address: Vec<u8>,
    token_id: u8,
    amount: u64,
    is_finalized: bool,
}

impl TokenTransfer {
    fn to_db(&self) -> DBTokenTransfer {
        DBTokenTransfer {
            chain_id: self.chain_id as i32,
            nonce: self.nonce as i64,
            block_height: self.block_height as i64,
            timestamp_ms: self.timestamp_ms as i64,
            txn_hash: self.txn_hash.clone(),
            txn_sender: self.txn_sender.clone(),
            status: self.status.to_string(),
            gas_usage: self.gas_usage,
            data_source: self.data_source.to_string(),
            is_finalized: self.is_finalized,
        }
    }

    fn to_data_maybe(&self) -> Option<DBTokenTransferData> {
        self.data.as_ref().map(|data| DBTokenTransferData {
            chain_id: self.chain_id as i32,
            nonce: self.nonce as i64,
            block_height: self.block_height as i64,
            timestamp_ms: self.timestamp_ms as i64,
            txn_hash: self.txn_hash.clone(),
            sender_address: data.sender_address.clone(),
            destination_chain: data.destination_chain as i32,
            recipient_address: data.recipient_address.clone(),
            token_id: data.token_id as i32,
            amount: data.amount as i64,
            is_finalized: data.is_finalized,
        })
    }
}

impl SuiTxnError {
    fn to_db(&self) -> SuiErrorTransactions {
        SuiErrorTransactions {
            txn_digest: self.tx_digest.inner().to_vec(),
            sender_address: self.sender.to_vec(),
            timestamp_ms: self.timestamp_ms as i64,
            failure_status: self.failure_status.clone(),
            cmd_idx: self.cmd_idx.map(|idx| idx as i64),
        }
    }
}

impl GovernanceAction {
    fn to_db(&self) -> DBGovernanceAction {
        DBGovernanceAction {
            nonce: self.nonce.map(|nonce| nonce as i64),
            data_source: self.data_source.to_string(),
            txn_digest: self.tx_digest.clone(),
            sender_address: self.sender.to_vec(),
            timestamp_ms: self.timestamp_ms as i64,
            action: self.action.to_string(),
            data: self.data.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) enum TokenTransferStatus {
    Deposited,
    Approved,
    Claimed,
}

impl Display for TokenTransferStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            TokenTransferStatus::Deposited => "Deposited",
            TokenTransferStatus::Approved => "Approved",
            TokenTransferStatus::Claimed => "Claimed",
        };
        write!(f, "{str}")
    }
}

#[derive(Clone, Display)]
pub(crate) enum GovernanceActionType {
    UpdateCommitteeBlocklist,
    EmergencyOperation,
    UpdateBridgeLimit,
    UpdateTokenPrices,
    UpgradeEVMContract,
    AddSuiTokens,
    AddEVMTokens,
}

#[derive(Clone)]
enum BridgeDataSource {
    Sui,
    Eth,
}

impl Display for BridgeDataSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            BridgeDataSource::Eth => "ETH",
            BridgeDataSource::Sui => "SUI",
        };
        write!(f, "{str}")
    }
}
