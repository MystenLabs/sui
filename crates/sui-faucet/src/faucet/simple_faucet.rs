// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::FaucetMetrics;
use anyhow::anyhow;
use async_trait::async_trait;
use prometheus::Registry;
use tap::tap::TapFallible;

#[cfg(test)]
use std::collections::HashSet;

use sui::client_commands::{SuiClientCommands, WalletContext};
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiObjectRead, SuiTransactionKind, SuiTransactionResponse, SuiTransferSui,
};
use sui_keys::keystore::AccountKeystore;
use sui_types::object::Owner;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    gas_coin::GasCoin,
    intent::Intent,
    messages::{ExecuteTransactionRequestType, Transaction, TransactionData},
};
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{CoinInfo, Faucet, FaucetError, FaucetReceipt};

pub struct SimpleFaucet {
    wallet: WalletContext,
    active_address: SuiAddress,
    producer: Mutex<Sender<ObjectID>>,
    consumer: Mutex<Receiver<ObjectID>>,
    metrics: FaucetMetrics,
}

const DEFAULT_GAS_BUDGET: u64 = 1000;
const TRANSFER_SUI_GAS: u64 = 100;

impl SimpleFaucet {
    pub async fn new(
        mut wallet: WalletContext,
        prometheus_registry: &Registry,
    ) -> Result<Self, FaucetError> {
        let active_address = wallet
            .active_address()
            .map_err(|err| FaucetError::Wallet(err.to_string()))?;
        info!("SimpleFaucet::new with active address: {active_address}");

        // Sync to have the latest status
        if wallet.client.is_gateway() {
            SuiClientCommands::SyncClientState {
                address: Some(active_address),
            }
            .execute(&mut wallet)
            .await
            .map_err(|err| FaucetError::Wallet(format!("Fail to sync client state: {}", err)))?;
        }

        let coins = wallet
            .gas_objects(active_address)
            .await
            .map_err(|e| FaucetError::Wallet(e.to_string()))?
            .iter()
            // Ok to unwrap() since `get_gas_objects` guarantees gas
            .map(|q| GasCoin::try_from(&q.1).unwrap())
            .collect::<Vec<GasCoin>>();

        let (producer, consumer) = mpsc::channel(coins.len());
        for coin in &coins {
            let coin_id = *coin.id();
            producer
                .send(coin_id)
                .await
                .tap_ok(|_| info!("Adding coin {:?} to gas pool", coin_id))
                .tap_err(|e| error!("Failed to add gas coin {} to pools: {:?}", coin_id, e))
                .unwrap();
        }

        let metrics = FaucetMetrics::new(prometheus_registry);

        Ok(Self {
            wallet,
            active_address,
            producer: Mutex::new(producer),
            consumer: Mutex::new(consumer),
            metrics,
        })
    }

    async fn select_coins(
        &self,
        number_of_coins: usize,
        transfer_amount: u64,
        uuid: Uuid,
    ) -> anyhow::Result<Vec<ObjectID>> {
        assert!(number_of_coins > 0);
        // If the gas candidate queue is exhausted, the request will be
        // suspended indefinitely until a producer puts in more candidate
        // gas objects. At the same time, other requests will be blocked by the
        // lock acquisition as well.
        let mut consumer = self.consumer.lock().await;
        debug!(?uuid, "Got consumer lock, pulling coins.");
        let mut coins = Vec::with_capacity(number_of_coins);
        loop {
            match tokio::time::timeout(Duration::from_secs(30), consumer.recv()).await {
                Ok(Some(coin)) => {
                    debug!(?uuid, "Pulling coin from pool {:?}", coin);
                    let gas_coin = self.get_gas_coin(coin).await?;
                    if let Some(gas_coin) = gas_coin {
                        if gas_coin.value() >= transfer_amount + TRANSFER_SUI_GAS {
                            info!(
                                ?uuid,
                                "Planning to use coin from pool {:?}, current balance: {}",
                                coin,
                                gas_coin.value()
                            );
                            coins.push(coin);
                            if coins.len() == number_of_coins {
                                break;
                            }
                        } else {
                            // If amount is not big enough, remove it
                            warn!(
                                ?uuid,
                                "Coin {:?} does not have enough balance ({:?}), removing from pool",
                                coin,
                                gas_coin.value(),
                            );
                        }
                    } else {
                        // Invalid gas, remove it
                        warn!(
                            ?uuid,
                            "Coin {:?} is not longer valid, removing from pool", coin
                        );
                    }
                }
                Ok(None) => {
                    unreachable!("channel is closed");
                }
                Err(_) => {
                    error!(?uuid, "Timeout when getting coins from the queue");
                    break;
                }
            }
        }

        Ok(coins)
    }

    /// Check if the gas coin is still valid. A valid gas coin is
    /// 1. existent presently
    /// 2. is a GasCoin
    /// 3. still belongs to facuet account
    /// If the coin is valid, return Ok(Some(GasCoin))
    /// If the coin invalid, return Ok(None)
    async fn get_gas_coin(&self, coin_id: ObjectID) -> anyhow::Result<Option<GasCoin>> {
        let gas_obj = self
            .wallet
            .client
            .read_api()
            .get_parsed_object(coin_id)
            .await?;
        Ok(match gas_obj {
            SuiObjectRead::NotExists(_) | SuiObjectRead::Deleted(_) => None,
            SuiObjectRead::Exists(obj) => match &obj.owner {
                Owner::AddressOwner(owner_addr) => {
                    if owner_addr == &self.active_address {
                        GasCoin::try_from(&obj).ok()
                    } else {
                        None
                    }
                }
                _ => None,
            },
        })
    }

    async fn transfer_gases(
        &self,
        amounts: &[u64],
        to: SuiAddress,
        uuid: Uuid,
    ) -> Result<Vec<(TransactionDigest, ObjectID, u64, ObjectID)>, FaucetError> {
        let number_of_coins = amounts.len();
        // We assume the amounts are the same
        let coins = self
            .select_coins(number_of_coins, amounts[0], uuid)
            .await
            .tap_ok(|res| {
                debug!(recipient=?to, ?uuid, "Planning to use coins: {:?}", res);
            })
            .tap_err(|err| error!(?uuid, "Failed to select coins: {:?}", err.to_string()))
            .map_err(|err| {
                FaucetError::Internal(format!("Failed to select coins: {:?}", err.to_string()))
            })?;

        if coins.is_empty() {
            return Err(FaucetError::Internal("Failed to select coins".into()));
        }

        let futures: Vec<_> = coins
            .iter()
            .zip(amounts)
            .map(|(coin_id, amount)| {
                self.transfer_sui(
                    *coin_id,
                    self.active_address,
                    to,
                    DEFAULT_GAS_BUDGET,
                    *amount,
                    uuid,
                )
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        // Once transactions are done, in despite of success or failure,
        // we put back the coins. The producer should never wait indefinitely,
        // in that the channel is initialized with big enough capacity.
        let producer = self.producer.lock().await;
        debug!(?uuid, "Got producer lock, putting back coins.");
        for coin in coins {
            producer
                .try_send(coin)
                .expect("unexpected - queue is large enough to hold all coins");
            debug!(?uuid, "Recycling coin {:?}", coin);
        }
        drop(producer);

        let responses: Vec<_> = results
            .into_iter()
            .filter(|res| {
                if res.is_ok() {
                    true
                } else {
                    error!(?uuid, "Encountered error in transfer sui: {:?}", res);
                    false
                }
            })
            .map(|res| {
                let response = res.unwrap();
                let txns = response.certificate.data.transactions;
                if txns.len() != 1 {
                    panic!("TransferSui Transaction should create one and exactly one txn, but got {:?}", txns);
                }
                let created = response.effects.created;
                if created.len() != 1 {
                    panic!("TransferSui Transaction should create one and exactly one object, but got {:?}", created);
                }
                let txn = &txns[0];
                let obj = &created[0];
                if let SuiTransactionKind::TransferSui(SuiTransferSui{recipient, amount: Some(amount)}) = txn {
                    assert_eq!(to, *recipient);
                    (response.certificate.transaction_digest, obj.reference.object_id, *amount, response.certificate.data.gas_payment.object_id)
                } else {
                    panic!("Expect SuiTransactionKind::TransferSui(SuiTransferSui) to address {} with Some(amount) but got {:?}", to, txn);
                }
            })
            .collect();

        Ok(responses)
    }

    async fn construct_transfer_sui_txn_with_retry(
        &self,
        coin_id: ObjectID,
        signer: SuiAddress,
        recipient: SuiAddress,
        budget: u64,
        amount: u64,
        uuid: Uuid,
    ) -> Result<TransactionData, anyhow::Error> {
        // if needed, retry 2 times with the following interval
        let retry_intervals_ms = [Duration::from_millis(500), Duration::from_millis(1000)];

        let mut data = self
            .construct_transfer_sui_txn(coin_id, signer, recipient, budget, amount)
            .await;
        let mut iter = retry_intervals_ms.iter();
        while data.is_err() {
            if let Some(duration) = iter.next() {
                tokio::time::sleep(*duration).await;
                debug!(
                    ?recipient,
                    ?coin_id,
                    ?uuid,
                    "Retrying constructing TransferSui txn. Previous error: {:?}",
                    &data,
                );
            } else {
                warn!(
                    ?recipient,
                    ?coin_id,
                    ?uuid,
                    "Failed to construct TransferSui txn after {} retries with interval {:?}",
                    retry_intervals_ms.len(),
                    &retry_intervals_ms
                );
                break;
            }
            data = self
                .construct_transfer_sui_txn(coin_id, signer, recipient, budget, amount)
                .await;
        }
        data
    }

    async fn construct_transfer_sui_txn(
        &self,
        coin_id: ObjectID,
        signer: SuiAddress,
        recipient: SuiAddress,
        budget: u64,
        amount: u64,
    ) -> Result<TransactionData, anyhow::Error> {
        self.wallet
            .client
            .transaction_builder()
            .transfer_sui(signer, coin_id, budget, recipient, Some(amount))
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to construct TransferSui transaction for coin {:?}, {:?}",
                    coin_id,
                    e
                )
            })
    }

    async fn transfer_sui(
        &self,
        coin_id: ObjectID,
        signer: SuiAddress,
        recipient: SuiAddress,
        budget: u64,
        amount: u64,
        uuid: Uuid,
    ) -> Result<SuiTransactionResponse, anyhow::Error> {
        let context = &self.wallet;
        let data = self
            .construct_transfer_sui_txn_with_retry(coin_id, signer, recipient, budget, amount, uuid)
            .await?;

        let signature = context
            .config
            .keystore
            .sign_secure(&signer, &data, Intent::default())?;

        let tx = Transaction::new(data, Intent::default(), signature);
        let tx_digest = *tx.digest();
        info!(
            ?tx_digest,
            ?recipient,
            ?coin_id,
            ?uuid,
            "Broadcasting transfer obj txn"
        );
        let response = context
            .client
            .quorum_driver()
            .execute_transaction(
                tx,
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
            })?;
        let tx_cert = response
            .tx_cert
            .ok_or_else(|| anyhow!("Expect Some(tx_cert)"))?;
        let effects = response
            .effects
            .ok_or_else(|| anyhow!("Expect Some(effects)"))?;
        if matches!(effects.status, SuiExecutionStatus::Failure { .. }) {
            return Err(anyhow!("Error transferring object: {:#?}", effects.status));
        }

        Ok(SuiTransactionResponse {
            certificate: tx_cert,
            effects,
            timestamp_ms: None,
            parsed_data: None,
        })
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

        self.metrics.total_requests_received.inc();
        self.metrics.current_requests_in_flight.inc();

        let _metrics_guard = scopeguard::guard(self.metrics.clone(), |metrics| {
            metrics.current_requests_in_flight.dec();
        });

        let timer = self.metrics.process_latency.start_timer();

        let results = self.transfer_gases(amounts, recipient, id).await?;

        if results.len() != amounts.len() {
            error!(
                uuid = ?id, ?recipient,
                "Requested {} coins but only got {}",
                amounts.len(),
                results.len()
            );
        }

        if results.is_empty() {
            return Err(FaucetError::Transfer(
                "Failed to transfer any coins to the requestor".into(),
            ));
        }

        let elapsed = timer.stop_and_record();

        info!(uuid = ?id, ?recipient, ?results, "Transfer txn succeeded in {} secs", elapsed);
        self.metrics.total_requests_succeeded.inc();

        Ok(FaucetReceipt {
            sent: results
                .iter()
                .map(|(digest, obj_id, amount, _gas_id)| CoinInfo {
                    transfer_tx_digest: *digest,
                    amount: *amount,
                    id: *obj_id,
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use sui::client_commands::{SuiClientCommandResult, SuiClientCommands};
    use test_utils::network::TestClusterBuilder;

    use super::*;

    #[tokio::test]
    async fn simple_faucet_basic_interface_should_work() {
        telemetry_subscribers::init_for_testing();
        let test_cluster = TestClusterBuilder::new().build().await.unwrap();

        let prom_registry = prometheus::Registry::new();
        let faucet = SimpleFaucet::new(test_cluster.wallet, &prom_registry)
            .await
            .unwrap();
        test_basic_interface(faucet).await;
    }

    #[tokio::test]
    async fn test_init_gas_queue() {
        let test_cluster = TestClusterBuilder::new().build().await.unwrap();
        let address = test_cluster.get_address_0();
        let mut context = test_cluster.wallet;
        let gases = get_current_gases(address, &mut context).await;
        let gases = HashSet::from_iter(gases.into_iter().map(|gas| *gas.id()));
        let prom_registry = Registry::new();
        let mut faucet = SimpleFaucet::new(context, &prom_registry).await.unwrap();

        let candidates = faucet.drain_gas_queue(gases.len()).await;
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

        let prom_registry = prometheus::Registry::new();
        let mut faucet = SimpleFaucet::new(context, &prom_registry).await.unwrap();

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
        let candidates = faucet.drain_gas_queue(gases.len()).await;
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

        let prom_registry = prometheus::Registry::new();
        let mut faucet = SimpleFaucet::new(context, &prom_registry).await.unwrap();

        // Now we transfer one gas out
        let res = SuiClientCommands::TransferSui {
            to: SuiAddress::random_for_testing_only(),
            sui_coin_object_id: *bad_gas.id(),
            amount: None,
            gas_budget: 50000,
        }
        .execute(faucet.wallet_mut())
        .await
        .unwrap();

        if let SuiClientCommandResult::TransferSui(_tx_cert, effects) = res {
            assert!(matches!(effects.status, SuiExecutionStatus::Success));
        } else {
            panic!("transfer command did not return SuiClientCommandResult::TransferSui");
        };

        let number_of_coins = gases.len();
        let amounts = &vec![1; number_of_coins];
        // We traverse the the list twice, which must trigger the transferred gas to be kicked out
        let _ = futures::future::join_all((0..2).map(|_| {
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

        // Verify that the bad gas is no longer in the queue.
        // Note `gases` does not contain the bad gas.
        let candidates = faucet.drain_gas_queue(gases.len()).await;
        assert_eq!(
            candidates, gases,
            "gases: {:?}, candidates: {:?}",
            gases, candidates
        );
    }

    #[tokio::test]
    async fn test_discard_smaller_amount_gas() {
        telemetry_subscribers::init_for_testing();
        let test_cluster = TestClusterBuilder::new().build().await.unwrap();
        let address = test_cluster.get_address_0();
        let mut context = test_cluster.wallet;
        let gases = get_current_gases(address, &mut context).await;

        // split out a coin that has a very small balance such that
        // this coin will be not used later on.
        let tiny_value = 1;
        let res = SuiClientCommands::SplitCoin {
            coin_id: *gases[0].id(),
            amounts: Some(vec![tiny_value + TRANSFER_SUI_GAS]),
            gas_budget: 50000,
            gas: None,
            count: None,
        }
        .execute(&mut context)
        .await
        .unwrap();

        let tiny_coin_id = if let SuiClientCommandResult::SplitCoin(resp) = res {
            assert!(matches!(resp.effects.status, SuiExecutionStatus::Success));
            resp.effects.created[0].reference.object_id
        } else {
            panic!("transfer command did not return SuiClientCommandResult::TransferSui");
        };

        // Get the latest list of gas
        let gases = get_current_gases(address, &mut context).await;
        let tiny_amount = gases
            .iter()
            .find(|gas| gas.id() == &tiny_coin_id)
            .unwrap()
            .value();
        assert_eq!(tiny_amount, tiny_value + TRANSFER_SUI_GAS);

        let gases: HashSet<ObjectID> = HashSet::from_iter(gases.into_iter().map(|gas| *gas.id()));

        let prom_registry = Registry::new();
        let mut faucet = SimpleFaucet::new(context, &prom_registry).await.unwrap();

        let number_of_coins = gases.len();
        // Ask for a value higher than tiny coin + TRANSFER_SUI_GAS
        let amounts = &vec![tiny_value + 1; number_of_coins - 1];
        // We traverse the the list twice, which must trigger the tiny gas to be examined but not used
        let _ = futures::future::join_all((0..2).map(|_| {
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

        // Verify that the tiny gas is still in the queue.
        let candidates = faucet.drain_gas_queue(gases.len() - 1).await;
        assert!(candidates.get(&tiny_coin_id).is_none());
    }

    async fn test_basic_interface(faucet: impl Faucet) {
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
