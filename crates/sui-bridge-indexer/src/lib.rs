// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::TokenTransfer as DBTokenTransfer;
use crate::models::TokenTransferData as DBTokenTransferData;
use anyhow::anyhow;
use std::fmt::{Display, Formatter};

pub mod config;
pub mod models;
pub mod postgres_writer;
pub mod schema;
pub mod worker;

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
}

pub struct TokenTransferData {
    sender_address: Vec<u8>,
    destination_chain: u8,
    recipient_address: Vec<u8>,
    token_id: u8,
    amount: u64,
}

impl From<TokenTransfer> for DBTokenTransfer {
    fn from(value: TokenTransfer) -> Self {
        DBTokenTransfer {
            chain_id: value.chain_id as i32,
            nonce: value.nonce as i64,
            block_height: value.block_height as i64,
            timestamp_ms: value.timestamp_ms as i64,
            txn_hash: value.txn_hash,
            txn_sender: value.txn_sender.clone(),
            status: value.status.to_string(),
            gas_usage: value.gas_usage,
            data_source: value.data_source.to_string(),
        }
    }
}

impl TryFrom<&TokenTransfer> for DBTokenTransferData {
    type Error = anyhow::Error;

    fn try_from(value: &TokenTransfer) -> Result<Self, Self::Error> {
        value
            .data
            .as_ref()
            .ok_or(anyhow!(
                "Data is empty for TokenTransfer: chain_id = {}, nonce = {}, status = {}",
                value.chain_id,
                value.nonce,
                value.status
            ))
            .map(|data| DBTokenTransferData {
                chain_id: value.chain_id as i32,
                nonce: value.nonce as i64,
                block_height: value.block_height as i64,
                timestamp_ms: value.timestamp_ms as i64,
                txn_hash: value.txn_hash.clone(),
                sender_address: data.sender_address.clone(),
                destination_chain: data.destination_chain as i32,
                recipient_address: data.recipient_address.clone(),
                token_id: data.token_id as i32,
                amount: data.amount as i64,
            })
    }
}

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
