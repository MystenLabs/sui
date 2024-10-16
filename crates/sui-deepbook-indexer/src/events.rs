// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_types::base_types::{ObjectID, SuiAddress};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveOrderFilledEvent {
    pub pool_id: ObjectID,
    pub maker_order_id: u128,
    pub taker_order_id: u128,
    pub maker_client_order_id: u64,
    pub taker_client_order_id: u64,
    pub price: u64,
    pub taker_is_bid: bool,
    pub taker_fee: u64,
    pub taker_fee_is_deep: bool,
    pub maker_fee: u64,
    pub maker_fee_is_deep: bool,
    pub base_quantity: u64,
    pub quote_quantity: u64,
    pub maker_balance_manager_id: ObjectID,
    pub taker_balance_manager_id: ObjectID,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveOrderCanceledEvent {
    pub balance_manager_id: ObjectID,
    pub pool_id: ObjectID,
    pub order_id: u128,
    pub client_order_id: u64,
    pub trader: SuiAddress,
    pub price: u64,
    pub is_bid: bool,
    pub original_quantity: u64,
    pub base_asset_quantity_canceled: u64,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveOrderExpiredEvent {
    pub balance_manager_id: ObjectID,
    pub pool_id: ObjectID,
    pub order_id: u128,
    pub client_order_id: u64,
    pub trader: SuiAddress,
    pub price: u64,
    pub is_bid: bool,
    pub original_quantity: u64,
    pub base_asset_quantity_canceled: u64,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveOrderModifiedEvent {
    pub balance_manager_id: ObjectID,
    pub pool_id: ObjectID,
    pub order_id: u128,
    pub client_order_id: u64,
    pub trader: SuiAddress,
    pub price: u64,
    pub is_bid: bool,
    pub previous_quantity: u64,
    pub filled_quantity: u64,
    pub new_quantity: u64,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveOrderPlacedEvent {
    pub balance_manager_id: ObjectID,
    pub pool_id: ObjectID,
    pub order_id: u128,
    pub client_order_id: u64,
    pub trader: SuiAddress,
    pub price: u64,
    pub is_bid: bool,
    pub placed_quantity: u64,
    pub expire_timestamp: u64,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MovePriceAddedEvent {
    pub conversion_rate: u64,
    pub timestamp: u64,
    pub is_base_conversion: bool,
    pub reference_pool: ObjectID,
    pub target_pool: ObjectID,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveFlashLoanBorrowedEvent {
    pub pool_id: ObjectID,
    pub borrow_quantity: u64,
    pub type_name: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveBalanceEvent {
    pub balance_manager_id: ObjectID,
    pub asset: String,
    pub amount: u64,
    pub deposit: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveTradeParamsUpdateEvent {
    pub taker_fee: u64,
    pub maker_fee: u64,
    pub stake_required: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveStakeEvent {
    pub pool_id: ObjectID,
    pub balance_manager_id: ObjectID,
    pub epoch: u64,
    pub amount: u64,
    pub stake: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveProposalEvent {
    pub pool_id: ObjectID,
    pub balance_manager_id: ObjectID,
    pub epoch: u64,
    pub taker_fee: u64,
    pub maker_fee: u64,
    pub stake_required: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveVoteEvent {
    pub pool_id: ObjectID,
    pub balance_manager_id: ObjectID,
    pub epoch: u64,
    pub from_proposal_id: Option<ObjectID>,
    pub to_proposal_id: ObjectID,
    pub stake: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveRebateEvent {
    pub pool_id: ObjectID,
    pub balance_manager_id: ObjectID,
    pub epoch: u64,
    pub claim_amount: u64,
}
