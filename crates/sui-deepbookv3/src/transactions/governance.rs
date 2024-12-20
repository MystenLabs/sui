// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use sui_sdk::types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_sdk::SuiClient;

use crate::utils::config::{DeepBookConfig, DEEP_SCALAR, FLOAT_SCALAR};
use crate::utils::types::ProposalParams;

use sui_sdk::types::base_types::ObjectID;
use sui_sdk::types::{Identifier, TypeTag};

use super::balance_manager::BalanceManagerContract;
use crate::DataReader;

/// GovernanceContract struct for managing governance operations in DeepBook.
pub struct GovernanceContract {
    client: SuiClient,
    config: DeepBookConfig,
    balance_manager_contract: BalanceManagerContract,
}

impl GovernanceContract {
    /// Creates a new GovernanceContract instance
    ///
    /// @param config - Configuration for GovernanceContract
    /// @param client - SuiClient instance
    /// @param balance_manager_contract - BalanceManagerContract instance
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

    /// Stake a specified amount in the pool
    ///
    /// @param pool_key - The key to identify the pool
    /// @param balance_manager_key - The key to identify the BalanceManager
    /// @param stake_amount - The amount to stake
    pub async fn stake(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        balance_manager_key: &str,
        stake_amount: f64,
    ) -> anyhow::Result<()> {
        let pool = self.config.get_pool(pool_key)?;
        let balance_manager = self.config.get_balance_manager(balance_manager_key)?;
        let trade_proof = self
            .balance_manager_contract
            .generate_proof(ptb, balance_manager_key)
            .await?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;
        let stake_input = (stake_amount * DEEP_SCALAR as f64).round() as u64;

        let base_coin_tag = TypeTag::from_str(base_coin.type_name.as_str())?;
        let quote_coin_tag = TypeTag::from_str(quote_coin.type_name.as_str())?;

        let pool_id = ObjectID::from_hex_literal(pool.address.as_str())?;
        let balance_manager_id = ObjectID::from_hex_literal(balance_manager.address.as_str())?;
        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(balance_manager_id).await?)?,
            trade_proof,
            ptb.pure(stake_input)?,
        ];

        ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("stake")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        );
        Ok(())
    }

    /// UnStake from the pool
    ///
    /// @param pool_key - The key to identify the pool
    /// @param balance_manager_key - The key to identify the BalanceManager
    pub async fn unstake(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        balance_manager_key: &str,
    ) -> anyhow::Result<()> {
        let pool = self.config.get_pool(pool_key)?;
        let balance_manager = self.config.get_balance_manager(balance_manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let base_coin_tag = TypeTag::from_str(base_coin.type_name.as_str())?;
        let quote_coin_tag = TypeTag::from_str(quote_coin.type_name.as_str())?;

        let trade_proof = self
            .balance_manager_contract
            .generate_proof(ptb, balance_manager_key)
            .await?;

        let pool_id = ObjectID::from_hex_literal(pool.address.as_str())?;
        let balance_manager_id = ObjectID::from_hex_literal(balance_manager.address.as_str())?;
        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(balance_manager_id).await?)?,
            trade_proof,
        ];

        ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("unstake")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        );
        Ok(())
    }

    /// Submit a governance proposal
    ///
    /// @param params - Parameters for the proposal
    pub async fn submit_proposal(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        params: ProposalParams,
    ) -> anyhow::Result<()> {
        let pool = self.config.get_pool(&params.pool_key)?;
        let balance_manager = self
            .config
            .get_balance_manager(&params.balance_manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let taker_fee = (params.taker_fee * FLOAT_SCALAR as f64).round() as u64;
        let maker_fee = (params.maker_fee * FLOAT_SCALAR as f64).round() as u64;
        let stake_required = (params.stake_required * DEEP_SCALAR as f64).round() as u64;

        let pool_id = ObjectID::from_hex_literal(pool.address.as_str())?;
        let balance_manager_id = ObjectID::from_hex_literal(balance_manager.address.as_str())?;

        let base_coin_tag = TypeTag::from_str(base_coin.type_name.as_str())?;
        let quote_coin_tag = TypeTag::from_str(quote_coin.type_name.as_str())?;

        let trade_proof = self
            .balance_manager_contract
            .generate_proof(ptb, &params.balance_manager_key)
            .await?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(balance_manager_id).await?)?,
            trade_proof,
            ptb.pure(taker_fee)?,
            ptb.pure(maker_fee)?,
            ptb.pure(stake_required)?,
        ];

        ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("submit_proposal")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        );

        Ok(())
    }

    /// Vote on a proposal
    ///
    /// @param pool_key - The key to identify the pool
    /// @param balance_manager_key - The key to identify the BalanceManager
    /// @param proposal_id - The ID of the proposal to vote on
    pub async fn vote(
        &self,
        ptb: &mut ProgrammableTransactionBuilder,
        pool_key: &str,
        balance_manager_key: &str,
        proposal_id: &str,
    ) -> anyhow::Result<()> {
        let pool = self.config.get_pool(pool_key)?;
        let balance_manager = self.config.get_balance_manager(balance_manager_key)?;
        let base_coin = self.config.get_coin(&pool.base_coin)?;
        let quote_coin = self.config.get_coin(&pool.quote_coin)?;

        let base_coin_tag = TypeTag::from_str(base_coin.type_name.as_str())?;
        let quote_coin_tag = TypeTag::from_str(quote_coin.type_name.as_str())?;

        let trade_proof = self
            .balance_manager_contract
            .generate_proof(ptb, balance_manager_key)
            .await?;

        let pool_id = ObjectID::from_hex_literal(pool.address.as_str())?;
        let balance_manager_id = ObjectID::from_hex_literal(balance_manager.address.as_str())?;
        let proposal_id = ObjectID::from_hex_literal(proposal_id)?;

        let arguments = vec![
            ptb.obj(self.client.share_object(pool_id).await?)?,
            ptb.obj(self.client.share_object(balance_manager_id).await?)?,
            trade_proof,
            ptb.pure(proposal_id)?,
        ];

        ptb.programmable_move_call(
            ObjectID::from_hex_literal(self.config.deepbook_package_id())?,
            Identifier::new("pool")?,
            Identifier::new("vote")?,
            vec![base_coin_tag, quote_coin_tag],
            arguments,
        );
        Ok(())
    }
}
