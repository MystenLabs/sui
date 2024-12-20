// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use crate::utils::config::DeepBookConfig;
use sui_sdk::{
    types::{
        base_types::{ObjectID, ObjectRef},
        programmable_transaction_builder::ProgrammableTransactionBuilder,
        transaction::{Argument, Command, ObjectArg},
        Identifier, TypeTag,
    },
    SuiClient,
};

use crate::DataReader;

/// FlashLoanContract struct for managing flash loans.
pub struct FlashLoanContract {
    client: SuiClient,
    config: DeepBookConfig,
}

impl FlashLoanContract {
    /// Creates a new FlashLoanContract instance
    ///
    /// @param client - SuiClient instance
    /// @param config - Configuration object for DeepBook
    /// @param balance_manager_contract - BalanceManagerContract instance
    pub fn new(client: SuiClient, config: DeepBookConfig) -> Self {
        Self { client, config }
    }

    /// Borrow base asset from the pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param borrow_amount - The amount to borrow
    /// @returns A tuple containing the base coin result and flash loan object
    pub async fn borrow_base_asset<'a>(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        borrow_amount: f64,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;
        let input_quantity = (borrow_amount * base_coin.scalar as f64).round() as u64;

        let pool_id = ObjectID::from_hex_literal(pool.address.as_str())?;

        let base_coin_tag = TypeTag::from_str(base_coin.type_name.as_str())?;
        let quote_coin_tag = TypeTag::from_str(quote_coin.type_name.as_str())?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.pure(input_quantity)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("borrow_flashloan_base")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Return base asset to the pool after a flash loan
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param borrow_amount - The amount of the base asset to return
    /// @param base_coin_input - Coin object representing the base asset to be returned
    /// @param flash_loan - FlashLoan object representing the loan to be settled
    pub async fn return_base_asset<'a>(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        borrow_amount: f64,
        base_coin_input: Argument,
        flash_loan: ObjectRef,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;
        let borrow_scalar = base_coin.scalar;

        let return_amount = ptb.pure((borrow_amount * borrow_scalar as f64).round() as u64)?;
        let base_coin_return =
            ptb.command(Command::SplitCoins(base_coin_input, vec![return_amount]));

        let pool_id = ObjectID::from_hex_literal(pool.address.as_str())?;
        let base_coin_tag = TypeTag::from_str(base_coin.type_name.as_str())?;
        let quote_coin_tag = TypeTag::from_str(quote_coin.type_name.as_str())?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.pure(base_coin_return)?,
            ptb.obj(ObjectArg::ImmOrOwnedObject(flash_loan))?,
        ];

        ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("return_flashloan_base")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        );

        Ok(base_coin_return)
    }

    /// Borrow quote asset from the pool
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param borrow_amount - The amount to borrow
    /// @returns A tuple containing the quote coin result and flash loan object
    pub async fn borrow_quote_asset<'a>(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        borrow_amount: f64,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;
        let input_quantity = (borrow_amount * quote_coin.scalar as f64).round() as u64;

        let pool_id = ObjectID::from_hex_literal(pool.address.as_str())?;
        let base_coin_tag = TypeTag::from_str(base_coin.type_name.as_str())?;
        let quote_coin_tag = TypeTag::from_str(quote_coin.type_name.as_str())?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.pure(input_quantity)?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("borrow_flashloan_quote")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }

    /// Return quote asset to the pool after a flash loan
    ///
    /// @param ptb - ProgrammableTransactionBuilder instance
    /// @param pool_key - The key to identify the pool
    /// @param borrow_amount - The amount of the quote asset to return
    /// @param quote_coin_input - Coin object representing the quote asset to be returned
    /// @param flash_loan - FlashLoan object representing the loan to be settled
    pub async fn return_quote_asset<'a>(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        borrow_amount: f64,
        quote_coin_input: Argument,
        flash_loan: ObjectRef,
    ) -> anyhow::Result<Argument> {
        let pool = self.config.get_pool(pool_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;
        let borrow_scalar = quote_coin.scalar;

        let return_amount = ptb.pure((borrow_amount * borrow_scalar as f64).round() as u64)?;
        let quote_coin_return =
            ptb.command(Command::SplitCoins(quote_coin_input, vec![return_amount]));

        let pool_id = ObjectID::from_hex_literal(pool.address.as_str())?;
        let base_coin_tag = TypeTag::from_str(base_coin.type_name.as_str())?;
        let quote_coin_tag = TypeTag::from_str(quote_coin.type_name.as_str())?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.pure(quote_coin_return)?,
            ptb.obj(ObjectArg::ImmOrOwnedObject(flash_loan))?,
        ];

        Ok(ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("return_flashloan_quote")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        ))
    }
}
