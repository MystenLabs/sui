// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use sui::client_commands::WalletContext;
use sui_json_rpc_types::{SuiExecutionStatus, SuiParsedObject};
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    gas_coin::GasCoin,
    messages::Transaction,
};
use tokio::sync::Mutex;
use tracing::{error, info};
use uuid::Uuid;

use crate::{Faucet, FaucetError, FaucetReceipt};

/// Pair a primary coin (to split off SUI coins) with a gas coin
/// for higher concurrenc.. They always work together.
/// Usage represents the times this pair has been used since
/// faucet's start. We just need a consistent metric to spread out
/// loads, a global counter such as sequence number is not necessary.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct CoinPair {
    pub primary_coin_id: ObjectID,
    gas_coin_id: ObjectID,
    pub usage: usize,
}

impl Ord for CoinPair {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .usage
            .cmp(&self.usage)
            .then_with(|| self.primary_coin_id.cmp(&other.primary_coin_id))
    }
}

impl PartialOrd for CoinPair {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub struct SimpleFaucet {
    wallet: WalletContext,
    /// A Min Heap to order CoinPairs. Favors the least used pair.
    pub(crate) coins: Mutex<BinaryHeap<CoinPair>>,
    active_address: SuiAddress,
}

const DEFAULT_GAS_BUDGET: u64 = 1000;

impl SimpleFaucet {
    pub async fn new(mut wallet: WalletContext, max_currency: usize) -> Result<Self, FaucetError> {
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
        if max_currency * 2 > coins.len() {
            panic!(
                "Not enough coins to guarantee max_currency ({max_currency}), got {} coins",
                coins.len()
            );
        }
        let primary_coins = coins.split_off(coins.len() - max_currency);
        let gas_coins = coins.split_off(coins.len() - max_currency);
        let mut coins = BinaryHeap::new();
        for (_i, (primary, gas)) in primary_coins.iter().zip(gas_coins.iter()).enumerate() {
            coins.push(CoinPair {
                primary_coin_id: *primary.id(),
                gas_coin_id: *gas.id(),
                usage: 0,
            });
        }

        info!("Using coins: {:?} with max concurrency {max_currency}", coins);

        Ok(Self {
            wallet,
            coins: Mutex::new(coins),
            active_address,
        })
    }

    async fn select_coins(&self) -> (ObjectID, ObjectID) {
        // TODO: for now we assume each SUI object is enough to cover the split
        // but this may not be true, if we run the faucet for really really long time or
        // due to some other unexpected issues.
        let mut coins = self.coins.lock().await;
        let mut candidate = coins.pop().expect("Coins heap shouldn't be empty");
        let primary = candidate.primary_coin_id;
        let gas = candidate.gas_coin_id;
        candidate.usage += 1;
        coins.push(candidate);
        info!("after selection, Coins: {:?}", coins);
        (primary, gas)
    }

    async fn get_coins(
        &self,
        amounts: &[u64],
    ) -> Result<(Vec<SuiParsedObject>, TransactionDigest), FaucetError> {
        let (primary, gas) = self.select_coins().await;

        let (coins, tx_digest) = self
            .split_coins(
                amounts,
                primary,
                gas,
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
        let futures: Vec<_> = coins
            .iter()
            .map(|coin_id| {
                self.transfer_sui(*coin_id, self.active_address, recipient, DEFAULT_GAS_BUDGET)
            })
            .collect();
        let results = futures::future::join_all(futures).await;
        let digests: Vec<_> = results
            .into_iter()
            .filter(|res| {
                if res.is_ok() {
                    true
                } else {
                    error!("Encountered error in transfer sui: {:?}", res);
                    false
                }
            })
            .map(|res| res.unwrap())
            .collect();
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

    async fn transfer_sui(
        &self,
        coin_id: ObjectID,
        signer: SuiAddress,
        recipient: SuiAddress,
        budget: u64,
    ) -> Result<TransactionDigest, anyhow::Error> {
        let context = &self.wallet;

        let data = context
            .gateway
            .transfer_sui(signer, coin_id, budget, recipient, None)
            .await?;
        let signature = context.keystore.sign(&signer, &data.to_bytes())?;

        let tx = Transaction::new(data, signature);
        info!(tx_digest = ?tx.digest(), recipient = ?recipient, coin_id = ?coin_id, "Broadcasting transfer obj txn");
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
