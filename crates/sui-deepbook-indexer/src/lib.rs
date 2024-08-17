// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bigdecimal::BigDecimal;
use sui_types::base_types::{SuiAddress, TransactionDigest};

use std::fmt::{Display, Formatter};

use crate::models::Flashloan as DBFlashloan;
use crate::models::OrderFill as DBOrderFill;
use crate::models::OrderUpdate as DBOrderUpdate;
use crate::models::PoolPrice as DBPoolPrice;
use crate::models::SuiErrorTransactions;

pub mod config;
pub mod events;
pub mod metrics;
pub mod models;
pub mod postgres_manager;
pub mod schema;
pub mod types;

pub mod sui_deepbook_indexer;

#[derive(Clone)]
pub enum ProcessedTxnData {
    Flashloan(Flashloan),
    OrderUpdate(OrderUpdate),
    OrderFill(OrderFill),
    PoolPrice(PoolPrice),
    Error(SuiTxnError),
}

#[derive(Clone)]
pub(crate) enum OrderUpdateStatus {
    Placed,
    Modified,
    Canceled,
}

impl Display for OrderUpdateStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            OrderUpdateStatus::Placed => "Placed",
            OrderUpdateStatus::Modified => "Modified",
            OrderUpdateStatus::Canceled => "Canceled",
        };
        write!(f, "{str}")
    }
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
pub struct OrderUpdate {
    digest: String,
    sender: String,
    checkpoint: u64,
    status: OrderUpdateStatus,
    pool_id: String,
    order_id: u128,
    client_order_id: u64,
    price: u64,
    is_bid: bool,
    quantity: u64,
    onchain_timestamp: u64,
    trader: String,
    balance_manager_id: String,
}

impl OrderUpdate {
    fn to_db(&self) -> DBOrderUpdate {
        DBOrderUpdate {
            digest: self.digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            status: self.status.clone().to_string(),
            pool_id: self.pool_id.clone(),
            order_id: BigDecimal::from(self.order_id.clone()),
            client_order_id: self.client_order_id.clone() as i64,
            trader: self.trader.clone(),
            price: self.price as i64,
            is_bid: self.is_bid,
            quantity: self.quantity.clone() as i64,
            onchain_timestamp: self.onchain_timestamp as i64,
            balance_manager_id: self.balance_manager_id.clone(),
        }
    }
}

#[derive(Clone)]
pub struct OrderFill {
    digest: String,
    sender: String,
    checkpoint: u64,
    pool_id: String,
    maker_order_id: u128,
    taker_order_id: u128,
    maker_client_order_id: u64,
    taker_client_order_id: u64,
    price: u64,
    taker_is_bid: bool,
    base_quantity: u64,
    quote_quantity: u64,
    maker_balance_manager_id: String,
    taker_balance_manager_id: String,
    onchain_timestamp: u64,
}

impl OrderFill {
    fn to_db(&self) -> DBOrderFill {
        println!("order id: {}", self.maker_order_id);
        println!("digest: {}", self.digest);
        DBOrderFill {
            digest: self.digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            pool_id: self.pool_id.clone(),
            maker_order_id: BigDecimal::from(self.maker_order_id.clone()),
            taker_order_id: BigDecimal::from(self.taker_order_id.clone()),
            maker_client_order_id: self.maker_client_order_id as i64,
            taker_client_order_id: self.taker_client_order_id as i64,
            price: self.price as i64,
            taker_is_bid: self.taker_is_bid,
            base_quantity: self.base_quantity as i64,
            quote_quantity: self.quote_quantity as i64,
            maker_balance_manager_id: self.maker_balance_manager_id.clone(),
            taker_balance_manager_id: self.taker_balance_manager_id.clone(),
            onchain_timestamp: self.onchain_timestamp as i64,
        }
    }
}

#[derive(Clone)]
pub struct Flashloan {
    digest: String,
    sender: String,
    checkpoint: u64,
    borrow: bool,
    pool_id: String,
    borrow_quantity: u64,
    type_name: String,
}

impl Flashloan {
    fn to_db(&self) -> DBFlashloan {
        DBFlashloan {
            digest: self.digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            borrow: self.borrow,
            pool_id: self.pool_id.clone(),
            borrow_quantity: self.borrow_quantity as i64,
            type_name: self.type_name.clone(),
        }
    }
}

#[derive(Clone)]
pub struct PoolPrice {
    digest: String,
    sender: String,
    checkpoint: u64,
    target_pool: String,
    reference_pool: String,
    conversion_rate: u64,
}

impl PoolPrice {
    fn to_db(&self) -> DBPoolPrice {
        DBPoolPrice {
            digest: self.digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            target_pool: self.target_pool.clone(),
            reference_pool: self.reference_pool.clone(),
            conversion_rate: self.conversion_rate as i64,
        }
    }
}

impl SuiTxnError {
    fn to_db(&self) -> SuiErrorTransactions {
        SuiErrorTransactions {
            txn_digest: self.tx_digest.to_string(),
            sender_address: self.sender.to_string(),
            timestamp_ms: self.timestamp_ms as i64,
            failure_status: self.failure_status.clone(),
            cmd_idx: self.cmd_idx.map(|idx| idx as i64),
        }
    }
}
