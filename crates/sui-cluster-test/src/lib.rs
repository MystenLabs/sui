// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::faucet::{FaucetClient, FaucetClientFactory};
use async_trait::async_trait;
use clap::*;
use cluster::{Cluster, ClusterFactory};
use config::ClusterTestOpt;
use std::sync::Arc;
use sui::client_commands::WalletContext;
use sui_json_rpc_types::SuiTransactionResponse;
use test_utils::messages::make_transactions_with_wallet_context;

use sui_sdk::SuiClient;
use sui_types::gas_coin::GasCoin;
use sui_types::{
    base_types::SuiAddress,
    messages::{Transaction, TransactionData},
};
use test_case::{
    call_contract_test::CallContractTest, coin_merge_split_test::CoinMergeSplitTest,
    fullnode_execute_transaction_test::FullNodeExecuteTransactionTest,
    native_transfer_test::NativeTransferTest, shared_object_test::SharedCounterTest,
};
use tokio::time::{sleep, Duration};
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
    /// Client that provides wallet context and gateway access
    /// Once we sunset gateway, we will spin off fullnode client,
    client: WalletClient,
    /// Facuet client that provides faucet access to a test
    faucet: Arc<dyn FaucetClient + Sync + Send>,
}

impl TestContext {
    async fn get_sui_from_faucet(&self, minimum_coins: Option<usize>) -> Vec<GasCoin> {
        self.faucet
            .request_sui_coins(self.get_context(), minimum_coins, None)
            .await
            .unwrap_or_else(|e| panic!("Failed to get test SUI coins from faucet, {e}"))
    }

    fn get_context(&self) -> &WalletClient {
        &self.client
    }

    fn get_gateway(&self) -> &SuiClient {
        self.client.get_gateway()
    }

    fn get_fullnode(&self) -> &SuiClient {
        self.client.get_fullnode()
    }

    fn get_wallet(&self) -> &WalletContext {
        self.client.get_wallet()
    }

    fn get_wallet_mut(&mut self) -> &mut WalletContext {
        self.client.get_wallet_mut()
    }

    fn get_wallet_address(&self) -> SuiAddress {
        self.client.get_wallet_address()
    }

    /// See `make_transactions_with_wallet_context` for potential caveats
    /// of this helper function.
    pub async fn make_transactions(&mut self, max_txn_num: usize) -> Vec<Transaction> {
        make_transactions_with_wallet_context(self.get_wallet_mut(), max_txn_num).await
    }

    async fn sign_and_execute(
        &self,
        txn_data: TransactionData,
        desc: &str,
    ) -> SuiTransactionResponse {
        let signer = self
            .get_wallet()
            .keystore
            .signer(self.get_context().get_wallet_address());
        let tx = Transaction::from_data(txn_data, &signer);
        self.get_gateway()
            .execute_transaction(tx)
            .await
            .unwrap_or_else(|e| panic!("Failed to execute transaction for {}. {}", desc, e))
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

    // TODO: figure out a more efficient way to test a local cluster
    // A potential way to do this is to subscribe to txns from fullnode
    // when the feature is ready
    pub async fn let_fullnode_sync(&self) {
        let duration = Duration::from_secs(5);
        sleep(duration).await;
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
            TestCase::new(CallContractTest {}),
            TestCase::new(SharedCounterTest {}),
            TestCase::new(FullNodeExecuteTransactionTest {}),
        ];

        // TODO: improve the runner parallelism for efficiency
        // For now we run tests serially
        let mut success_cnt = 0;
        let total_cnt = tests.len() as i32;
        for t in tests {
            let is_sucess = t.run(&mut ctx).await as i32;
            success_cnt += is_sucess;
        }
        if success_cnt < total_cnt {
            // If any test failed, panic to bubble up the signal
            panic!("{success_cnt} of {total_cnt} tests passed.");
        }
        info!("{success_cnt} of {total_cnt} tests passed.");
    }
}
