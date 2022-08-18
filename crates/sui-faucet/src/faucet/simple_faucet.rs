// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;

use crate::metrics::FaucetMetrics;
use prometheus::Registry;

// HashSet is in fact used but linter does not think so
#[allow(unused_imports)]
use std::collections::HashSet;

use sui::client_commands::{SuiClientCommands, WalletContext};
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionKind, SuiTransactionResponse, SuiTransferSui,
};
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    gas_coin::GasCoin,
    messages::{Transaction, TransactionData},
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
        SuiClientCommands::SyncClientState {
            address: Some(active_address),
        }
        .execute(&mut wallet)
        .await
        .map_err(|err| FaucetError::Wallet(format!("Fail to sync client state: {}", err)))?;

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
            if let Err(e) = producer.send(*coin.id()).await {
                panic!("Failed to set up gas pools: {:?}", e);
            }
        }

        debug!("Using coins: {:?}", coins);

        let metrics = FaucetMetrics::new(prometheus_registry);

        Ok(Self {
            wallet,
            active_address,
            producer: Mutex::new(producer),
            consumer: Mutex::new(consumer),
            metrics,
        })
    }

    async fn select_coins(&self, number_of_coins: usize) -> Vec<ObjectID> {
        assert!(number_of_coins > 0);
        // If the gas candidate queue is exhausted, the request will be
        // suspended indefinitely until a producer puts in more candidate
        // gas objects. At the same time, other requests will be blocked by the
        // lock acquisition as well.
        let mut consumer = self.consumer.lock().await;
        let mut coins = Vec::with_capacity(number_of_coins);
        while let Some(coin) = consumer.recv().await {
            // TODO: for now we assume each SUI object is enough to cover the split
            // but this may not be true, if we run the faucet for really really long time or
            // due to some other unexpected issues.
            coins.push(coin);
            if coins.len() == number_of_coins {
                break;
            }
        }
        coins
    }

    async fn transfer_gases(
        &self,
        amounts: &[u64],
        to: SuiAddress,
        uuid: Uuid,
    ) -> Result<Vec<(TransactionDigest, ObjectID, u64, ObjectID)>, FaucetError> {
        let number_of_coins = amounts.len();
        let coins = self.select_coins(number_of_coins).await;
        debug!(recipient=?to, ?uuid, "Planning to use coins: {:?}", coins);

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
        for coin in coins {
            if let Err(e) = producer.send(coin).await {
                panic!("Failed to put coin {:?} back to queue: {:?}", coin, e);
            }
        }
        drop(producer);

        let responses: Vec<_> = results
            .into_iter()
            .filter(|res| {
                if res.is_ok() {
                    true
                } else {
                    error!("Encountered error in transfer sui: {:?}", res);
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
            .gateway
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

        let tx = Transaction::from_data(data, &context.keystore.signer(signer));
        info!(tx_digest = ?tx.digest(), ?recipient, ?coin_id, ?uuid, "Broadcasting transfer obj txn");
        let response = context.gateway.execute_transaction(tx).await?;
        let effects = &response.effects;
        if matches!(effects.status, SuiExecutionStatus::Failure { .. }) {
            return Err(anyhow!("Error transferring object: {:#?}", effects.status));
        }

        Ok(response)
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
            return Err(FaucetError::Transfer(format!(
                "Requested {} coins but only got {}",
                amounts.len(),
                results.len()
            )));
        }

        let elapsed = timer.stop_and_record();

        info!(uuid = ?id, ?recipient, ?results, "Transfer txn succeeded in {} secs", elapsed);
        self.metrics.total_requests_succeeded.inc();

        Ok(FaucetReceipt {
            sent: results
                .iter()
                .map(|(_digest, obj_id, amount, _gas_id)| CoinInfo {
                    amount: *amount,
                    id: *obj_id,
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use sui::client_commands::{SuiClientCommandResult, SuiClientCommands};
    use test_utils::network::setup_network_and_wallet;

    use super::*;

    #[tokio::test]
    async fn simple_faucet_basic_interface_should_work() {
        telemetry_subscribers::init_for_testing();
        let (_network, context, _address) = setup_network_and_wallet().await.unwrap();
        let prom_registry = prometheus::Registry::new();
        let faucet = SimpleFaucet::new(context, &prom_registry).await.unwrap();
        test_basic_interface(faucet).await;
    }

    #[tokio::test]
    async fn test_init_gas_queue() {
        let (_network, mut context, address) = setup_network_and_wallet().await.unwrap();
        let results = SuiClientCommands::Gas {
            address: Some(address),
        }
        .execute(&mut context)
        .await
        .unwrap();
        let gases = match results {
            SuiClientCommandResult::Gas(gases) => gases,
            other => panic!("Expect SuiClientCommandResult::Gas, but got {:?}", other),
        };
        let gases = HashSet::from_iter(gases.into_iter().map(|gas| *gas.id()));
        let prom_registry = prometheus::Registry::new();
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
        let (_network, mut context, address) = setup_network_and_wallet().await.unwrap();
        let results = SuiClientCommands::Gas {
            address: Some(address),
        }
        .execute(&mut context)
        .await
        .unwrap();
        let gases = match results {
            SuiClientCommandResult::Gas(gases) => gases,
            other => panic!("Expect SuiClientCommandResult::Gas, but got {:?}", other),
        };
        let gases = HashSet::from_iter(gases.into_iter().map(|gas| *gas.id()));

        let prom_registry = prometheus::Registry::new();
        let mut faucet = SimpleFaucet::new(context, &prom_registry).await.unwrap();

        let number_of_coins = gases.len();
        let amounts = &vec![1; number_of_coins];
        let _ = futures::future::join_all([0..30].iter().map(|_| {
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

        // After all transfer reuqests settle, we still have the original candidates gas in queue.
        let candidates = faucet.drain_gas_queue(gases.len()).await;
        assert_eq!(
            candidates, gases,
            "gases: {:?}, candidates: {:?}",
            gases, candidates
        );
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
}
