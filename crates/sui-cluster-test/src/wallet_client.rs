// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::cluster::new_wallet_context_from_cluster;

use super::Cluster;
use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_rpc_api::Client as GrpcClient;
use sui_sdk::wallet_context::WalletContext;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{KeypairTraits, Signature};
use sui_types::transaction::TransactionData;
use tracing::{Instrument, info_span};

/// Wraps a `WalletContext` for the test user. The wallet context already caches
/// a gRPC client (`WalletContext::grpc_client`) that talks to the fullnode over
/// the public gRPC services (`LedgerService`, `StateService`,
/// `TransactionExecutionService`), so we do not build a separate JSON-RPC client.
pub struct WalletClient {
    wallet_context: WalletContext,
    address: SuiAddress,
}

#[allow(clippy::borrowed_box)]
impl WalletClient {
    pub async fn new_from_cluster(cluster: &(dyn Cluster + Sync + Send)) -> Self {
        let key = cluster.user_key();
        let address: SuiAddress = key.public().into();
        let wallet_context = new_wallet_context_from_cluster(cluster, key)
            .await
            .instrument(info_span!("init_wallet_context_for_test_user"));

        Self {
            wallet_context: wallet_context.into_inner(),
            address,
        }
    }

    pub fn get_wallet(&self) -> &WalletContext {
        &self.wallet_context
    }

    pub fn get_wallet_mut(&mut self) -> &mut WalletContext {
        &mut self.wallet_context
    }

    pub fn get_wallet_address(&self) -> SuiAddress {
        self.address
    }

    /// Returns a fresh (owned, cheaply cloned) gRPC client backed by the wallet's
    /// cached connection. All network reads and execution in the suite go through
    /// this client rather than the retired full JSON-RPC contract.
    pub fn grpc_client(&self) -> GrpcClient {
        self.wallet_context
            .grpc_client()
            .expect("wallet context should expose a gRPC client")
    }

    pub async fn sign(&self, txn_data: &TransactionData, desc: &str) -> Signature {
        self.get_wallet()
            .config
            .keystore
            .sign_secure(&self.address, txn_data, Intent::sui_transaction())
            .await
            .unwrap_or_else(|e| panic!("Failed to sign transaction for {}. {}", desc, e))
    }
}
