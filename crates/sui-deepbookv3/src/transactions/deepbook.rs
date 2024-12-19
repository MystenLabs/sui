// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use sui_sdk::{
    types::{
        base_types::ObjectID, programmable_transaction_builder::ProgrammableTransactionBuilder,
        transaction::Argument, Identifier, TypeTag, SUI_CLOCK_OBJECT_ID,
    },
    SuiClient,
};

use crate::utils::{
    config::{DeepBookConfig, DEEP_SCALAR, FLOAT_SCALAR, MAX_TIMESTAMP},
    types::{
        OrderType, PlaceLimitOrderParams, PlaceMarketOrderParams, SelfMatchingOptions, SwapParams,
    },
};

use super::balance_manager::BalanceManagerContract;

use crate::DataReader;

/// DeepBookContract struct for managing DeepBook operations
pub struct DeepBookContract {
    client: SuiClient,
    config: DeepBookConfig,
    balance_manager_contract: BalanceManagerContract,
}

impl DeepBookContract {
    /// Creates a new DeepBookContract instance
    ///
    /// @param client - The SuiClient instance
    /// @param config - The DeepBookConfig instance
    /// @param balance_manager_contract - The BalanceManagerContract instance
    pub fn new(
        client: SuiClient,
        config: DeepBookConfig,
        balance_manager_contract: BalanceManagerContract,
    ) -> Self {
        Self {
            client,
            config,
            balance_manager_contract,
        }
    }

    /// Place a limit order
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param params - The PlaceLimitOrderParams instance
    /// @returns The place limit order call
    pub async fn place_limit_order(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        params: PlaceLimitOrderParams,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(&params.pool_key)?;
        let balance_manager = self
            .config
            .get_balance_manager(&params.balance_manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let input_price = ((params.price * FLOAT_SCALAR as f64 * quote_coin.scalar as f64)
            / base_coin.scalar as f64)
            .round() as u64;
        let input_quantity = (params.quantity * base_coin.scalar as f64).round() as u64;

        let trade_proof = self
            .balance_manager_contract
            .generate_proof(ptb, &params.balance_manager_key)
            .await?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let balance_manager_id = ObjectID::from_hex_literal(&balance_manager.address)?;
        let expiration = params.expiration.unwrap_or(MAX_TIMESTAMP);
        let order_type = params.order_type.unwrap_or(OrderType::NoRestriction);
        let self_matching_option = params
            .self_matching_option
            .unwrap_or(SelfMatchingOptions::SelfMatchingAllowed);
        let pay_with_deep = params.pay_with_deep.unwrap_or(true);

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(balance_manager_id).await?)?,
            trade_proof,
            ptb.pure(params.client_order_id)?,
            ptb.pure(order_type as u8)?,
            ptb.pure(self_matching_option as u8)?,
            ptb.pure(input_price)?,
            ptb.pure(input_quantity)?,
            ptb.pure(params.is_bid)?,
            ptb.pure(pay_with_deep)?,
            ptb.pure(expiration)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("place_limit_order")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Place a market order
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param params - The PlaceMarketOrderParams instance
    /// @returns The place market order call
    pub async fn place_market_order(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        params: PlaceMarketOrderParams,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(&params.pool_key)?;
        let balance_manager = self
            .config
            .get_balance_manager(&params.balance_manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let input_quantity = (params.quantity * base_coin.scalar as f64).round() as u64;
        let trade_proof = self
            .balance_manager_contract
            .generate_proof(ptb, &params.balance_manager_key)
            .await?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let balance_manager_id = ObjectID::from_hex_literal(&balance_manager.address)?;
        let self_matching_option = params
            .self_matching_option
            .unwrap_or(SelfMatchingOptions::SelfMatchingAllowed);
        let pay_with_deep = params.pay_with_deep.unwrap_or(true);

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(balance_manager_id).await?)?,
            trade_proof,
            ptb.pure(params.client_order_id)?,
            ptb.pure(self_matching_option as u8)?,
            ptb.pure(input_quantity)?,
            ptb.pure(params.is_bid)?,
            ptb.pure(pay_with_deep)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("place_market_order")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Modify an existing order
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param balance_manager_key - The key to identify the BalanceManager
    /// @param order_id - The ID of the order to modify
    /// @param new_quantity - The new quantity to set for the order
    /// @returns The modify order call
    pub async fn modify_order(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        balance_manager_key: &str,
        order_id: &str,
        new_quantity: f64,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let balance_manager = self.config.get_balance_manager(balance_manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let input_quantity = (new_quantity * base_coin.scalar as f64).round() as u64;
        let trade_proof = self
            .balance_manager_contract
            .generate_proof(ptb, balance_manager_key)
            .await?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let balance_manager_id = ObjectID::from_hex_literal(&balance_manager.address)?;
        let order_id = ObjectID::from_hex_literal(order_id)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(balance_manager_id).await?)?,
            trade_proof,
            ptb.pure(order_id)?,
            ptb.pure(input_quantity)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("modify_order")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Cancel an existing order
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param balance_manager_key - The key to identify the BalanceManager
    /// @param order_id - The ID of the order to cancel
    /// @returns The cancel order call
    pub async fn cancel_order(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        balance_manager_key: &str,
        order_id: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let balance_manager = self.config.get_balance_manager(balance_manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let trade_proof = self
            .balance_manager_contract
            .generate_proof(ptb, balance_manager_key)
            .await?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let balance_manager_id = ObjectID::from_hex_literal(&balance_manager.address)?;
        let order_id = ObjectID::from_hex_literal(order_id)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(balance_manager_id).await?)?,
            trade_proof,
            ptb.pure(order_id)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("cancel_order")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Cancel all open orders for a balance manager
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param balance_manager_key - The key to identify the BalanceManager
    /// @returns The cancel all orders call
    pub async fn cancel_all_orders(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        balance_manager_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let balance_manager = self.config.get_balance_manager(balance_manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let trade_proof = self
            .balance_manager_contract
            .generate_proof(ptb, balance_manager_key)
            .await?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let balance_manager_id = ObjectID::from_hex_literal(&balance_manager.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(balance_manager_id).await?)?,
            trade_proof,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("cancel_all_orders")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Withdraw settled amounts for a balance manager
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param balance_manager_key - The key to identify the BalanceManager
    /// @returns The withdraw settled amounts call
    pub async fn withdraw_settled_amounts(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        balance_manager_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let balance_manager = self.config.get_balance_manager(balance_manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let trade_proof = self
            .balance_manager_contract
            .generate_proof(ptb, balance_manager_key)
            .await?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let balance_manager_id = ObjectID::from_hex_literal(&balance_manager.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(balance_manager_id).await?)?,
            trade_proof,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("withdraw_settled_amounts")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Add a deep price point for a target pool using a reference pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param target_pool_key - The key to identify the target pool
    /// @param reference_pool_key - The key to identify the reference pool
    /// @returns The add deep price point call
    pub async fn add_deep_price_point(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        target_pool_key: &str,
        reference_pool_key: &str,
    ) -> anyhow::Result<Argument> {
        let target_pool = self.config.get_pool(target_pool_key)?;
        let reference_pool = self.config.get_pool(reference_pool_key)?;

        let target_base_coin = self.config.get_coin(&target_pool.base_coin)?;
        let target_quote_coin = self.config.get_coin(&target_pool.quote_coin)?;
        let reference_base_coin = self.config.get_coin(&reference_pool.base_coin)?;
        let reference_quote_coin = self.config.get_coin(&reference_pool.quote_coin)?;

        let target_pool_id = ObjectID::from_hex_literal(&target_pool.address)?;
        let reference_pool_id = ObjectID::from_hex_literal(&reference_pool.address)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(target_pool_id).await?)?,
            ptb.obj(self.client.share_object(reference_pool_id).await?)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("add_deep_price_point")?,
            vec![
                TypeTag::from_str(&target_base_coin.type_name)?,
                TypeTag::from_str(&target_quote_coin.type_name)?,
                TypeTag::from_str(&reference_base_coin.type_name)?,
                TypeTag::from_str(&reference_quote_coin.type_name)?,
            ],
            arguments,
        ))
    }

    /// Claim rebates for a balance manager
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param balance_manager_key - The key to identify the BalanceManager
    /// @returns The claim rebates call
    pub async fn claim_rebates(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        balance_manager_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let balance_manager = self.config.get_balance_manager(balance_manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let trade_proof = self
            .balance_manager_contract
            .generate_proof(ptb, balance_manager_key)
            .await?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let balance_manager_id = ObjectID::from_hex_literal(&balance_manager.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(balance_manager_id).await?)?,
            trade_proof,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("claim_rebates")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Gets an order
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param order_id - The ID of the order to get
    /// @returns The order
    pub async fn get_order(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        order_id: u128,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.pure(order_id)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("get_order")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Gets multiple orders from a specified pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param order_ids - Array of order IDs to retrieve
    /// @returns The orders
    pub async fn get_orders(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        order_ids: Vec<String>,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let order_ids: Vec<ObjectID> = order_ids
            .iter()
            .map(|id| ObjectID::from_hex_literal(id))
            .collect::<Result<Vec<_>, _>>()?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.pure(order_ids)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("get_orders")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Burns DEEP tokens from the pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @returns The burn deep call
    pub async fn burn_deep(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let treasury_id = ObjectID::from_hex_literal(self.config.deep_treasury_id())?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(treasury_id).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("burn_deep")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Gets the mid price for a pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @returns The mid price
    pub async fn mid_price(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("mid_price")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Checks if a pool is whitelisted
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @returns The whitelisted status
    pub async fn whitelisted(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![ptb.obj(self.client.share_object(pool_id).await?)?];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("whitelisted")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Gets the quote quantity out for a given base quantity in
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param base_quantity - Base quantity to convert
    /// @returns The quote quantity out
    pub async fn get_quote_quantity_out(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        base_quantity: f64,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let input_quantity = (base_quantity * base_coin.scalar as f64).round() as u64;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.pure(input_quantity)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("get_quote_quantity_out")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Gets the base quantity out for a given quote quantity in
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param quote_quantity - Quote quantity to convert
    /// @returns The base quantity out
    pub async fn get_base_quantity_out(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        quote_quantity: f64,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let input_quantity = (quote_quantity * quote_coin.scalar as f64).round() as u64;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.pure(input_quantity)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("get_base_quantity_out")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Gets the quantity out for a given base or quote quantity
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param base_quantity - Base quantity to convert
    /// @param quote_quantity - Quote quantity to convert
    /// @returns The quantity out
    pub async fn get_quantity_out(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        base_quantity: f64,
        quote_quantity: f64,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let base_input = (base_quantity * base_coin.scalar as f64).round() as u64;
        let quote_input = (quote_quantity * quote_coin.scalar as f64).round() as u64;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.pure(base_input)?,
            ptb.pure(quote_input)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("get_quantity_out")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Gets open orders for a balance manager in a pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param manager_key - Key of the balance manager
    /// @returns The open orders
    pub async fn account_open_orders(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        manager_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let manager = self.config.get_balance_manager(manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let manager_id = ObjectID::from_hex_literal(&manager.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(manager_id).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("account_open_orders")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Gets level 2 order book specifying range of price
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param price_low - Lower bound of the price range
    /// @param price_high - Upper bound of the price range
    /// @param is_bid - Whether to get bid or ask orders
    /// @returns The level 2 order book ticks from mid-price
    pub async fn get_level2_range(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        price_low: f64,
        price_high: f64,
        is_bid: bool,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let low_price = ((price_low * FLOAT_SCALAR as f64 * quote_coin.scalar as f64)
            / base_coin.scalar as f64)
            .round() as u64;
        let high_price = ((price_high * FLOAT_SCALAR as f64 * quote_coin.scalar as f64)
            / base_coin.scalar as f64)
            .round() as u64;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.pure(low_price)?,
            ptb.pure(high_price)?,
            ptb.pure(is_bid)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("get_level2_range")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Gets level 2 order book ticks from mid-price for a pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param tick_from_mid - Number of ticks from mid-price
    /// @returns The level 2 order book ticks from mid-price
    pub async fn get_level2_ticks_from_mid(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        tick_from_mid: u64,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.pure(tick_from_mid)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("get_level2_ticks_from_mid")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Gets the vault balances for a pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @returns The vault balances
    pub async fn vault_balances(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![ptb.obj(self.client.share_object(pool_id).await?)?];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("vault_balances")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Gets the pool ID by asset types
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param base_type - Type of the base asset
    /// @param quote_type - Type of the quote asset
    /// @returns The pool ID
    pub async fn get_pool_id_by_assets(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        base_type: &str,
        quote_type: &str,
    ) -> anyhow::Result<Argument> {
        let registry_id = ObjectID::from_hex_literal(self.config.registry_id())?;

        let base_coin_tag = TypeTag::from_str(base_type)?;
        let quote_coin_tag = TypeTag::from_str(quote_type)?;

        let arguments = vec![ptb.obj(self.client.share_object(registry_id).await?)?];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("get_pool_id_by_asset")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Swap exact base amount for quote amount
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param params - Parameters for the swap
    pub async fn swap_exact_base_for_quote(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        params: SwapParams,
    ) -> anyhow::Result<()> {
        if params.quote_coin.is_some() {
            return Err(anyhow::anyhow!(
                "quote_coin is not accepted for swapping base asset"
            ));
        }

        let pool = self.config.get_pool(&params.pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;
        let deep_coin = self.config.get_coin("DEEP")?;

        let base_amount = (params.amount * base_coin.scalar as f64).round() as u64;
        let deep_amount = (params.deep_amount * DEEP_SCALAR as f64).round() as u64;
        let min_quote = (params.min_out * quote_coin.scalar as f64).round() as u64;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let base_coin = match params.base_coin {
            Some(coin) => coin,
            None => {
                self.client
                    .get_coin_object(params.sender, base_coin.type_name.clone(), base_amount)
                    .await?
            }
        };

        let deep_coin = match params.deep_coin {
            Some(coin) => coin,
            None => {
                self.client
                    .get_coin_object(params.sender, deep_coin.type_name.clone(), deep_amount)
                    .await?
            }
        };

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.coin_object(base_coin).await?)?,
            ptb.obj(self.client.coin_object(deep_coin).await?)?,
            ptb.pure(min_quote)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("swap_exact_base_for_quote")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        );

        Ok(())
    }

    /// Swap exact quote amount for base amount
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param params - Parameters for the swap
    pub async fn swap_exact_quote_for_base(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        params: SwapParams,
    ) -> anyhow::Result<()> {
        if params.base_coin.is_some() {
            return Err(anyhow::anyhow!(
                "base_coin is not accepted for swapping quote asset"
            ));
        }

        let pool = self.config.get_pool(&params.pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;
        let deep_coin = self.config.get_coin("DEEP")?;

        let quote_amount = (params.amount * quote_coin.scalar as f64).round() as u64;
        let deep_amount = (params.deep_amount * DEEP_SCALAR as f64).round() as u64;
        let min_base = (params.min_out * base_coin.scalar as f64).round() as u64;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let quote_coin = match params.base_coin {
            Some(coin) => coin,
            None => {
                self.client
                    .get_coin_object(params.sender, quote_coin.type_name.clone(), quote_amount)
                    .await?
            }
        };

        let deep_coin = match params.deep_coin {
            Some(coin) => coin,
            None => {
                self.client
                    .get_coin_object(params.sender, deep_coin.type_name.clone(), deep_amount)
                    .await?
            }
        };

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.coin_object(quote_coin).await?)?,
            ptb.obj(self.client.coin_object(deep_coin).await?)?,
            ptb.pure(min_base)?,
            ptb.obj(self.client.share_object(SUI_CLOCK_OBJECT_ID).await?)?,
        ];

        ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("swap_exact_quote_for_base")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        );

        Ok(())
    }

    /// Get the trade parameters for a given pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - Key of the pool
    /// @returns The trade parameters
    pub async fn pool_trade_params(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![ptb.obj(self.client.share_object(pool_id).await?)?];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("pool_trade_params")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Get the book parameters for a given pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - Key of the pool
    /// @returns The book parameters
    pub async fn pool_book_params(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![ptb.obj(self.client.share_object(pool_id).await?)?];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("pool_book_params")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Get the account information for a given pool and balance manager
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - Key of the pool
    /// @param manager_key - The key of the BalanceManager
    /// @returns The account information
    pub async fn account(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        manager_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let manager = self.config.get_balance_manager(manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let manager_id = ObjectID::from_hex_literal(&manager.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(manager_id).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("account")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Get the locked balance for a given pool and balance manager
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - Key of the pool
    /// @param manager_key - The key of the BalanceManager
    /// @returns The locked balance
    pub async fn locked_balance(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        manager_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let manager = self.config.get_balance_manager(manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;
        let manager_id = ObjectID::from_hex_literal(&manager.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(manager_id).await?)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("locked_balance")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Get the DEEP price conversion for a pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @returns The DEEP price conversion
    pub async fn get_pool_deep_price(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let pool_id = ObjectID::from_hex_literal(&pool.address)?;

        let base_coin_tag = TypeTag::from_str(&base_coin.type_name)?;
        let quote_coin_tag = TypeTag::from_str(&quote_coin.type_name)?;

        let arguments = vec![ptb.obj(self.client.share_object(pool_id).await?)?];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("get_order_deep_price")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }
}
