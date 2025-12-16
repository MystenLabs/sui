// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use std::path::Path;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_move_build::BuildConfig;
use sui_sdk::rpc_types::SuiTransactionBlockResponseOptions;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{Signature, SuiKeyPair};
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::transaction::{Transaction, TransactionData};
use shared_crypto::intent::{Intent, IntentMessage};
use tracing::info;

pub struct Actor {
    keypair: SuiKeyPair,
    address: SuiAddress,
    client: SuiClient,
}

impl Actor {
    pub fn new(keypair: SuiKeyPair, client: SuiClient) -> Self {
        let address = (&keypair.public()).into();
        Self {
            keypair,
            address,
            client,
        }
    }

    pub fn address(&self) -> SuiAddress {
        self.address
    }

    pub async fn publish_package(&self, package_path: &str) -> Result<ObjectID> {
        info!("Publishing package from path: {}", package_path);
        
        // Build the Move package
        let build_config = BuildConfig::new_for_testing();
        let package = build_config.build(Path::new(package_path))?;
        let compiled_modules = package.get_package_bytes(false);
        let dependencies = package.get_dependency_storage_package_ids();
        
        // Get gas object
        let gas_objects = self.client.coin_read_api()
            .get_coins(self.address, None, None, None)
            .await?
            .data;
        
        let gas_object = gas_objects.first()
            .ok_or_else(|| anyhow!("No gas objects found for address"))?;
        
        // Create publish transaction
        let tx_data = TransactionData::new_module(
            self.address,
            gas_object.object_ref(),
            compiled_modules,
            dependencies,
            100_000_000, // gas budget
            1000, // gas price
        );
        
        // Sign transaction
        let intent_msg = IntentMessage::new(
            Intent::sui_transaction(),
            tx_data.clone(),
        );
        let signature = Signature::new_secure(
            &intent_msg,
            &self.keypair,
        );
        
        let transaction = Transaction::from_data(tx_data, vec![signature]);
        
        // Submit transaction
        let response = self.client
            .quorum_driver_api()
            .execute_transaction_block(
                transaction,
                SuiTransactionBlockResponseOptions::full_content(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;
        
        // Extract package ID from effects
        let package_id = response
            .effects
            .as_ref()
            .and_then(|effects| effects.created()
                .iter()
                .find(|obj| obj.owner.is_immutable())
                .map(|obj| obj.reference.object_id))
            .ok_or_else(|| anyhow!("Failed to extract package ID from transaction effects"))?;
        
        info!("Successfully published package with ID: {}", package_id);
        Ok(package_id)
    }

    pub async fn spawn_tx_clients(&self, count: usize) -> Result<()> {
        todo!("Spawn {} transaction clients", count);
    }

    pub async fn spawn_rpc_clients(&self, count: usize) -> Result<()> {
        todo!("Spawn {} RPC clients", count);
    }

    pub async fn get_balance(&self) -> Result<u64> {
        todo!("Get balance for address {}", self.address);
    }
}