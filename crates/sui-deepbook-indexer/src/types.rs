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

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub(crate) enum OrderUpdateStatus {
    Placed,
    Modified,
    Canceled,
    Expired,
}

impl Display for OrderUpdateStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            OrderUpdateStatus::Placed => "Placed",
            OrderUpdateStatus::Modified => "Modified",
            OrderUpdateStatus::Canceled => "Canceled",
            OrderUpdateStatus::Expired => "Expired",
        };
        write!(f, "{str}")
    }
}

#[derive(Clone, Debug)]
pub struct OrderUpdate {
    pub digest: String,
    pub(crate) event_digest: String,
    pub(crate) sender: String,
    pub(crate) checkpoint: u64,
    pub(crate) checkpoint_timestamp_ms: u64,
    pub(crate) package: String,
    pub(crate) status: OrderUpdateStatus,
    pub(crate) pool_id: String,
    pub(crate) order_id: u128,
    pub(crate) client_order_id: u64,
    pub(crate) price: u64,
    pub(crate) is_bid: bool,
    pub(crate) original_quantity: u64,
    pub(crate) quantity: u64,
    pub(crate) filled_quantity: u64,
    pub(crate) onchain_timestamp: u64,
    pub(crate) trader: String,
    pub(crate) balance_manager_id: String,
}

impl OrderUpdate {
    pub(crate) fn to_db(&self) -> DBOrderUpdate {
        DBOrderUpdate {
            digest: self.digest.clone(),
            event_digest: self.event_digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            checkpoint_timestamp_ms: self.checkpoint_timestamp_ms as i64,
            package: self.package.clone(),
            status: self.status.clone().to_string(),
            pool_id: self.pool_id.clone(),
            order_id: BigDecimal::from(self.order_id).to_string(),
            client_order_id: self.client_order_id as i64,
            trader: self.trader.clone(),
            price: self.price as i64,
            is_bid: self.is_bid,
            original_quantity: self.original_quantity as i64,
            quantity: self.quantity as i64,
            filled_quantity: self.filled_quantity as i64,
            onchain_timestamp: self.onchain_timestamp as i64,
            balance_manager_id: self.balance_manager_id.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct OrderFill {
    pub(crate) digest: String,
    pub(crate) event_digest: String,
    pub(crate) sender: String,
    pub(crate) checkpoint: u64,
    pub(crate) checkpoint_timestamp_ms: u64,
    pub(crate) package: String,
    pub(crate) pool_id: String,
    pub(crate) maker_order_id: u128,
    pub(crate) taker_order_id: u128,
    pub(crate) maker_client_order_id: u64,
    pub(crate) taker_client_order_id: u64,
    pub(crate) price: u64,
    pub(crate) taker_is_bid: bool,
    pub(crate) taker_fee: u64,
    pub(crate) taker_fee_is_deep: bool,
    pub(crate) maker_fee: u64,
    pub(crate) maker_fee_is_deep: bool,
    pub(crate) base_quantity: u64,
    pub(crate) quote_quantity: u64,
    pub(crate) maker_balance_manager_id: String,
    pub(crate) taker_balance_manager_id: String,
    pub(crate) onchain_timestamp: u64,
}

impl OrderFill {
    pub(crate) fn to_db(&self) -> DBOrderFill {
        DBOrderFill {
            digest: self.digest.clone(),
            event_digest: self.event_digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            checkpoint_timestamp_ms: self.checkpoint_timestamp_ms as i64,
            package: self.package.clone(),
            pool_id: self.pool_id.clone(),
            maker_order_id: BigDecimal::from(self.maker_order_id).to_string(),
            taker_order_id: BigDecimal::from(self.taker_order_id).to_string(),
            maker_client_order_id: self.maker_client_order_id as i64,
            taker_client_order_id: self.taker_client_order_id as i64,
            price: self.price as i64,
            taker_fee: self.taker_fee as i64,
            taker_fee_is_deep: self.taker_fee_is_deep,
            maker_fee: self.maker_fee as i64,
            maker_fee_is_deep: self.maker_fee_is_deep,
            taker_is_bid: self.taker_is_bid,
            base_quantity: self.base_quantity as i64,
            quote_quantity: self.quote_quantity as i64,
            maker_balance_manager_id: self.maker_balance_manager_id.clone(),
            taker_balance_manager_id: self.taker_balance_manager_id.clone(),
            onchain_timestamp: self.onchain_timestamp as i64,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Flashloan {
    pub(crate) digest: String,
    pub(crate) event_digest: String,
    pub(crate) sender: String,
    pub(crate) checkpoint: u64,
    pub(crate) checkpoint_timestamp_ms: u64,
    pub(crate) package: String,
    pub(crate) borrow: bool,
    pub(crate) pool_id: String,
    pub(crate) borrow_quantity: u64,
    pub(crate) type_name: String,
}

impl Flashloan {
    pub(crate) fn to_db(&self) -> DBFlashloan {
        DBFlashloan {
            digest: self.digest.clone(),
            event_digest: self.event_digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            checkpoint_timestamp_ms: self.checkpoint_timestamp_ms as i64,
            package: self.package.clone(),
            borrow: self.borrow,
            pool_id: self.pool_id.clone(),
            borrow_quantity: self.borrow_quantity as i64,
            type_name: self.type_name.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PoolPrice {
    pub(crate) digest: String,
    pub(crate) event_digest: String,
    pub(crate) sender: String,
    pub(crate) checkpoint: u64,
    pub(crate) checkpoint_timestamp_ms: u64,
    pub(crate) package: String,
    pub(crate) target_pool: String,
    pub(crate) reference_pool: String,
    pub(crate) conversion_rate: u64,
}

impl PoolPrice {
    pub(crate) fn to_db(&self) -> DBPoolPrice {
        DBPoolPrice {
            digest: self.digest.clone(),
            event_digest: self.event_digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            checkpoint_timestamp_ms: self.checkpoint_timestamp_ms as i64,
            package: self.package.clone(),
            target_pool: self.target_pool.clone(),
            reference_pool: self.reference_pool.clone(),
            conversion_rate: self.conversion_rate as i64,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Balances {
    pub(crate) digest: String,
    pub(crate) event_digest: String,
    pub(crate) sender: String,
    pub(crate) checkpoint: u64,
    pub(crate) checkpoint_timestamp_ms: u64,
    pub(crate) package: String,
    pub(crate) balance_manager_id: String,
    pub(crate) asset: String,
    pub(crate) amount: u64,
    pub(crate) deposit: bool,
}

impl Balances {
    pub(crate) fn to_db(&self) -> DBBalances {
        DBBalances {
            digest: self.digest.clone(),
            event_digest: self.event_digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            checkpoint_timestamp_ms: self.checkpoint_timestamp_ms as i64,
            package: self.package.clone(),
            balance_manager_id: self.balance_manager_id.clone(),
            asset: self.asset.clone(),
            amount: self.amount as i64,
            deposit: self.deposit,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Proposals {
    pub(crate) digest: String,
    pub(crate) event_digest: String,
    pub(crate) sender: String,
    pub(crate) checkpoint: u64,
    pub(crate) checkpoint_timestamp_ms: u64,
    pub(crate) package: String,
    pub(crate) pool_id: String,
    pub(crate) balance_manager_id: String,
    pub(crate) epoch: u64,
    pub(crate) taker_fee: u64,
    pub(crate) maker_fee: u64,
    pub(crate) stake_required: u64,
}

impl Proposals {
    pub(crate) fn to_db(&self) -> DBProposals {
        DBProposals {
            digest: self.digest.clone(),
            event_digest: self.event_digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            checkpoint_timestamp_ms: self.checkpoint_timestamp_ms as i64,
            package: self.package.clone(),
            pool_id: self.pool_id.clone(),
            balance_manager_id: self.balance_manager_id.clone(),
            epoch: self.epoch as i64,
            taker_fee: self.taker_fee as i64,
            maker_fee: self.maker_fee as i64,
            stake_required: self.stake_required as i64,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Rebates {
    pub(crate) digest: String,
    pub(crate) event_digest: String,
    pub(crate) sender: String,
    pub(crate) checkpoint: u64,
    pub(crate) checkpoint_timestamp_ms: u64,
    pub(crate) package: String,
    pub(crate) pool_id: String,
    pub(crate) balance_manager_id: String,
    pub(crate) epoch: u64,
    pub(crate) claim_amount: u64,
}

impl Rebates {
    pub(crate) fn to_db(&self) -> DBRebates {
        DBRebates {
            digest: self.digest.clone(),
            event_digest: self.event_digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            checkpoint_timestamp_ms: self.checkpoint_timestamp_ms as i64,
            package: self.package.clone(),
            pool_id: self.pool_id.clone(),
            balance_manager_id: self.balance_manager_id.clone(),
            epoch: self.epoch as i64,
            claim_amount: self.claim_amount as i64,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Stakes {
    pub(crate) digest: String,
    pub(crate) event_digest: String,
    pub(crate) sender: String,
    pub(crate) checkpoint: u64,
    pub(crate) checkpoint_timestamp_ms: u64,
    pub(crate) package: String,
    pub(crate) pool_id: String,
    pub(crate) balance_manager_id: String,
    pub(crate) epoch: u64,
    pub(crate) amount: u64,
    pub(crate) stake: bool,
}

impl Stakes {
    pub(crate) fn to_db(&self) -> DBStakes {
        DBStakes {
            digest: self.digest.clone(),
            event_digest: self.event_digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            checkpoint_timestamp_ms: self.checkpoint_timestamp_ms as i64,
            package: self.package.clone(),
            pool_id: self.pool_id.clone(),
            balance_manager_id: self.balance_manager_id.clone(),
            epoch: self.epoch as i64,
            amount: self.amount as i64,
            stake: self.stake,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TradeParamsUpdate {
    pub(crate) digest: String,
    pub(crate) event_digest: String,
    pub(crate) sender: String,
    pub(crate) checkpoint: u64,
    pub(crate) checkpoint_timestamp_ms: u64,
    pub(crate) package: String,
    pub(crate) pool_id: String,
    pub(crate) taker_fee: u64,
    pub(crate) maker_fee: u64,
    pub(crate) stake_required: u64,
}

impl TradeParamsUpdate {
    pub(crate) fn to_db(&self) -> DBTradeParamsUpdate {
        DBTradeParamsUpdate {
            digest: self.digest.clone(),
            event_digest: self.event_digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            checkpoint_timestamp_ms: self.checkpoint_timestamp_ms as i64,
            package: self.package.clone(),
            pool_id: self.pool_id.clone(),
            taker_fee: self.taker_fee as i64,
            maker_fee: self.maker_fee as i64,
            stake_required: self.stake_required as i64,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Votes {
    pub(crate) digest: String,
    pub(crate) event_digest: String,
    pub(crate) sender: String,
    pub(crate) checkpoint: u64,
    pub(crate) checkpoint_timestamp_ms: u64,
    pub(crate) package: String,
    pub(crate) pool_id: String,
    pub(crate) balance_manager_id: String,
    pub(crate) epoch: u64,
    pub(crate) from_proposal_id: Option<String>,
    pub(crate) to_proposal_id: String,
    pub(crate) stake: u64,
}

impl Votes {
    pub(crate) fn to_db(&self) -> DBVotes {
        DBVotes {
            digest: self.digest.clone(),
            event_digest: self.event_digest.clone(),
            sender: self.sender.clone(),
            checkpoint: self.checkpoint as i64,
            checkpoint_timestamp_ms: self.checkpoint_timestamp_ms as i64,
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

#[derive(Clone, Debug)]
pub struct SuiTxnError {
    pub(crate) tx_digest: TransactionDigest,
    pub(crate) sender: SuiAddress,
    pub(crate) timestamp_ms: u64,
    pub(crate) failure_status: String,
    pub(crate) package: String,
    pub(crate) cmd_idx: Option<u64>,
}

impl SuiTxnError {
    pub(crate) fn to_db(&self) -> SuiErrorTransactions {
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
