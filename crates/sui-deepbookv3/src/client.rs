// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_sdk::SuiClient;

use crate::transactions::balance_manager::BalanceManagerContract;
use crate::transactions::deepbook::DeepBookContract;
use crate::transactions::deepbook_admin::DeepBookAdminContract;
use crate::transactions::flashloan::FlashLoanContract;
use crate::transactions::governance::GovernanceContract;
use crate::utils::config::{
    BalanceManagerMap, CoinMap, DeepBookConfig, Environment, PoolMap, DEEP_SCALAR, FLOAT_SCALAR,
};
use crate::DataReader;

#[derive(Debug, Serialize, Deserialize)]
pub struct QuoteQuantityOut {
    pub base_quantity: f64,
    pub base_out: f64,
    pub quote_out: f64,
    pub deep_required: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QuantityOut {
    pub base_quantity: f64,
    pub quote_quantity: f64,
    pub base_out: f64,
    pub quote_out: f64,
    pub deep_required: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ID {
    pub bytes: SuiAddress,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderDeepPrice {
    pub asset_is_base: bool,
    pub deep_per_asset: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Order {
    pub balance_manager_id: ID,
    pub order_id: u128,
    pub client_order_id: u64,
    pub quantity: u64,
    pub filled_quantity: u64,
    pub fee_is_deep: bool,
    pub order_deep_price: OrderDeepPrice,
    pub epoch: u64,
    pub status: u8,
    pub expire_timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NormalizedOrder {
    pub balance_manager_id: ID,
    pub order_id: u128,
    pub client_order_id: u64,
    pub quantity: String,
    pub filled_quantity: String,
    pub fee_is_deep: bool,
    pub order_deep_price: NormalizedOrderDeepPrice,
    pub epoch: u64,
    pub status: u8,
    pub expire_timestamp: u64,
    pub is_bid: bool,
    pub normalized_price: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NormalizedOrderDeepPrice {
    pub asset_is_base: bool,
    pub deep_per_asset: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Level2Range {
    pub prices: Vec<f64>,
    pub quantities: Vec<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Level2TicksFromMid {
    pub bid_prices: Vec<f64>,
    pub bid_quantities: Vec<f64>,
    pub ask_prices: Vec<f64>,
    pub ask_quantities: Vec<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VaultBalances {
    pub base: f64,
    pub quote: f64,
    pub deep: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PoolTradeParams {
    pub taker_fee: f64,
    pub maker_fee: f64,
    pub stake_required: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PoolBookParams {
    pub tick_size: f64,
    pub lot_size: f64,
    pub min_size: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Balances {
    pub base: f64,
    pub quote: f64,
    pub deep: f64,
}

#[derive(Deserialize)]
struct RawOrderDeepPrice {
    asset_is_base: bool,
    deep_per_asset: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Account {
    pub epoch: u64,
    pub open_orders: Vec<u128>,
    pub taker_volume: f64,
    pub maker_volume: f64,
    pub active_stake: f64,
    pub inactive_stake: f64,
    pub created_proposal: bool,
    pub voted_proposal: Option<ID>,
    pub unclaimed_rebates: Balances,
    pub settled_balances: Balances,
    pub owed_balances: Balances,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PoolDeepPrice {
    pub asset_is_base: bool,
    pub deep_per_base: Option<f64>,
    pub deep_per_quote: Option<f64>,
}

/// DeepBookClient struct for managing DeepBook operations.
pub struct DeepBookClient {
    client: SuiClient,
    config: DeepBookConfig,
    address: SuiAddress,
    pub balance_manager: BalanceManagerContract,
    pub deep_book: DeepBookContract,
    pub deep_book_admin: DeepBookAdminContract,
    pub flash_loans: FlashLoanContract,
    pub governance: GovernanceContract,
}

impl DeepBookClient {
    /// Creates a new DeepBookClient instance
    ///
    /// @param client - The SuiClient instance
    /// @param address - The address of the DeepBook contract
    /// @param env - The environment of the DeepBook contract
    /// @param balance_managers - The balance managers associated with the DeepBook contract
    /// @param coins - The coins associated with the DeepBook contract
    /// @param pools - The pools associated with the DeepBook contract
    /// @param admin_cap - The admin cap associated with the DeepBook contract
    pub fn new(
        client: SuiClient,
        address: SuiAddress,
        env: Environment,
        balance_managers: Option<BalanceManagerMap>,
        coins: Option<CoinMap>,
        pools: Option<PoolMap>,
        admin_cap: Option<String>,
    ) -> Self {
        let config = DeepBookConfig::new(env, address, admin_cap, balance_managers, coins, pools);
        let balance_manager = BalanceManagerContract::new(client.clone(), config.clone());
        Self {
            client: client.clone(),
            address,
            config: config.clone(),
            balance_manager: balance_manager.clone(),
            deep_book: DeepBookContract::new(
                client.clone(),
                config.clone(),
                balance_manager.clone(),
            ),
            deep_book_admin: DeepBookAdminContract::new(client.clone(), config.clone()),
            flash_loans: FlashLoanContract::new(client.clone(), config.clone()),
            governance: GovernanceContract::new(
                client.clone(),
                config.clone(),
                balance_manager.clone(),
            ),
        }
    }

    /// Check the balance of a balance manager for a specific coin
    ///
    /// @param manager_key - The key of the balance manager
    /// @param coin_key - The key of the coin
    pub async fn check_manager_balance(
        &self,
        manager_key: &str,
        coin_key: &str,
    ) -> anyhow::Result<(String, f64)> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let coin = self.config.get_coin(coin_key)?;

        self.balance_manager
            .check_manager_balance(&mut ptb, manager_key, coin_key)
            .await?;
        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(res) => {
                let res = res
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let balance = bcs::from_bytes::<u64>(&res.0)?;
                let adjusted_balance = balance as f64 / coin.scalar as f64;

                Ok((
                    coin.type_name.clone(),
                    (adjusted_balance * 1e9).round() / 1e9,
                ))
            }
            Err(e) => Err(e),
        }
    }

    /// Check if a pool is whitelisted
    ///
    /// @param pool_key - The key of the pool
    pub async fn whitelisted(&self, pool_key: &str) -> anyhow::Result<bool> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        self.deep_book.whitelisted(&mut ptb, pool_key).await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(res) => {
                let res = res
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let whitelisted = bcs::from_bytes::<bool>(&res.0)?;
                Ok(whitelisted)
            }
            Err(e) => Err(e),
        }
    }

    /// Get the quote quantity out for a given base quantity
    ///
    /// @param pool_key - The key of the pool
    /// @param base_quantity - The base quantity to convert
    pub async fn get_quote_quantity_out(
        &self,
        pool_key: &str,
        base_quantity: f64,
    ) -> anyhow::Result<QuoteQuantityOut> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        self.deep_book
            .get_quote_quantity_out(&mut ptb, pool_key, base_quantity)
            .await?;

        self.get_quote_quantity_out_inner(ptb, pool_key, base_quantity)
            .await
    }

    /// Get the base quantity out for a given quote quantity
    ///
    /// @param pool_key - The key of the pool
    /// @param quote_quantity - The quote quantity to convert
    pub async fn get_base_quantity_out(
        &self,
        pool_key: &str,
        quote_quantity: f64,
    ) -> anyhow::Result<QuoteQuantityOut> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        self.deep_book
            .get_base_quantity_out(&mut ptb, pool_key, quote_quantity)
            .await?;
        self.get_quote_quantity_out_inner(ptb, pool_key, quote_quantity)
            .await
    }

    /// Get the output quantities for given base and quote quantities. Only one quantity can be non-zero
    ///
    /// @param pool_key - The key of the pool
    /// @param base_quantity - Base quantity to convert
    /// @param quote_quantity - Quote quantity to convert
    pub async fn get_quantity_out(
        &self,
        pool_key: &str,
        base_quantity: f64,
        quote_quantity: f64,
    ) -> anyhow::Result<QuantityOut> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        self.deep_book
            .get_quantity_out(&mut ptb, pool_key, base_quantity, quote_quantity)
            .await?;
        let result = self
            .get_quote_quantity_out_inner(ptb, pool_key, quote_quantity)
            .await?;
        Ok(QuantityOut {
            base_quantity,
            quote_quantity,
            base_out: result.base_out,
            quote_out: result.quote_out,
            deep_required: result.deep_required,
        })
    }

    /// Get open orders for a balance manager in a pool
    ///
    /// @param pool_key - The key of the pool
    /// @param manager_key - The key of the balance manager
    pub async fn account_open_orders(
        &self,
        pool_key: &str,
        manager_key: &str,
    ) -> anyhow::Result<Vec<u128>> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        self.deep_book
            .account_open_orders(&mut ptb, pool_key, manager_key)
            .await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(res) => {
                let res = res
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let order_ids = bcs::from_bytes::<Vec<u128>>(&res.0)?;
                Ok(order_ids)
            }
            Err(e) => Err(e),
        }
    }

    /// Get the order information for a specific order in a pool
    ///
    /// @param pool_key - The key of the pool
    /// @param order_id - The order ID
    pub async fn get_order(&self, pool_key: &str, order_id: u128) -> anyhow::Result<Option<Order>> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        self.deep_book
            .get_order(&mut ptb, pool_key, order_id)
            .await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(res) => {
                let res = res
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let order = bcs::from_bytes::<Order>(&res.0)?;
                Ok(Some(order))
            }
            Err(e) => Err(e),
        }
    }

    /// Get the order information for a specific order in a pool, with normalized price
    ///
    /// @param pool_key - The key of the pool
    /// @param order_id - The order ID
    pub async fn get_order_normalized(
        &self,
        pool_key: &str,
        order_id: u128,
    ) -> anyhow::Result<Option<NormalizedOrder>> {
        let order = match self.get_order(pool_key, order_id).await? {
            Some(order) => order,
            None => return Ok(None),
        };

        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let (is_bid, raw_price, _order_id) = self.decode_order_id(order.order_id)?;
        let normalized_price = (raw_price as f64 * base_coin.scalar as f64)
            / (quote_coin.scalar as f64 * FLOAT_SCALAR as f64);

        Ok(Some(NormalizedOrder {
            balance_manager_id: order.balance_manager_id,
            order_id: order.order_id,
            client_order_id: order.client_order_id,
            quantity: format!("{:.9}", order.quantity as f64 / base_coin.scalar as f64),
            filled_quantity: format!(
                "{:.9}",
                order.filled_quantity as f64 / base_coin.scalar as f64
            ),
            fee_is_deep: order.fee_is_deep,
            order_deep_price: NormalizedOrderDeepPrice {
                asset_is_base: order.order_deep_price.asset_is_base,
                deep_per_asset: format!(
                    "{:.9}",
                    order.order_deep_price.deep_per_asset as f64 / DEEP_SCALAR as f64
                ),
            },
            epoch: order.epoch,
            status: order.status,
            expire_timestamp: order.expire_timestamp,
            is_bid,
            normalized_price: format!("{:.9}", normalized_price),
        }))
    }

    /// Get multiple orders from a pool
    ///
    /// @param pool_key - The key of the pool
    /// @param order_ids - List of order IDs to retrieve
    pub async fn get_orders(
        &self,
        pool_key: &str,
        order_ids: Vec<String>,
    ) -> anyhow::Result<Option<Vec<Order>>> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        self.deep_book
            .get_orders(&mut ptb, pool_key, order_ids)
            .await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(res) => {
                let res = res
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let orders = bcs::from_bytes::<Vec<Order>>(&res.0)?;
                Ok(Some(orders))
            }
            Err(e) => Err(e),
        }
    }

    /// Get level 2 order book specifying range of price
    ///
    /// @param pool_key - Key of the pool
    /// @param price_low - Lower bound of the price range
    /// @param price_high - Upper bound of the price range
    /// @param is_bid - Whether to get bid or ask orders
    pub async fn get_level2_range(
        &self,
        pool_key: &str,
        price_low: f64,
        price_high: f64,
        is_bid: bool,
    ) -> anyhow::Result<Level2Range> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        self.deep_book
            .get_level2_range(&mut ptb, pool_key, price_low, price_high, is_bid)
            .await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(mut res) => {
                let prices = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let prices = bcs::from_bytes::<Vec<u64>>(&prices.0)?;

                let quantities = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let quantities = bcs::from_bytes::<Vec<u64>>(&quantities.0)?;

                Ok(Level2Range {
                    prices: prices
                        .into_iter()
                        .map(|price| {
                            ((price as f64 * base_coin.scalar as f64)
                                / (FLOAT_SCALAR as f64 * quote_coin.scalar as f64)
                                * 1e9)
                                .round()
                                / 1e9
                        })
                        .collect(),
                    quantities: quantities
                        .into_iter()
                        .map(|qty| ((qty as f64 / base_coin.scalar as f64) * 1e9).round() / 1e9)
                        .collect(),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Get level 2 order book ticks from mid-price for a pool
    ///
    /// @param pool_key - Key of the pool
    /// @param ticks - Number of ticks from mid-price
    pub async fn get_level2_ticks_from_mid(
        &self,
        pool_key: &str,
        ticks: u64,
    ) -> anyhow::Result<Level2TicksFromMid> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        self.deep_book
            .get_level2_ticks_from_mid(&mut ptb, pool_key, ticks)
            .await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(mut res) => {
                let bid_prices = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let bid_prices = bcs::from_bytes::<Vec<u64>>(&bid_prices.0)?;

                let bid_quantities = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let bid_quantities = bcs::from_bytes::<Vec<u64>>(&bid_quantities.0)?;

                let ask_prices = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let ask_prices = bcs::from_bytes::<Vec<u64>>(&ask_prices.0)?;

                let ask_quantities = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let ask_quantities = bcs::from_bytes::<Vec<u64>>(&ask_quantities.0)?;

                Ok(Level2TicksFromMid {
                    bid_prices: bid_prices
                        .into_iter()
                        .map(|price| {
                            ((price as f64 * base_coin.scalar as f64)
                                / (FLOAT_SCALAR as f64 * quote_coin.scalar as f64)
                                * 1e9)
                                .round()
                                / 1e9
                        })
                        .collect(),
                    bid_quantities: bid_quantities
                        .into_iter()
                        .map(|qty| ((qty as f64 / base_coin.scalar as f64) * 1e9).round() / 1e9)
                        .collect(),
                    ask_prices: ask_prices
                        .into_iter()
                        .map(|price| {
                            ((price as f64 * base_coin.scalar as f64)
                                / (FLOAT_SCALAR as f64 * quote_coin.scalar as f64)
                                * 1e9)
                                .round()
                                / 1e9
                        })
                        .collect(),
                    ask_quantities: ask_quantities
                        .into_iter()
                        .map(|qty| ((qty as f64 / base_coin.scalar as f64) * 1e9).round() / 1e9)
                        .collect(),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Get the vault balances for a pool
    ///
    /// @param pool_key - Key of the pool
    pub async fn vault_balances(&self, pool_key: &str) -> anyhow::Result<VaultBalances> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let pool = self.config.get_pool(pool_key)?;
        let base_scalar = self.config.get_coin(&pool.base_coin)?.scalar;
        let quote_scalar = self.config.get_coin(&pool.quote_coin)?.scalar;

        self.deep_book.vault_balances(&mut ptb, pool_key).await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(mut res) => {
                let base_in_vault = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let base_in_vault = bcs::from_bytes::<u64>(&base_in_vault.0)?;

                let quote_in_vault = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let quote_in_vault = bcs::from_bytes::<u64>(&quote_in_vault.0)?;

                let deep_in_vault = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let deep_in_vault = bcs::from_bytes::<u64>(&deep_in_vault.0)?;

                Ok(VaultBalances {
                    base: ((base_in_vault as f64 / base_scalar as f64) * 1e9).round() / 1e9,
                    quote: ((quote_in_vault as f64 / quote_scalar as f64) * 1e9).round() / 1e9,
                    deep: ((deep_in_vault as f64 / DEEP_SCALAR as f64) * 1e9).round() / 1e9,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Get the pool ID by asset types
    ///
    /// @param base_type - Type of the base asset
    /// @param quote_type - Type of the quote asset
    pub async fn get_pool_id_by_assets(
        &self,
        base_type: &str,
        quote_type: &str,
    ) -> anyhow::Result<String> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        self.deep_book
            .get_pool_id_by_assets(&mut ptb, base_type, quote_type)
            .await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(res) => {
                let res = res
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let pool_id = bcs::from_bytes::<SuiAddress>(&res.0)?;
                Ok(pool_id.to_string())
            }
            Err(e) => Err(e),
        }
    }

    /// Get the mid price for a pool
    ///
    /// @param pool_key - Key of the pool
    pub async fn mid_price(&self, pool_key: &str) -> anyhow::Result<f64> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        self.deep_book.mid_price(&mut ptb, pool_key).await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(res) => {
                let res = res
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;

                let mid_price = bcs::from_bytes::<u64>(&res.0)?;
                let adjusted_mid_price = (mid_price as f64 * base_coin.scalar as f64)
                    / (quote_coin.scalar as f64 * FLOAT_SCALAR as f64);

                Ok((adjusted_mid_price * 1e9).round() / 1e9)
            }
            Err(e) => Err(e),
        }
    }

    /// Get the trade parameters for a given pool
    ///
    /// @param pool_key - Key of the pool
    pub async fn pool_trade_params(&self, pool_key: &str) -> anyhow::Result<PoolTradeParams> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        self.deep_book.pool_trade_params(&mut ptb, pool_key).await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(mut res) => {
                let taker_fee = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let taker_fee = bcs::from_bytes::<u64>(&taker_fee.0)?;

                let maker_fee = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let maker_fee = bcs::from_bytes::<u64>(&maker_fee.0)?;

                let stake_required = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let stake_required = bcs::from_bytes::<u64>(&stake_required.0)?;

                Ok(PoolTradeParams {
                    taker_fee: (taker_fee as f64 / FLOAT_SCALAR as f64 * 1e9).round() / 1e9,
                    maker_fee: (maker_fee as f64 / FLOAT_SCALAR as f64 * 1e9).round() / 1e9,
                    stake_required: (stake_required as f64 / DEEP_SCALAR as f64 * 1e9).round()
                        / 1e9,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Get the trade parameters for a given pool, including tick size, lot size, and min size
    ///
    /// @param pool_key - Key of the pool
    pub async fn pool_book_params(&self, pool_key: &str) -> anyhow::Result<PoolBookParams> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let pool = self.config.get_pool(pool_key)?;
        let base_scalar = self.config.get_coin(&pool.base_coin)?.scalar;
        let quote_scalar = self.config.get_coin(&pool.quote_coin)?.scalar;

        self.deep_book.pool_book_params(&mut ptb, pool_key).await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(mut res) => {
                let tick_size = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let tick_size = bcs::from_bytes::<u64>(&tick_size.0)?;

                let lot_size = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let lot_size = bcs::from_bytes::<u64>(&lot_size.0)?;

                let min_size = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let min_size = bcs::from_bytes::<u64>(&min_size.0)?;

                Ok(PoolBookParams {
                    tick_size: (tick_size as f64 * base_scalar as f64)
                        / (quote_scalar as f64 * FLOAT_SCALAR as f64),
                    lot_size: lot_size as f64 / base_scalar as f64,
                    min_size: min_size as f64 / base_scalar as f64,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Get the account information for a given pool and balance manager
    ///
    /// @param pool_key - Key of the pool
    /// @param manager_key - The key of the BalanceManager
    pub async fn account(&self, pool_key: &str, manager_key: &str) -> anyhow::Result<Account> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let pool = self.config.get_pool(pool_key)?;
        let base_scalar = self.config.get_coin(&pool.base_coin)?.scalar;
        let quote_scalar = self.config.get_coin(&pool.quote_coin)?.scalar;

        self.deep_book
            .account(&mut ptb, pool_key, manager_key)
            .await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(res) => {
                let res = res
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let raw_account = bcs::from_bytes::<Account>(&res.0)?;
                Ok(Account {
                    epoch: raw_account.epoch,
                    open_orders: raw_account.open_orders,
                    taker_volume: raw_account.taker_volume as f64 / base_scalar as f64,
                    maker_volume: raw_account.maker_volume as f64 / base_scalar as f64,
                    active_stake: raw_account.active_stake as f64 / DEEP_SCALAR as f64,
                    inactive_stake: raw_account.inactive_stake as f64 / DEEP_SCALAR as f64,
                    created_proposal: raw_account.created_proposal,
                    voted_proposal: raw_account.voted_proposal,
                    unclaimed_rebates: Balances {
                        base: raw_account.unclaimed_rebates.base as f64 / base_scalar as f64,
                        quote: raw_account.unclaimed_rebates.quote as f64 / quote_scalar as f64,
                        deep: raw_account.unclaimed_rebates.deep as f64 / DEEP_SCALAR as f64,
                    },
                    settled_balances: Balances {
                        base: raw_account.settled_balances.base as f64 / base_scalar as f64,
                        quote: raw_account.settled_balances.quote as f64 / quote_scalar as f64,
                        deep: raw_account.settled_balances.deep as f64 / DEEP_SCALAR as f64,
                    },
                    owed_balances: Balances {
                        base: raw_account.owed_balances.base as f64 / base_scalar as f64,
                        quote: raw_account.owed_balances.quote as f64 / quote_scalar as f64,
                        deep: raw_account.owed_balances.deep as f64 / DEEP_SCALAR as f64,
                    },
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Get the locked balances for a pool and balance manager
    ///
    /// @param pool_key - Key of the pool
    /// @param balance_manager_key - The key of the BalanceManager
    pub async fn locked_balance(
        &self,
        pool_key: &str,
        balance_manager_key: &str,
    ) -> anyhow::Result<Balances> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let pool = self.config.get_pool(pool_key)?;
        let base_scalar = self.config.get_coin(&pool.base_coin)?.scalar;
        let quote_scalar = self.config.get_coin(&pool.quote_coin)?.scalar;

        self.deep_book
            .locked_balance(&mut ptb, pool_key, balance_manager_key)
            .await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(mut res) => {
                let base_locked = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let base_locked = bcs::from_bytes::<u64>(&base_locked.0)?;

                let quote_locked = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let quote_locked = bcs::from_bytes::<u64>(&quote_locked.0)?;

                let deep_locked = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let deep_locked = bcs::from_bytes::<u64>(&deep_locked.0)?;

                Ok(Balances {
                    base: ((base_locked as f64 / base_scalar as f64) * 1e9).round() / 1e9,
                    quote: ((quote_locked as f64 / quote_scalar as f64) * 1e9).round() / 1e9,
                    deep: ((deep_locked as f64 / DEEP_SCALAR as f64) * 1e9).round() / 1e9,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Get the DEEP price conversion for a pool
    ///
    /// @param pool_key - Key of the pool
    pub async fn get_pool_deep_price(&self, pool_key: &str) -> anyhow::Result<PoolDeepPrice> {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;
        let deep_coin = self.config.get_coin("DEEP")?;

        self.deep_book
            .get_pool_deep_price(&mut ptb, pool_key)
            .await?;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(res) => {
                let res = res
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get first result"))?;
                let pool_deep_price = bcs::from_bytes::<RawOrderDeepPrice>(&res.0)?;
                let deep_per_asset =
                    (pool_deep_price.deep_per_asset as f64 / FLOAT_SCALAR as f64) * 1e9;

                Ok(PoolDeepPrice {
                    asset_is_base: pool_deep_price.asset_is_base,
                    deep_per_base: if pool_deep_price.asset_is_base {
                        Some(
                            (deep_per_asset * base_coin.scalar as f64 / deep_coin.scalar as f64)
                                .round()
                                / 1e9,
                        )
                    } else {
                        None
                    },
                    deep_per_quote: if !pool_deep_price.asset_is_base {
                        Some(
                            (deep_per_asset * quote_coin.scalar as f64 / deep_coin.scalar as f64)
                                .round()
                                / 1e9,
                        )
                    } else {
                        None
                    },
                })
            }
            Err(e) => Err(e),
        }
    }

    async fn get_quote_quantity_out_inner(
        &self,
        ptb: ProgrammableTransactionBuilder,
        pool_key: &str,
        base_quantity: f64,
    ) -> anyhow::Result<QuoteQuantityOut> {
        let pool = self.config.get_pool(pool_key)?;
        let base_scalar = self.config.get_coin(&pool.base_coin)?.scalar;
        let quote_scalar = self.config.get_coin(&pool.quote_coin)?.scalar;

        match self.client.dev_inspect_transaction(self.address, ptb).await {
            Ok(mut res) => {
                let base_out = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get last result"))?;
                let base_out = bcs::from_bytes::<u64>(&base_out.0)?;

                let quote_out = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get last result"))?;
                let quote_out = bcs::from_bytes::<u64>(&quote_out.0)?;

                let deep_required = res
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get last result"))?;
                let deep_required = bcs::from_bytes::<u64>(&deep_required.0)?;

                Ok(QuoteQuantityOut {
                    base_quantity,
                    base_out: (base_out as f64 / base_scalar as f64 * 1e9).round() / 1e9,
                    quote_out: (quote_out as f64 / quote_scalar as f64 * 1e9).round() / 1e9,
                    deep_required: (deep_required as f64 / DEEP_SCALAR as f64 * 1e9).round() / 1e9,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Decode the order ID to get bid/ask status, price, and order ID
    ///
    /// @param encoded_order_id - Encoded order ID
    pub fn decode_order_id(&self, encoded_order_id: u128) -> anyhow::Result<(bool, u64, u64)> {
        let is_bid = (encoded_order_id >> 127) == 0;
        let price = ((encoded_order_id >> 64) & ((1 << 63) - 1)) as u64;
        let order_id = (encoded_order_id & ((1 << 64) - 1)) as u64;

        Ok((is_bid, price, order_id))
    }
}
