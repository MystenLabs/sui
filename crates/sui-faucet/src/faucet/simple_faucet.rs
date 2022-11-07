// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::FaucetMetrics;
use anyhow::anyhow;
use async_trait::async_trait;
use prometheus::Registry;
use tap::tap::TapFallible;

#[cfg(test)]
use std::collections::HashSet;

use sui::client_commands::WalletContext;
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiObjectRead, SuiPaySui, SuiTransactionKind, SuiTransactionResponse,
};
use sui_keys::keystore::AccountKeystore;
use sui_types::object::Owner;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    gas_coin::GasCoin,
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
const PAY_SUI_GAS: u64 = 1000;

impl SimpleFaucet {
    pub async fn new(
        mut wallet: WalletContext,
        prometheus_registry: &Registry,
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

    async fn prepare_gas_coin(
        &self,
        total_amount: u64,
        uuid: Uuid,
    ) -> anyhow::Result<Option<ObjectID>> {
        // If the gas candidate queue is exhausted, the request will be
        // suspended indefinitely until a producer puts in more candidate
        // gas objects. At the same time, other requests will be blocked by the
        // lock acquisition as well.
        let mut consumer = self.consumer.lock().await;
        debug!(?uuid, "Got consumer lock, pulling coins.");
        loop {
            match tokio::time::timeout(Duration::from_secs(30), consumer.recv()).await {
                Ok(Some(coin)) => {
                    debug!(?uuid, "Pulling coin from pool {:?}", coin);
                    let gas_coin = self.get_gas_coin(coin).await?;
                    if let Some(gas_coin) = gas_coin {
                        if gas_coin.value() >= total_amount + PAY_SUI_GAS {
                            info!(
                                ?uuid,
                                "Planning to use coin from pool {:?}, current balance: {}",
                                coin,
                                gas_coin.value()
                            );
                            return Ok(Some(coin));
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
                    error!(?uuid, "Timeout when getting gas coin from the queue");
                    break;
                }
            }
        }
        Ok(None)
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
        recipient: SuiAddress,
        uuid: Uuid,
    ) -> Result<(TransactionDigest, Vec<ObjectID>, Vec<u64>), FaucetError> {
        let number_of_coins = amounts.len();
        let total_amount: u64 = amounts.iter().sum();

        let gas_coin_opt = self
            .prepare_gas_coin(total_amount, uuid)
            .await
            .tap_ok(|res| {
                debug!(recipient=?recipient, ?uuid, "Planning to use gas coin: {:?}", res);
            })
            .tap_err(|err| {
                error!(
                    ?uuid,
                    "Failed to prepare gas coin with err: {:?}",
                    err.to_string()
                )
            })
            .map_err(|err| {
                FaucetError::Internal(format!(
                    "Failed to prepare gas coin with err: {:?}",
                    err.to_string()
                ))
            })?;
        let interal_err = FaucetError::Internal("Failed to prepare coin, None returned.".into());
        let gas_coin = gas_coin_opt.ok_or(interal_err)?;

        let result = self
            .execute_pay_sui_txn_with_retrials(
                gas_coin,
                self.active_address,
                recipient,
                amounts,
                DEFAULT_GAS_BUDGET,
                uuid,
            )
            .await;

        // Once transactions are done, in despite of success or failure,
        // we put back the coins. The producer should never wait indefinitely,
        // in that the channel is initialized with big enough capacity.
        let producer = self.producer.lock().await;
        debug!(
            ?uuid,
            "Got producer lock and recycling gas coin {:?}.", gas_coin
        );
        producer
            .try_send(gas_coin)
            .expect("unexpected - queue is large enough to hold all coins");
        debug!(?uuid, "Recycled coin {:?}", gas_coin);
        drop(producer);

        match result {
            Ok(res) => {
                let txns = res.certificate.data.transactions;
                if txns.len() != 1 {
                    panic!(
                        "PaySui Transaction should create one and exactly one txn, but got {:?}",
                        txns
                    );
                }
                let created = res.effects.created;
                if created.len() != number_of_coins {
                    panic!(
                        "PaySui Transaction should create exact {:?} new coins, but got {:?}",
                        number_of_coins, created
                    );
                }
                let txn = &txns[0];
                if let SuiTransactionKind::PaySui(SuiPaySui {
                    // coins here are input coins, rather than the created coins under recipients.
                    coins: _,
                    recipients,
                    amounts,
                }) = txn
                {
                    assert!(recipients
                        .iter()
                        .all(|sent_recipient| sent_recipient == &recipient));
                    let coin_ids: Vec<ObjectID> = created
                        .iter()
                        .map(|created_coin_owner_ref| created_coin_owner_ref.reference.object_id)
                        .collect();
                    Ok((
                        res.certificate.transaction_digest,
                        coin_ids,
                        amounts.clone(),
                    ))
                } else {
                    panic!("Expect SuiTransactionKind::PaySui(SuiPaySui) to send coins to address {} but got txn {:?}", recipient, txn);
                }
            }
            Err(e) => Err(FaucetError::Internal(format!(
                "Encountered error in transfer gases with err: {:?}.",
                e
            ))),
        }
    }

    async fn execute_pay_sui_txn_with_retrials(
        &self,
        coin_id: ObjectID,
        signer: SuiAddress,
        recipient: SuiAddress,
        amounts: &[u64],
        budget: u64,
        uuid: Uuid,
    ) -> Result<SuiTransactionResponse, anyhow::Error> {
        let retry_intervals_ms = [Duration::from_millis(500), Duration::from_millis(1000)];
        let mut retry_iter = retry_intervals_ms.iter();
        let mut res = self
            .execute_pay_sui_txn(coin_id, signer, recipient, amounts, budget, uuid)
            .await;
        while res.is_err() {
            if let Some(interval) = retry_iter.next() {
                tokio::time::sleep(*interval).await;
                debug!(
                    ?recipient,
                    ?coin_id,
                    ?uuid,
                    "Retrying executing PaySui transaction in faucet, previous error: {:?}",
                    &res,
                );
                res = self
                    .execute_pay_sui_txn(coin_id, signer, recipient, amounts, budget, uuid)
                    .await;
            } else {
                warn!(
                    ?recipient,
                    ?coin_id,
                    ?uuid,
                    "Failed to execute PaySui transactions in faucet with {} retries and intervals {:?}",
                    retry_intervals_ms.len(),
                    &retry_intervals_ms
                );
                break;
            }
        }
        res
    }

    async fn execute_pay_sui_txn(
        &self,
        coin_id: ObjectID,
        signer: SuiAddress,
        recipient: SuiAddress,
        amounts: &[u64],
        budget: u64,
        uuid: Uuid,
    ) -> Result<SuiTransactionResponse, anyhow::Error> {
        let context = &self.wallet;
        let txn_data = self
            .build_pay_sui_txn(coin_id, signer, recipient, amounts, budget)
            .await?;
        let signature = context
            .config
            .keystore
            .sign(&signer, &txn_data.to_bytes())?;

        let tx = Transaction::new(txn_data, signature).verify()?;
        let tx_digest = *tx.digest();
        info!(
            ?tx_digest,
            ?recipient,
            ?coin_id,
            ?uuid,
            "PaySui transaction in faucet."
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

        self.wallet
            .client
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

        let (digest, coin_ids, sent_amounts) = self.transfer_gases(amounts, recipient, id).await?;
        if coin_ids.len() != amounts.len() {
            error!(
                uuid = ?id, ?recipient,
                "Requested {} coins but got {}",
                amounts.len(),
                coin_ids.len()
            );
        }
        let elapsed = timer.stop_and_record();

        info!(uuid = ?id, ?recipient, ?digest, "PaySui txn succeeded in {} secs", elapsed);
        self.metrics.total_requests_succeeded.inc();
        Ok(FaucetReceipt {
            sent: coin_ids
                .iter()
                .zip(sent_amounts)
                .map(|(coin_id, sent_amount)| CoinInfo {
                    transfer_tx_digest: digest,
                    amount: sent_amount,
                    id: *coin_id,
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
        let res = SuiClientCommands::PayAllSui {
            input_coins: vec![*bad_gas.id()],
            recipient: SuiAddress::random_for_testing_only(),
            gas_budget: 50000,
        }
        .execute(faucet.wallet_mut())
        .await
        .unwrap();

        if let SuiClientCommandResult::PayAllSui(_tx_cert, effects) = res {
            assert!(matches!(effects.status, SuiExecutionStatus::Success));
        } else {
            panic!("PayAllSui command did not return SuiClientCommandResult::PayAllSui");
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
            amounts: Some(vec![tiny_value + PAY_SUI_GAS]),
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
            panic!("split command did not return SuiClientCommandResult::SplitCoin");
        };

        // Get the latest list of gas
        let gases = get_current_gases(address, &mut context).await;
        let tiny_amount = gases
            .iter()
            .find(|gas| gas.id() == &tiny_coin_id)
            .unwrap()
            .value();
        assert_eq!(tiny_amount, tiny_value + PAY_SUI_GAS);

        let gases: HashSet<ObjectID> = HashSet::from_iter(gases.into_iter().map(|gas| *gas.id()));

        let prom_registry = Registry::new();
        let mut faucet = SimpleFaucet::new(context, &prom_registry).await.unwrap();

        // Ask for a value higher than tiny coin + PAY_SUI_GAS
        let number_of_coins = gases.len();
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
        info!(
            ?number_of_coins,
            "Sent to random addresses: {} {}",
            amounts[0],
            amounts.len(),
        );

        // Verify that the tiny gas is still in the queue.
        tokio::task::yield_now().await;
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
