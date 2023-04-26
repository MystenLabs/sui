// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::faucet::write_ahead_log;
use crate::metrics::FaucetMetrics;
use async_trait::async_trait;
use prometheus::Registry;
use tap::tap::TapFallible;

use shared_crypto::intent::Intent;
#[cfg(test)]
use std::collections::HashSet;
use std::path::Path;
use typed_store::Map;

use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_sdk::wallet_context::WalletContext;
use sui_types::object::Owner;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    gas_coin::GasCoin,
    messages::{ExecuteTransactionRequestType, Transaction, TransactionData, VerifiedTransaction},
};
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};
use tokio::time::{timeout, Duration};
use tracing::{error, info, warn};
use uuid::Uuid;

use super::write_ahead_log::WriteAheadLog;
use crate::{CoinInfo, Faucet, FaucetConfig, FaucetError, FaucetReceipt};

pub struct SimpleFaucet {
    wallet: WalletContext,
    active_address: SuiAddress,
    producer: Mutex<Sender<ObjectID>>,
    consumer: Mutex<Receiver<ObjectID>>,
    pub metrics: FaucetMetrics,
    wal: Mutex<WriteAheadLog>,
}

enum GasCoinResponse {
    GasCoinWithInsufficientBalance(ObjectID),
    InvalidGasCoin(ObjectID),
    NoGasCoinAvailable,
    UnknownGasCoin(ObjectID),
    ValidGasCoin(ObjectID),
}

// TODO: replace this with dryrun at the SDK level
const DEFAULT_GAS_COMPUTATION_BUCKET: u64 = 10_000_000;
const LOCK_TIMEOUT: Duration = Duration::from_secs(10);
const RECV_TIMEOUT: Duration = Duration::from_secs(5);

impl SimpleFaucet {
    pub async fn new(
        mut wallet: WalletContext,
        prometheus_registry: &Registry,
        wal_path: &Path,
        config: FaucetConfig,
    ) -> Result<Self, FaucetError> {
        let active_address = wallet
            .active_address()
            .map_err(|err| FaucetError::Wallet(err.to_string()))?;
        info!("SimpleFaucet::new with active address: {active_address}");

        let coins = wallet
            .gas_objects(active_address)
            .await
            .map_err(|e| FaucetError::Wallet(e.to_string()))?
            .iter()
            // Ok to unwrap() since `get_gas_objects` guarantees gas
            .map(|q| GasCoin::try_from(&q.1).unwrap())
            .filter(|coin| coin.0.balance.value() >= (config.amount * config.num_coins as u64))
            .collect::<Vec<GasCoin>>();
        let metrics = FaucetMetrics::new(prometheus_registry);

        let wal = WriteAheadLog::open(wal_path);
        let mut pending = vec![];

        let (producer, consumer) = mpsc::channel(coins.len());
        for coin in &coins {
            let coin_id = *coin.id();
            if let Some(write_ahead_log::Entry {
                uuid,
                recipient,
                tx,
                retry_count: _,
            }) = wal.reclaim(coin_id).map_err(FaucetError::internal)?
            {
                let uuid = Uuid::from_bytes(uuid);
                info!(?uuid, ?recipient, ?coin_id, "Retrying txn from WAL.");
                pending.push((uuid, recipient, coin_id, tx));
            } else {
                producer
                    .send(coin_id)
                    .await
                    .tap_ok(|_| {
                        info!(?coin_id, "Adding coin to gas pool");
                        metrics.total_available_coins.inc();
                    })
                    .tap_err(|e| error!(?coin_id, "Failed to add coin to gas pools: {e:?}"))
                    .unwrap();
            }
        }

        let faucet = Self {
            wallet,
            active_address,
            producer: Mutex::new(producer),
            consumer: Mutex::new(consumer),
            metrics,
            wal: Mutex::new(wal),
        };

        // Retrying all the pending transactions from the WAL, before continuing.  Ignore return
        // values -- if the executions failed, the pending coins will simply remain in the WAL, and
        // not recycled.
        futures::future::join_all(pending.into_iter().map(|(uuid, recipient, coin_id, tx)| {
            faucet.sign_and_execute_txn(uuid, recipient, coin_id, tx)
        }))
        .await;

        Ok(faucet)
    }

    /// Take the consumer lock and pull a Coin ID from the queue, without checking whether it is
    /// valid or not.
    async fn pop_gas_coin(&self, uuid: Uuid) -> Option<ObjectID> {
        // If the gas candidate queue is exhausted, the request will be suspended indefinitely until
        // a producer puts in more candidate gas objects. At the same time, other requests will be
        // blocked by the lock acquisition as well.
        let Ok(mut consumer) = tokio::time::timeout(LOCK_TIMEOUT, self.consumer.lock()).await else {
            error!(?uuid, "Timeout when getting consumer lock");
            return None;
        };

        info!(?uuid, "Got consumer lock, pulling coins.");
        let Ok(coin) = tokio::time::timeout(RECV_TIMEOUT, consumer.recv()).await else {
            error!(?uuid, "Timeout when getting gas coin from the queue");
            return None;
        };

        let Some(coin) = coin else {
            unreachable!("channel is closed");
        };

        self.metrics.total_available_coins.dec();
        Some(coin)
    }

    /// Pulls a coin from the queue and makes sure it is fit for use (belongs to the faucet, has
    /// sufficient balance).
    async fn prepare_gas_coin(&self, total_amount: u64, uuid: Uuid) -> GasCoinResponse {
        let Some(coin_id) = self.pop_gas_coin(uuid).await else {
            warn!("Failed getting gas coin, try later!");
            return GasCoinResponse::NoGasCoinAvailable;
        };

        match self.get_gas_coin_and_check_faucet_owner(coin_id).await {
            Ok(Some(gas_coin)) if gas_coin.value() >= total_amount => {
                info!(?uuid, ?coin_id, "balance: {}", gas_coin.value());
                GasCoinResponse::ValidGasCoin(coin_id)
            }

            Ok(Some(_)) => GasCoinResponse::GasCoinWithInsufficientBalance(coin_id),

            Ok(None) => GasCoinResponse::InvalidGasCoin(coin_id),

            Err(e) => {
                error!(?uuid, ?coin_id, "Fullnode read error: {e:?}");
                GasCoinResponse::UnknownGasCoin(coin_id)
            }
        }
    }

    /// Check if the gas coin is still valid. A valid gas coin
    /// 1. Exists presently
    /// 2. is a gas coin
    /// If the coin is valid, return Ok(Some(GasCoin))
    /// If the coin invalid, return Ok(None)
    /// If the fullnode returns an unexpected error, returns Err(e)
    async fn get_coin(
        &self,
        coin_id: ObjectID,
    ) -> anyhow::Result<Option<(Option<Owner>, GasCoin)>> {
        let client = self.wallet.get_client().await?;
        let gas_obj = client
            .read_api()
            .get_object_with_options(
                coin_id,
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_content(),
            )
            .await?;
        let o = gas_obj.data;
        if let Some(o) = o {
            Ok(GasCoin::try_from(&o).ok().map(|coin| (o.owner, coin)))
        } else {
            Ok(None)
        }
    }

    /// Similar to get_coin but checks that the owner is the active
    /// faucet address. If the coin exists, but does not have the correct owner,
    /// returns None
    async fn get_gas_coin_and_check_faucet_owner(
        &self,
        coin_id: ObjectID,
    ) -> anyhow::Result<Option<GasCoin>> {
        let gas_obj = self.get_coin(coin_id).await?;
        Ok(gas_obj.and_then(|(owner_opt, coin)| match owner_opt {
            Some(Owner::AddressOwner(owner_addr)) if owner_addr == self.active_address => {
                Some(coin)
            }
            _ => None,
        }))
    }

    /// Clear the WAL list in the faucet
    pub async fn retry_wal_coins(&self) -> Result<(), FaucetError> {
        let mut wal = self.wal.lock().await;
        let mut pending = vec![];

        for item in wal.log.safe_iter() {
            // Safe unwrap as we are the only ones that ever add to the WAL.
            let (coin_id, entry) = item.unwrap();
            let uuid = Uuid::from_bytes(entry.uuid);
            pending.push((uuid, entry.recipient, coin_id, entry.tx));
        }

        for (_, _, coin_id, _) in &pending {
            wal.increment_retry_count(*coin_id)
                .map_err(FaucetError::internal)?;
        }

        // Drops the lock early because sign_and_execute_txn requires the lock.
        drop(wal);

        futures::future::join_all(pending.into_iter().map(|(uuid, recipient, coin_id, tx)| {
            self.sign_and_execute_txn(uuid, recipient, coin_id, tx)
        }))
        .await;

        Ok(())
    }

    /// Sign an already created transaction (in `tx_data`) and keep trying to execute it until
    /// fullnode returns a definite response or a timeout is hit.
    async fn sign_and_execute_txn(
        &self,
        uuid: Uuid,
        recipient: SuiAddress,
        coin_id: ObjectID,
        tx_data: TransactionData,
    ) -> Result<SuiTransactionBlockResponse, FaucetError> {
        let signature = self
            .wallet
            .config
            .keystore
            .sign_secure(&self.active_address, &tx_data, Intent::sui_transaction())
            .map_err(FaucetError::internal)?;
        let tx = Transaction::from_data(tx_data, Intent::sui_transaction(), vec![signature])
            .verify()
            .unwrap();
        let tx_digest = *tx.digest();
        info!(
            ?tx_digest,
            ?recipient,
            ?coin_id,
            ?uuid,
            "PaySui transaction in faucet."
        );

        match timeout(
            Duration::from_secs(300),
            self.execute_pay_sui_txn_with_retries(&tx, coin_id, recipient, uuid),
        )
        .await
        {
            Err(elapsed) => {
                warn!(
                    ?recipient,
                    ?coin_id,
                    ?uuid,
                    "Failed to execute PaySui transactions in faucet after {elapsed}. Coin will \
                     not be reused."
                );
                Err(FaucetError::Transfer(
                    "could not complete transfer within timeout".into(),
                ))
            }

            Ok(result) => {
                // Note: we do not recycle gas unless the transaction was successful - the faucet
                // may run out of available coins due to errors, but this allows a human to
                // intervene and attempt to fix things. If we re-use coins that had errors, we may
                // lock them permanently.

                // It's important to remove the coin from the WAL before recycling it, to avoid a
                // race with the next request served with this coin.  If this operation fails, log
                // it and continue so we don't lose access to the coin -- the worst that can happen
                // is that the WAL contains a stale entry.
                if self.wal.lock().await.commit(coin_id).is_err() {
                    error!(?coin_id, "Failed to remove coin from WAL");
                }
                self.recycle_gas_coin(coin_id, uuid).await;
                Ok(result)
            }
        }
    }

    async fn transfer_gases(
        &self,
        amounts: &[u64],
        recipient: SuiAddress,
        uuid: Uuid,
    ) -> Result<(TransactionDigest, Vec<ObjectID>), FaucetError> {
        let number_of_coins = amounts.len();
        let total_amount: u64 = amounts.iter().sum();
        let gas_cost = self.get_gas_cost().await?;

        let gas_coin_response = self.prepare_gas_coin(total_amount + gas_cost, uuid).await;
        match gas_coin_response {
            GasCoinResponse::ValidGasCoin(coin_id) => {
                let tx_data = self
                    .build_pay_sui_txn(coin_id, self.active_address, recipient, amounts, gas_cost)
                    .await
                    .map_err(FaucetError::internal)?;

                {
                    // Register the intention to send this transaction before we send it, so that if
                    // faucet fails or we give up before we get a definite response, we have a
                    // chance to retry later.
                    let mut wal = self.wal.lock().await;
                    wal.reserve(uuid, coin_id, recipient, tx_data.clone())
                        .map_err(FaucetError::internal)?;
                }
                let response = self
                    .sign_and_execute_txn(uuid, recipient, coin_id, tx_data)
                    .await?;
                self.check_and_map_transfer_gas_result(response, number_of_coins, recipient)
                    .await
            }

            GasCoinResponse::UnknownGasCoin(coin_id) => {
                self.recycle_gas_coin(coin_id, uuid).await;
                Err(FaucetError::FullnodeReadingError(format!(
                    "unknown gas coin {coin_id:?}"
                )))
            }

            GasCoinResponse::GasCoinWithInsufficientBalance(coin_id) => {
                warn!(?uuid, ?coin_id, "Insufficient balance, removing from pool");
                self.metrics.total_discarded_coins.inc();
                Err(FaucetError::GasCoinWithInsufficientBalance(
                    coin_id.to_hex_uncompressed(),
                ))
            }

            GasCoinResponse::InvalidGasCoin(coin_id) => {
                // The coin does not exist, or does not belong to the current active address.
                warn!(?uuid, ?coin_id, "Invalid, removing from pool");
                self.metrics.total_discarded_coins.inc();
                Err(FaucetError::InvalidGasCoin(coin_id.to_hex_uncompressed()))
            }

            GasCoinResponse::NoGasCoinAvailable => Err(FaucetError::NoGasCoinAvailable),
        }
    }

    async fn recycle_gas_coin(&self, coin_id: ObjectID, uuid: Uuid) {
        // Once transactions are done, in despite of success or failure,
        // we put back the coins. The producer should never wait indefinitely,
        // in that the channel is initialized with big enough capacity.
        let producer = self.producer.lock().await;
        info!(?uuid, ?coin_id, "Got producer lock and recycling coin");
        producer
            .try_send(coin_id)
            .expect("unexpected - queue is large enough to hold all coins");
        self.metrics.total_available_coins.inc();
        info!(?uuid, ?coin_id, "Recycled coin");
    }

    async fn execute_pay_sui_txn_with_retries(
        &self,
        tx: &VerifiedTransaction,
        coin_id: ObjectID,
        recipient: SuiAddress,
        uuid: Uuid,
    ) -> SuiTransactionBlockResponse {
        let mut retry_delay = Duration::from_millis(500);

        loop {
            let res = self.execute_pay_sui_txn(tx, coin_id, recipient, uuid).await;

            if let Ok(res) = res {
                return res;
            }

            info!(
                ?recipient,
                ?coin_id,
                ?uuid,
                ?retry_delay,
                "PaySui transaction in faucet failed, previous error: {:?}",
                &res,
            );

            tokio::time::sleep(retry_delay).await;
            retry_delay *= 2;
        }
    }

    async fn execute_pay_sui_txn(
        &self,
        tx: &VerifiedTransaction,
        coin_id: ObjectID,
        recipient: SuiAddress,
        uuid: Uuid,
    ) -> Result<SuiTransactionBlockResponse, anyhow::Error> {
        self.metrics.current_executions_in_flight.inc();
        let _metrics_guard = scopeguard::guard(self.metrics.clone(), |metrics| {
            metrics.current_executions_in_flight.dec();
        });

        let tx_digest = tx.digest();
        let client = self.wallet.get_client().await?;
        Ok(client
            .quorum_driver_api()
            .execute_transaction_block(
                tx.clone(),
                SuiTransactionBlockResponseOptions::new().with_effects(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await
            .tap_err(|e| {
                error!(
                    ?tx_digest,
                    ?recipient,
                    ?coin_id,
                    ?uuid,
                    "Transfer Transaction failed: {:?}",
                    e
                )
            })?)
    }

    async fn get_gas_cost(&self) -> Result<u64, FaucetError> {
        let gas_price = self.get_gas_price().await?;
        Ok(gas_price * DEFAULT_GAS_COMPUTATION_BUCKET)
    }

    async fn get_gas_price(&self) -> Result<u64, FaucetError> {
        let client = self
            .wallet
            .get_client()
            .await
            .map_err(|e| FaucetError::Wallet(format!("Unable to get client: {e:?}")))?;
        client
            .read_api()
            .get_reference_gas_price()
            .await
            .map_err(|e| FaucetError::FullnodeReadingError(format!("Error fetch gas price {e:?}")))
    }

    async fn build_pay_sui_txn(
        &self,
        coin_id: ObjectID,
        signer: SuiAddress,
        recipient: SuiAddress,
        amounts: &[u64],
        budget: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        let recipients: Vec<SuiAddress> =
            std::iter::repeat(recipient).take(amounts.len()).collect();
        let client = self.wallet.get_client().await?;
        client
            .transaction_builder()
            .pay_sui(signer, vec![coin_id], recipients, amounts.to_vec(), budget)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to build PaySui transaction for coin {:?}, with err {:?}",
                    coin_id,
                    e
                )
            })
    }

    async fn check_and_map_transfer_gas_result(
        &self,
        res: SuiTransactionBlockResponse,
        number_of_coins: usize,
        recipient: SuiAddress,
    ) -> Result<(TransactionDigest, Vec<ObjectID>), FaucetError> {
        let created = res
            .effects
            .ok_or_else(|| {
                FaucetError::ParseTransactionResponseError(format!(
                    "effects field missing for txn {}",
                    res.digest
                ))
            })?
            .created()
            .to_vec();
        if created.len() != number_of_coins {
            panic!(
                "PaySui Transaction should create exact {:?} new coins, but got {:?}",
                number_of_coins, created
            );
        }
        assert!(created
            .iter()
            .all(|created_coin_owner_ref| created_coin_owner_ref.owner == recipient));
        let coin_ids: Vec<ObjectID> = created
            .iter()
            .map(|created_coin_owner_ref| created_coin_owner_ref.reference.object_id)
            .collect();
        Ok((res.digest, coin_ids))
    }

    #[cfg(test)]
    async fn drain_gas_queue(&mut self, expected_gas_count: usize) -> HashSet<ObjectID> {
        use tokio::sync::mpsc::error::TryRecvError;
        let mut consumer = self.consumer.lock().await;
        let mut candidates = HashSet::new();
        let mut i = 0;
        loop {
            let coin_id = consumer
                .try_recv()
                .unwrap_or_else(|e| panic!("Expect the {}th candidate but got {}", i, e));
            candidates.insert(coin_id);
            i += 1;
            if i == expected_gas_count {
                assert_eq!(consumer.try_recv().unwrap_err(), TryRecvError::Empty);
                break;
            }
        }
        candidates
    }

    #[cfg(test)]
    pub fn wallet_mut(&mut self) -> &mut WalletContext {
        &mut self.wallet
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

        let (digest, coin_ids) = self.transfer_gases(amounts, recipient, id).await?;

        info!(uuid = ?id, ?recipient, ?digest, "PaySui txn succeeded");
        let mut sent = Vec::with_capacity(coin_ids.len());
        let coin_results =
            futures::future::join_all(coin_ids.iter().map(|coin_id| self.get_coin(*coin_id))).await;
        for (coin_id, res) in coin_ids.into_iter().zip(coin_results) {
            let amount = if let Ok(Some((_, coin))) = res {
                coin.value()
            } else {
                info!(
                    ?recipient,
                    ?coin_id,
                    uuid = ?id,
                    "Could not find coin after successful transaction, error: {:?}",
                    &res,
                );
                0
            };
            sent.push(CoinInfo {
                transfer_tx_digest: digest,
                amount,
                id: coin_id,
            });
        }
        Ok(FaucetReceipt { sent })
    }
}

#[cfg(test)]
mod tests {
    use sui::client_commands::{SuiClientCommandResult, SuiClientCommands};
    use sui_json_rpc_types::SuiExecutionStatus;
    use sui_sdk::wallet_context::WalletContext;
    use test_utils::network::TestClusterBuilder;

    use super::*;

    #[tokio::test]
    async fn simple_faucet_basic_interface_should_work() {
        telemetry_subscribers::init_for_testing();
        let test_cluster = TestClusterBuilder::new().build().await.unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let prom_registry = Registry::new();
        let config = FaucetConfig::default();
        let faucet = SimpleFaucet::new(
            test_cluster.wallet,
            &prom_registry,
            &tmp.path().join("faucet.wal"),
            config,
        )
        .await
        .unwrap();

        let available = faucet.metrics.total_available_coins.get();
        let discarded = faucet.metrics.total_discarded_coins.get();

        test_basic_interface(&faucet).await;

        assert_eq!(available, faucet.metrics.total_available_coins.get());
        assert_eq!(discarded, faucet.metrics.total_discarded_coins.get());
    }

    #[tokio::test]
    async fn test_init_gas_queue() {
        let test_cluster = TestClusterBuilder::new().build().await.unwrap();
        let address = test_cluster.get_address_0();
        let mut context = test_cluster.wallet;
        let gases = get_current_gases(address, &mut context).await;
        let gases = HashSet::from_iter(gases.into_iter().map(|gas| *gas.id()));

        let tmp = tempfile::tempdir().unwrap();
        let prom_registry = Registry::new();
        let config = FaucetConfig::default();
        let mut faucet = SimpleFaucet::new(
            context,
            &prom_registry,
            &tmp.path().join("faucet.wal"),
            config,
        )
        .await
        .unwrap();

        let available = faucet.metrics.total_available_coins.get();
        let candidates = faucet.drain_gas_queue(gases.len()).await;

        assert_eq!(available as usize, candidates.len());
        assert_eq!(
            candidates, gases,
            "gases: {:?}, candidates: {:?}",
            gases, candidates
        );
    }

    #[tokio::test]
    async fn test_transfer_state() {
        let test_cluster = TestClusterBuilder::new().build().await.unwrap();
        let address = test_cluster.get_address_0();
        let mut context = test_cluster.wallet;
        let gases = get_current_gases(address, &mut context).await;

        let gases = HashSet::from_iter(gases.into_iter().map(|gas| *gas.id()));

        let tmp = tempfile::tempdir().unwrap();
        let prom_registry = Registry::new();
        let config = FaucetConfig::default();
        let mut faucet = SimpleFaucet::new(
            context,
            &prom_registry,
            &tmp.path().join("faucet.wal"),
            config,
        )
        .await
        .unwrap();

        let number_of_coins = gases.len();
        let amounts = &vec![1; number_of_coins];
        let _ = futures::future::join_all((0..30).map(|_| {
            faucet.send(
                Uuid::new_v4(),
                SuiAddress::random_for_testing_only(),
                amounts,
            )
        }))
        .await
        .into_iter()
        .map(|res| res.unwrap())
        .collect::<Vec<_>>();
        // After all transfer requests settle, we still have the original candidates gas in queue.
        let available = faucet.metrics.total_available_coins.get();
        let candidates = faucet.drain_gas_queue(gases.len()).await;
        assert_eq!(available as usize, candidates.len());
        assert_eq!(
            candidates, gases,
            "gases: {:?}, candidates: {:?}",
            gases, candidates
        );
    }

    #[tokio::test]
    async fn test_discard_invalid_gas() {
        let test_cluster = TestClusterBuilder::new().build().await.unwrap();
        let address = test_cluster.get_address_0();
        let mut context = test_cluster.wallet;
        let mut gases = get_current_gases(address, &mut context).await;

        let bad_gas = gases.swap_remove(0);
        let gases = HashSet::from_iter(gases.into_iter().map(|gas| *gas.id()));

        let tmp = tempfile::tempdir().unwrap();
        let prom_registry = Registry::new();
        let config = FaucetConfig::default();
        let mut faucet = SimpleFaucet::new(
            context,
            &prom_registry,
            &tmp.path().join("faucet.wal"),
            config,
        )
        .await
        .unwrap();

        // Now we transfer one gas out
        let res = SuiClientCommands::PayAllSui {
            input_coins: vec![*bad_gas.id()],
            recipient: SuiAddress::random_for_testing_only(),
            gas_budget: 2_000_000,
            serialize_output: false,
        }
        .execute(faucet.wallet_mut())
        .await
        .unwrap();

        if let SuiClientCommandResult::PayAllSui(response) = res {
            assert!(matches!(
                response.effects.unwrap().status(),
                SuiExecutionStatus::Success
            ));
        } else {
            panic!("PayAllSui command did not return SuiClientCommandResult::PayAllSui");
        };

        let number_of_coins = gases.len();
        let amounts = &vec![1; number_of_coins];
        // We traverse the the list twice, which must trigger the transferred gas to be kicked out
        futures::future::join_all((0..2).map(|_| {
            faucet.send(
                Uuid::new_v4(),
                SuiAddress::random_for_testing_only(),
                amounts,
            )
        }))
        .await;

        // Verify that the bad gas is no longer in the queue.
        // Note `gases` does not contain the bad gas.
        let available = faucet.metrics.total_available_coins.get();
        let discarded = faucet.metrics.total_discarded_coins.get();
        let candidates = faucet.drain_gas_queue(gases.len()).await;
        assert_eq!(available as usize, candidates.len());
        assert_eq!(discarded, 1);
        assert_eq!(
            candidates, gases,
            "gases: {:?}, candidates: {:?}",
            gases, candidates
        );
    }

    #[tokio::test]
    async fn test_clear_wal() {
        telemetry_subscribers::init_for_testing();
        let test_cluster = TestClusterBuilder::new().build().await.unwrap();
        let context = test_cluster.wallet;
        let tmp = tempfile::tempdir().unwrap();
        let prom_registry = Registry::new();
        let config = FaucetConfig::default();
        let mut faucet = SimpleFaucet::new(
            context,
            &prom_registry,
            &tmp.path().join("faucet.wal"),
            config,
        )
        .await
        .unwrap();

        let original_available = faucet.metrics.total_available_coins.get();
        let original_discarded = faucet.metrics.total_discarded_coins.get();

        let recipient = SuiAddress::random_for_testing_only();
        let faucet_address = faucet.wallet_mut().active_address().unwrap();
        let uuid = Uuid::new_v4();
        let coin_response = faucet.prepare_gas_coin(100, uuid).await;

        match coin_response {
            GasCoinResponse::ValidGasCoin(gas) => {
                let tx_data = faucet
                    .build_pay_sui_txn(gas, faucet_address, recipient, &[100], 200_000_000)
                    .await
                    .map_err(FaucetError::internal)
                    .unwrap();

                let mut wal = faucet.wal.lock().await;
                // Check no wal
                assert!(wal.log.is_empty());
                wal.reserve(Uuid::new_v4(), gas, recipient, tx_data)
                    .map_err(FaucetError::internal)
                    .ok();
                // Check the wal has one entry
                assert!(!wal.log.is_empty());
                drop(wal);
            }
            _ => {
                panic!("prepare_gas_coin_did not give a valid function.");
            }
        }

        faucet.retry_wal_coins().await.ok();

        let wal_2 = faucet.wal.lock().await;
        assert!(wal_2.log.is_empty());
        let total_coins = faucet.metrics.total_available_coins.get();
        let discarded_coins = faucet.metrics.total_discarded_coins.get();
        assert_eq!(total_coins, original_available);
        assert_eq!(discarded_coins, original_discarded);
    }

    #[tokio::test]
    async fn test_discard_smaller_amount_gas() {
        telemetry_subscribers::init_for_testing();
        let test_cluster = TestClusterBuilder::new().build().await.unwrap();
        let address = test_cluster.get_address_0();
        let mut context = test_cluster.wallet;
        let gases = get_current_gases(address, &mut context).await;

        // split out a coin that has a very small balance such that
        // this coin will be not used later on. This is the new default amount for faucet due to gas changes
        let config = FaucetConfig::default();
        let tiny_value = (config.num_coins as u64 * config.amount) + 1;
        let res = SuiClientCommands::SplitCoin {
            coin_id: *gases[0].id(),
            amounts: Some(vec![tiny_value]),
            gas_budget: 50000000,
            gas: None,
            count: None,
            serialize_output: false,
        }
        .execute(&mut context)
        .await;

        let tiny_coin_id = if let SuiClientCommandResult::SplitCoin(resp) = res.unwrap() {
            resp.effects.as_ref().unwrap().created()[0]
                .reference
                .object_id
        } else {
            panic!("split command did not return SuiClientCommandResult::SplitCoin");
        };

        // Get the latest list of gas
        let gases = get_current_gases(address, &mut context).await;
        let tiny_amount = gases
            .iter()
            .find(|gas| gas.id() == &tiny_coin_id)
            .unwrap()
            .value();
        assert_eq!(tiny_amount, tiny_value);
        println!("gas list: {gases:?}");

        let gases: HashSet<ObjectID> = HashSet::from_iter(gases.into_iter().map(|gas| *gas.id()));

        let tmp = tempfile::tempdir().unwrap();
        let prom_registry = Registry::new();
        let mut faucet = SimpleFaucet::new(
            context,
            &prom_registry,
            &tmp.path().join("faucet.wal"),
            config,
        )
        .await
        .unwrap();

        // Ask for a value higher than tiny coin + DEFAULT_GAS_COMPUTATION_BUCKET
        let number_of_coins = gases.len();
        let amounts = &vec![tiny_value + 1; number_of_coins - 1];
        // We traverse the the list ten times, which must trigger the tiny gas to be examined and then discarded
        futures::future::join_all((0..10).map(|_| {
            faucet.send(
                Uuid::new_v4(),
                SuiAddress::random_for_testing_only(),
                amounts,
            )
        }))
        .await;
        info!(
            ?number_of_coins,
            "Sent to random addresses: {} {}",
            amounts[0],
            amounts.len(),
        );

        // Verify that the tiny gas is not in the queue.
        tokio::task::yield_now().await;
        let discarded = faucet.metrics.total_discarded_coins.get();
        let candidates = faucet.drain_gas_queue(gases.len() - 1).await;
        assert_eq!(discarded, 1);
        assert!(candidates.get(&tiny_coin_id).is_none());
    }

    async fn test_basic_interface(faucet: &impl Faucet) {
        let recipient = SuiAddress::random_for_testing_only();
        let amounts = vec![1, 2, 3];

        let FaucetReceipt { sent } = faucet
            .send(Uuid::new_v4(), recipient, &amounts)
            .await
            .unwrap();
        let mut actual_amounts: Vec<u64> = sent.iter().map(|c| c.amount).collect();
        actual_amounts.sort_unstable();
        assert_eq!(actual_amounts, amounts);
    }

    async fn get_current_gases(address: SuiAddress, context: &mut WalletContext) -> Vec<GasCoin> {
        // Get the latest list of gas
        let results = SuiClientCommands::Gas {
            address: Some(address),
        }
        .execute(context)
        .await
        .unwrap();
        match results {
            SuiClientCommandResult::Gas(gases) => gases,
            other => panic!("Expect SuiClientCommandResult::Gas, but got {:?}", other),
        }
    }
}
