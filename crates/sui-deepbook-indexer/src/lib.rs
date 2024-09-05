// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bigdecimal::BigDecimal;
use sui_types::base_types::{SuiAddress, TransactionDigest};

use std::fmt::{Display, Formatter};

use crate::models::Balances as DBBalances;
use crate::models::Flashloan as DBFlashloan;
use crate::models::OrderFill as DBOrderFill;
use crate::models::OrderUpdate as DBOrderUpdate;
use crate::models::PoolPrice as DBPoolPrice;
use crate::models::Proposals as DBProposals;
use crate::models::Rebates as DBRebates;
use crate::models::Stakes as DBStakes;
use crate::models::SuiErrorTransactions;
use crate::models::TradeParamsUpdate as DBTradeParamsUpdate;
use crate::models::Votes as DBVotes;

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
    Balances(Balances),
    Proposals(Proposals),
    Rebates(Rebates),
    Stakes(Stakes),
    TradeParamsUpdate(TradeParamsUpdate),
    Votes(Votes),
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
pub struct OrderUpdate {
    digest: String,
    sender: String,
    checkpoint: u64,
    package: String,
    status: OrderUpdateStatus,
    pool_id: String,
    order_id: u128,
    client_order_id: u64,
    price: u64,
    is_bid: bool,
    original_quantity: u64,
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
            package: self.package.clone(),
            status: self.status.clone().to_string(),
            pool_id: self.pool_id.clone(),
            order_id: BigDecimal::from(self.order_id),
            client_order_id: self.client_order_id as i64,
            trader: self.trader.clone(),
            price: self.price as i64,
            is_bid: self.is_bid,
            original_quantity: self.original_quantity as i64,
            quantity: self.quantity as i64,
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
    package: String,
    pool_id: String,
    maker_order_id: u128,
    taker_order_id: u128,
    maker_client_order_id: u64,
    taker_client_order_id: u64,
    price: u64,
    taker_is_bid: bool,
    taker_fee: u64,
    maker_fee: u64,
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
            package: self.package.clone(),
            pool_id: self.pool_id.clone(),
            maker_order_id: BigDecimal::from(self.maker_order_id),
            taker_order_id: BigDecimal::from(self.taker_order_id),
            maker_client_order_id: self.maker_client_order_id as i64,
            taker_client_order_id: self.taker_client_order_id as i64,
            price: self.price as i64,
            taker_fee: self.taker_fee as i64,
            maker_fee: self.maker_fee as i64,
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
    package: String,
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
            package: self.package.clone(),
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
    package: String,
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
            package: self.package.clone(),
            target_pool: self.target_pool.clone(),
            reference_pool: self.reference_pool.clone(),
            conversion_rate: self.conversion_rate as i64,
        }
    }
}

#[derive(Clone)]
pub struct Balances {
    digest: String,
    sender: String,
    checkpoint: u64,
    package: String,
    balance_manager_id: String,
    asset: String,
    amount: u64,
    deposit: bool,
}

impl Balances {
    fn to_db(&self) -> DBBalances {
        DBBalances {
            digest: self.digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            package: self.package.clone(),
            balance_manager_id: self.balance_manager_id.clone(),
            asset: self.asset.clone(),
            amount: self.amount as i64,
            deposit: self.deposit,
        }
    }
}

#[derive(Clone)]
pub struct Proposals {
    digest: String,
    sender: String,
    checkpoint: u64,
    package: String,
    balance_manager_id: String,
    epoch: u64,
    taker_fee: u64,
    maker_fee: u64,
    stake_required: u64,
}

impl Proposals {
    fn to_db(&self) -> DBProposals {
        DBProposals {
            digest: self.digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            package: self.package.clone(),
            balance_manager_id: self.balance_manager_id.clone(),
            epoch: self.epoch as i64,
            taker_fee: self.taker_fee as i64,
            maker_fee: self.maker_fee as i64,
            stake_required: self.stake_required as i64,
        }
    }
}

#[derive(Clone)]
pub struct Rebates {
    digest: String,
    sender: String,
    checkpoint: u64,
    package: String,
    pool_id: String,
    balance_manager_id: String,
    epoch: u64,
    claim_amount: u64,
}

impl Rebates {
    fn to_db(&self) -> DBRebates {
        DBRebates {
            digest: self.digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            package: self.package.clone(),
            pool_id: self.pool_id.clone(),
            balance_manager_id: self.balance_manager_id.clone(),
            epoch: self.epoch as i64,
            claim_amount: self.claim_amount as i64,
        }
    }
}

#[derive(Clone)]
pub struct Stakes {
    digest: String,
    sender: String,
    checkpoint: u64,
    package: String,
    pool_id: String,
    balance_manager_id: String,
    epoch: u64,
    amount: u64,
    stake: bool,
}

impl Stakes {
    fn to_db(&self) -> DBStakes {
        DBStakes {
            digest: self.digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            package: self.package.clone(),
            pool_id: self.pool_id.clone(),
            balance_manager_id: self.balance_manager_id.clone(),
            epoch: self.epoch as i64,
            amount: self.amount as i64,
            stake: self.stake,
        }
    }
}

#[derive(Clone)]
pub struct TradeParamsUpdate {
    digest: String,
    sender: String,
    checkpoint: u64,
    package: String,
    pool_id: String,
    taker_fee: u64,
    maker_fee: u64,
    stake_required: u64,
}

impl TradeParamsUpdate {
    fn to_db(&self) -> DBTradeParamsUpdate {
        DBTradeParamsUpdate {
            digest: self.digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            package: self.package.clone(),
            pool_id: self.pool_id.clone(),
            taker_fee: self.taker_fee as i64,
            maker_fee: self.maker_fee as i64,
            stake_required: self.stake_required as i64,
        }
    }
}

#[derive(Clone)]
pub struct Votes {
    digest: String,
    sender: String,
    checkpoint: u64,
    package: String,
    pool_id: String,
    balance_manager_id: String,
    epoch: u64,
    from_proposal_id: Option<String>,
    to_proposal_id: String,
    stake: u64,
}

impl Votes {
    fn to_db(&self) -> DBVotes {
        DBVotes {
            digest: self.digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            package: self.package.clone(),
            pool_id: self.pool_id.clone(),
            balance_manager_id: self.balance_manager_id.clone(),
            epoch: self.epoch as i64,
            from_proposal_id: self.from_proposal_id.clone(),
            to_proposal_id: self.to_proposal_id.clone(),
            stake: self.stake as i64,
        }
    }
}

#[derive(Clone)]
pub struct SuiTxnError {
    tx_digest: TransactionDigest,
    sender: SuiAddress,
    timestamp_ms: u64,
    failure_status: String,
    package: String,
    cmd_idx: Option<u64>,
}

impl SuiTxnError {
    fn to_db(&self) -> SuiErrorTransactions {
        SuiErrorTransactions {
            txn_digest: self.tx_digest.to_string(),
            sender_address: self.sender.to_string(),
            timestamp_ms: self.timestamp_ms as i64,
            failure_status: self.failure_status.clone(),
            package: self.package.clone(),
            cmd_idx: self.cmd_idx.map(|idx| idx as i64),
        }
    }
}
