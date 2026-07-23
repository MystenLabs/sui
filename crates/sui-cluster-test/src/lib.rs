// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::faucet::{FaucetClient, FaucetClientFactory};
use async_trait::async_trait;
use cluster::{Cluster, ClusterFactory};
use config::ClusterTestOpt;
use futures::future::join_all;
use helper::ObjectChecker;
use std::sync::Arc;
use sui_faucet::{CoinInfo, RequestStatus};
use sui_rpc_api::Client as GrpcClient;
use sui_rpc_api::client::ExecutedTransaction;
use sui_sdk::wallet_context::WalletContext;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectRef, TransactionDigest};
use sui_types::object::Owner;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;

use sui_types::gas_coin::GasCoin;
use sui_types::{
    base_types::SuiAddress,
    transaction::{Transaction, TransactionData},
};
use test_case::{
    coin_index_test::CoinIndexTest, coin_merge_split_test::CoinMergeSplitTest,
    fullnode_execute_transaction_test::FullNodeExecuteTransactionTest,
    grpc_publish_transaction_test::GrpcPublishTransactionTest,
    native_transfer_test::NativeTransferTest, random_beacon_test::RandomBeaconTest,
    shared_object_test::SharedCounterTest, staking_test::StakingTest,
};
use tracing::{error, info};
use wallet_client::WalletClient;

pub mod cluster;
pub mod config;
pub mod faucet;
pub mod helper;
pub mod test_case;
pub mod wallet_client;

#[allow(unused)]
pub struct TestContext {
    /// Cluster handle that allows access to various components in a cluster
    cluster: Box<dyn Cluster + Sync + Send>,
    /// Client that provides wallet context and gRPC fullnode access
    client: WalletClient,
    /// Facuet client that provides faucet access to a test
    faucet: Arc<dyn FaucetClient + Sync + Send>,
}

impl TestContext {
    /// Request coins from the faucet, wait for every faucet transfer to be
    /// visible over gRPC (`LedgerService`), then read each funded coin object by
    /// ID and verify ownership. Gas/coin object IDs come straight from the faucet
    /// response (which returns the exact IDs), so we never depend on owner
    /// enumeration to fund a transaction — good practice for determinism, not a
    /// workaround for any missing service.
    async fn get_sui_from_faucet(&self, minimum_coins: Option<usize>) -> Vec<GasCoin> {
        let addr = self.get_wallet_address();
        let minimum_coins = minimum_coins.unwrap_or(1);

        // Coins-per-request varies by faucet (the local test faucet sends
        // several; gas-station-backed remote faucets send exactly one), so
        // accumulate requests until the minimum is met.
        let mut coin_info = Vec::new();
        for _ in 0..minimum_coins {
            let faucet_response = self.faucet.request_sui_coins(addr).await;
            if let RequestStatus::Failure(e) = faucet_response.status {
                panic!("Failed to get coins from faucet: {e}");
            }
            coin_info.extend(faucet_response.coins_sent.unwrap_or_default());
            if coin_info.len() >= minimum_coins {
                break;
            }
        }

        let digests = coin_info
            .iter()
            .map(|coin_info| coin_info.transfer_tx_digest)
            .collect::<Vec<_>>();

        // Wait (concurrently) for the independent faucet transactions to be
        // checkpointed before we read the funded objects.
        self.wait_for_txns(&digests).await;

        let gas_coins = self.check_owner_and_into_gas_coin(coin_info, addr).await;

        if gas_coins.len() < minimum_coins {
            panic!(
                "Expect to get at least {minimum_coins} Sui Coins for address {addr}, but only got {}",
                gas_coins.len()
            )
        }

        gas_coins
    }

    fn get_context(&self) -> &WalletClient {
        &self.client
    }

    /// The shared gRPC client (owned, cheaply cloned). Backed by the wallet's
    /// cached connection.
    fn get_grpc_client(&self) -> GrpcClient {
        self.client.grpc_client()
    }

    fn get_wallet(&self) -> &WalletContext {
        self.client.get_wallet()
    }

    async fn get_latest_sui_system_state(&self) -> SuiSystemStateSummary {
        self.get_grpc_client()
            .get_system_state_summary(None)
            .await
            .unwrap()
    }

    async fn get_reference_gas_price(&self) -> u64 {
        self.get_grpc_client()
            .get_reference_gas_price()
            .await
            .unwrap()
    }

    fn get_wallet_address(&self) -> SuiAddress {
        self.client.get_wallet_address()
    }

    /// Fetch the current `ObjectRef` (id, version, digest) for a known object ID
    /// over `LedgerService`. Object references must be refreshed before every
    /// transaction because version + digest change on each mutation.
    pub async fn current_object_ref(
        &self,
        object_id: sui_types::base_types::ObjectID,
    ) -> ObjectRef {
        self.get_grpc_client()
            .get_object(object_id)
            .await
            .unwrap_or_else(|e| panic!("Failed to fetch object {object_id}: {e}"))
            .compute_object_reference()
    }

    /// Build up to `max_txn_num` simple transfer-SUI transactions, each paying a
    /// tiny amount to a fresh recipient. Gas is sourced explicitly from the
    /// faucet-funded coins (one coin per transaction), keeping construction
    /// deterministic without relying on gas enumeration.
    pub async fn make_transactions(&self, max_txn_num: usize) -> Vec<Transaction> {
        let sender = self.get_wallet_address();
        let gas_price = self.get_reference_gas_price().await;
        // Fund enough coins so each transaction has its own gas coin.
        let coins = self.get_sui_from_faucet(Some(max_txn_num)).await;

        let mut txns = Vec::with_capacity(max_txn_num);
        for coin in coins.into_iter().take(max_txn_num) {
            let recipient = SuiAddress::random_for_testing_only();
            let gas_ref = self.current_object_ref(*coin.id()).await;
            let data = TestTransactionBuilder::new(sender, gas_ref, gas_price)
                .transfer_sui(Some(1), recipient)
                .build();
            let signature = self.get_context().sign(&data, "make_transactions").await;
            txns.push(Transaction::from_data(data, vec![signature]));
        }
        txns
    }

    /// Sign and execute a transaction over gRPC (`TransactionExecutionService`),
    /// waiting for it to be checkpointed, and assert success. Returns the native
    /// `ExecutedTransaction` (effects, events, balance changes, changed objects).
    /// Does not refetch the transaction afterwards.
    async fn sign_and_execute(&self, txn_data: TransactionData, desc: &str) -> ExecutedTransaction {
        let signature = self.get_context().sign(&txn_data, desc).await;
        let tx = Transaction::from_data(txn_data, vec![signature]);
        self.get_wallet().execute_transaction_must_succeed(tx).await
    }

    pub async fn setup(options: ClusterTestOpt) -> Result<Self, anyhow::Error> {
        let cluster = ClusterFactory::start(&options).await?;
        let wallet_client = WalletClient::new_from_cluster(&cluster).await;
        let faucet = FaucetClientFactory::new_from_cluster(&cluster).await;
        Ok(Self {
            cluster,
            client: wallet_client,
            faucet,
        })
    }

    /// Wait (concurrently) for each transaction digest to be indexed into a
    /// checkpoint on the fullnode using the standard gRPC transaction-wait
    /// mechanism (`Client::wait_for_transaction`, 30s timeout each).
    pub async fn wait_for_txns(&self, digests: &[TransactionDigest]) {
        let client = self.get_grpc_client();
        let waits = digests.iter().map(|digest| {
            let client = client.clone();
            async move {
                client
                    .wait_for_transaction(digest)
                    .await
                    .unwrap_or_else(|e| panic!("Fullnode did not observe {digest}: {e}"));
            }
        });
        join_all(waits).await;
    }

    async fn check_owner_and_into_gas_coin(
        &self,
        coin_info: Vec<CoinInfo>,
        owner: SuiAddress,
    ) -> Vec<GasCoin> {
        let client = self.get_grpc_client();
        join_all(coin_info.iter().map(|coin_info| {
            let client = client.clone();
            async move {
                ObjectChecker::new(coin_info.id)
                    .owner(Owner::AddressOwner(owner))
                    .check_into_gas_coin(&client)
                    .await
            }
        }))
        .await
    }
}

pub struct TestCase<'a> {
    test_case: Box<dyn TestCaseImpl + 'a>,
}

impl<'a> TestCase<'a> {
    pub fn new(test_case: impl TestCaseImpl + 'a) -> Self {
        TestCase {
            test_case: (Box::new(test_case)),
        }
    }

    pub async fn run(self, ctx: &mut TestContext) -> bool {
        let test_name = self.test_case.name();
        info!("Running test {}.", test_name);

        // TODO: unwind panic and fail gracefully?

        match self.test_case.run(ctx).await {
            Ok(()) => {
                info!("Test {test_name} succeeded.");
                true
            }
            Err(e) => {
                error!("Test {test_name} failed with error: {e}.");
                false
            }
        }
    }
}

#[async_trait]
pub trait TestCaseImpl {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error>;
}

pub struct ClusterTest;

impl ClusterTest {
    pub async fn run(options: ClusterTestOpt) {
        let mut ctx = TestContext::setup(options)
            .await
            .unwrap_or_else(|e| panic!("Failed to set up TestContext, e: {e}"));

        // TODO: collect tests from each test_case file instead.
        let tests = vec![
            TestCase::new(NativeTransferTest {}),
            TestCase::new(CoinMergeSplitTest {}),
            TestCase::new(SharedCounterTest {}),
            TestCase::new(FullNodeExecuteTransactionTest {}),
            TestCase::new(GrpcPublishTransactionTest {}),
            TestCase::new(CoinIndexTest {}),
            TestCase::new(RandomBeaconTest {}),
            TestCase::new(StakingTest {}),
        ];

        // TODO: improve the runner parallelism for efficiency
        // For now we run tests serially
        let mut success_cnt = 0;
        let total_cnt = tests.len() as i32;
        for t in tests {
            let is_success = t.run(&mut ctx).await as i32;
            success_cnt += is_success;
        }
        if success_cnt < total_cnt {
            // If any test failed, panic to bubble up the signal
            panic!("{success_cnt} of {total_cnt} tests passed.");
        }
        info!("{success_cnt} of {total_cnt} tests passed.");
    }
}
