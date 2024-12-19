// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_sdk::types::base_types::{ObjectID, SuiAddress};

/// Represents a balance manager in the system
#[derive(Debug, Clone)]
pub struct BalanceManager {
    pub address: String,
    pub trade_cap: Option<String>,
}

/// Represents a coin in the system
#[derive(Debug, Clone)]
pub struct Coin {
    pub address: String,
    pub type_name: String,
    pub scalar: u64,
}

/// Represents a trading pool
#[derive(Debug, Clone)]
pub struct Pool {
    pub address: String,
    pub base_coin: String,
    pub quote_coin: String,
}

#[derive(Debug, Clone)]
pub struct DeepBookPackageIds {
    pub deepbook_package_id: &'static str,
    pub registry_id: &'static str,
    pub deep_treasury_id: &'static str,
}

/// Trading order types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderType {
    NoRestriction,
    ImmediateOrCancel,
    FillOrKill,
    PostOnly,
}

/// Self-matching options for orders
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelfMatchingOptions {
    SelfMatchingAllowed,
    CancelTaker,
    CancelMaker,
}

/// Parameters for placing a limit order
#[derive(Debug, Clone)]
pub struct PlaceLimitOrderParams {
    pub pool_key: String,
    pub balance_manager_key: String,
    pub client_order_id: String,
    pub price: f64,
    pub quantity: f64,
    pub is_bid: bool,
    pub expiration: Option<u64>,
    pub order_type: Option<OrderType>,
    pub self_matching_option: Option<SelfMatchingOptions>,
    pub pay_with_deep: Option<bool>,
}

/// Parameters for placing a market order
#[derive(Debug, Clone)]
pub struct PlaceMarketOrderParams {
    pub pool_key: String,
    pub balance_manager_key: String,
    pub client_order_id: String,
    pub quantity: f64,
    pub is_bid: bool,
    pub self_matching_option: Option<SelfMatchingOptions>,
    pub pay_with_deep: Option<bool>,
}

/// Parameters for submitting a proposal
#[derive(Debug, Clone)]
pub struct ProposalParams {
    pub pool_key: String,
    pub balance_manager_key: String,
    pub taker_fee: f64,
    pub maker_fee: f64,
    pub stake_required: f64,
}

/// Parameters for swap operations
#[derive(Debug, Clone)]
pub struct SwapParams {
    pub sender: SuiAddress,
    pub pool_key: String,
    pub amount: f64,
    pub deep_amount: f64,
    pub min_out: f64,
    pub deep_coin: Option<sui_json_rpc_types::Coin>,
    pub base_coin: Option<sui_json_rpc_types::Coin>,
    pub quote_coin: Option<sui_json_rpc_types::Coin>,
}

/// Parameters for creating a pool admin
#[derive(Debug, Clone)]
pub struct CreatePoolAdminParams {
    pub base_coin_key: String,
    pub quote_coin_key: String,
    pub tick_size: f64,
    pub lot_size: f64,
    pub min_size: f64,
    pub whitelisted: bool,
    pub stable_pool: bool,
    pub deep_coin: Option<ObjectID>,
    pub base_coin: Option<ObjectID>,
}

/// Configuration for the DeepBook system
#[derive(Debug, Clone)]
pub struct Config {
    pub deepbook_package_id: String,
    pub registry_id: String,
    pub deep_treasury_id: String,
}

/// Environment type for the system
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Environment {
    Mainnet,
    Testnet,
}
