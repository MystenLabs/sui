// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use sui::client_commands::WalletContext;
use sui_json_rpc_types::{SuiExecutionStatus, SuiParsedObject};
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    gas_coin::GasCoin,
    messages::Transaction,
};
use tracing::info;
use uuid::Uuid;

use crate::{Faucet, FaucetError, FaucetReceipt};

/// A naive implementation of a faucet that processes
/// request sequentially
pub struct SimpleFaucet {
    wallet: WalletContext,
    // TODO: use a queue of coins to improve concurrency
    /// Used to provide fund to users
    primary_coin_id: ObjectID,
    /// Pay for the gas incurred in operations such as
    /// transfer and split(as opposed to sending to users)
    gas_coin_id: ObjectID,
    active_address: SuiAddress,
}

const DEFAULT_GAS_BUDGET: u64 = 1000;

impl SimpleFaucet {
    pub async fn new(mut wallet: WalletContext) -> Result<Self, FaucetError> {
        let active_address = wallet
            .active_address()
            .map_err(|err| FaucetError::Wallet(err.to_string()))?;
        info!("SimpleFaucet::new with active address: {active_address}");

        let mut coins = wallet
            .gas_objects(active_address)
            .await
            .map_err(|e| FaucetError::Wallet(e.to_string()))?
            .iter()
            // Ok to unwrap() since `get_gas_objects` guarantees gas
            .map(|q| GasCoin::try_from(&q.1).unwrap())
            .collect::<Vec<GasCoin>>();
        info!("Coins held: {:?}", coins);
        coins.sort_by_key(|a| a.value());

        if coins.len() < 2 {
            return Err(FaucetError::InsuffientCoins(2, coins.len()));
        }

        let primary_coin = &coins[coins.len() - 1];
        let gas_coin = &coins[coins.len() - 2];

        info!(
            "Using {} as primary, {} as the gas payment",
            primary_coin, gas_coin
        );

        Ok(Self {
            wallet,
            primary_coin_id: *primary_coin.id(),
            gas_coin_id: *gas_coin.id(),
            active_address,
        })
    }

    async fn get_coins(
        &self,
        amounts: &[u64],
    ) -> Result<(Vec<SuiParsedObject>, TransactionDigest), FaucetError> {
        let (coins, tx_digest) = self
            .split_coins(
                amounts,
                self.primary_coin_id,
                self.gas_coin_id,
                self.active_address,
                DEFAULT_GAS_BUDGET,
            )
            .await
            .map_err(|err| FaucetError::Wallet(err.to_string()))?;

        Ok((coins, tx_digest))
    }

    async fn public_transfer_objects(
        &self,
        coins: &[ObjectID],
        recipient: SuiAddress,
    ) -> Result<Vec<TransactionDigest>, FaucetError> {
        let mut digests = Vec::with_capacity(coins.len());
        for coin_id in coins.iter() {
            let digest = self
                .public_transfer_object(
                    *coin_id,
                    self.gas_coin_id,
                    self.active_address,
                    recipient,
                    DEFAULT_GAS_BUDGET,
                )
                .await
                .map_err(|err| FaucetError::Transfer(err.to_string()))?;
            digests.push(digest);
        }
        Ok(digests)
    }

    async fn split_coins(
        &self,
        amounts: &[u64],
        coin_id: ObjectID,
        gas_object_id: ObjectID,
        signer: SuiAddress,
        budget: u64,
    ) -> Result<(Vec<SuiParsedObject>, TransactionDigest), anyhow::Error> {
        // TODO: move this function to impl WalletContext{} and reuse in wallet_commands
        info!(primary_coin_id = ?coin_id, ?amounts, ?gas_object_id, "Splitting coins");
        let context = &self.wallet;
        let data = context
            .gateway
            .split_coin(
                signer,
                coin_id,
                amounts.to_vec().clone(),
                Some(gas_object_id),
                budget,
            )
            .await?;
        let signature = context.keystore.sign(&signer, &data.to_bytes())?;
        let tx = Transaction::new(data, signature);

        info!(tx_digest = ?tx.digest(), coin_id = ?coin_id, gas_object_id = ?gas_object_id, "Broadcasting split coin txn");
        let response = context
            .gateway
            .execute_transaction(tx)
            .await?
            .to_split_coin_response()?;
        let new_coins = response.new_coins;
        let tx_digest = response.certificate.transaction_digest;
        Ok((new_coins, tx_digest))
    }

    async fn public_transfer_object(
        &self,
        coin_id: ObjectID,
        gas_object_id: ObjectID,
        signer: SuiAddress,
        recipient: SuiAddress,
        budget: u64,
    ) -> Result<TransactionDigest, anyhow::Error> {
        let context = &self.wallet;

        let data = context
            .gateway
            .public_transfer_object(signer, coin_id, Some(gas_object_id), budget, recipient)
            .await?;
        let signature = context.keystore.sign(&signer, &data.to_bytes())?;

        let tx = Transaction::new(data, signature);
        info!(tx_digest = ?tx.digest(), recipient = ?recipient, coin_id = ?coin_id, gas_object_id = ?gas_object_id, "Broadcasting transfer obj txn");
        let response = context
            .gateway
            .execute_transaction(tx)
            .await?
            .to_effect_response()?;
        let effects = response.effects;
        if matches!(effects.status, SuiExecutionStatus::Failure { .. }) {
            return Err(anyhow!("Error transferring object: {:#?}", effects.status));
        }

        Ok(response.certificate.transaction_digest)
    }
}

#[async_trait]
impl Faucet for SimpleFaucet {
    async fn send(
        &self,
        id: Uuid,
        recipient: SuiAddress,
        amounts: &[u64],
    ) -> Result<FaucetReceipt, FaucetError> {
        info!(?recipient, uuid = ?id, "Getting faucet requests");
        let (coins, tx_digest) = self.get_coins(amounts).await?;
        let coin_ids = coins.iter().map(|c| c.id()).collect::<Vec<ObjectID>>();
        info!(?recipient, ?tx_digest, ?coin_ids, "SplitCoin txn succeeded");
        let tx_digests = self.public_transfer_objects(&coin_ids, recipient).await?;
        info!(?recipient, ?tx_digests, ?coin_ids, "Transfer txn succeeded");
        Ok(coins.iter().by_ref().collect())
    }
}
