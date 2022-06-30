// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use serde_json::json;
use std::collections::HashMap;
use sui::client_commands::{
    call_move, WalletContext, EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_NAME, EXAMPLE_NFT_URL,
};
use sui::config::{Config, GatewayType, SuiClientConfig};
use sui_config::SUI_KEYSTORE_FILENAME;
use sui_faucet::FaucetResponse;
use sui_json::SuiJsonValue;
use sui_json_rpc_api::keystore::KeystoreType;
use sui_json_rpc_api::rpc_types::{GetObjectDataResponse, SuiExecutionStatus, TransactionResponse};
use sui_types::{
    base_types::{encode_bytes_hex, ObjectID, SuiAddress},
    crypto::KeyPair,
    gas_coin::GasCoin,
    messages::{Transaction, TransactionData},
    object::Owner,
    SUI_FRAMEWORK_ADDRESS,
};
use tracing::{debug, info};

#[derive(Parser, Clone, ArgEnum)]
enum Env {
    Prod,
    Staging,
    Custom,
}

#[derive(Parser)]
#[clap(name = "", rename_all = "kebab-case")]
struct ClusterTestOpt {
    #[clap(arg_enum)]
    env: Env,
    #[clap(long)]
    gateway_address: Option<String>,
    #[clap(long)]
    faucet_address: Option<String>,
}

struct TestContext {
    wallet_context: WalletContext,
    address: SuiAddress,
    faucet_address: String,
}

const PROD_GATEWAY_ADDR: &str = "https://gateway.devnet.sui.io:443";
const PROD_FAUCET_ADDR: &str = "https://faucet.devnet.sui.io:443";
const STAGING_GATEWAY_ADDR: &str = "https://gateway.staging.sui.io:443";
const STAGING_FAUCET_ADDR: &str = "https://faucet.staging.sui.io:443";

struct ClusterTest {
    context: TestContext,
}

impl ClusterTest {
    async fn test_transfer(&self, coins: &mut Vec<GasCoin>, gas_obj_id: ObjectID) {
        info!("Testing gas coin transfer");
        assert!(!coins.is_empty(), "Not enough gas objects to run test.");
        let signer = self.context.address;
        let wallet_context = self.wallet_context();
        let (receipent_addr, _) = KeyPair::get_key_pair();
        let obj_to_transfer = coins.remove(0);
        let data = wallet_context
            .gateway
            .public_transfer_object(
                signer,
                *obj_to_transfer.id(),
                Some(gas_obj_id),
                5000,
                receipent_addr,
            )
            .await
            .expect("Failed to get transaction data for transfer.");

        let response = self
            .sign_and_execute(data, "coin transfer")
            .await
            .to_effect_response()
            .unwrap();
        let effects = response.effects;
        if !matches!(effects.status, SuiExecutionStatus::Success { .. }) {
            panic!(
                "Failed to execute transfer tranasction: {:?}",
                effects.status
            );
        }
    }

    async fn test_merge_and_split(&self, coins: &mut Vec<GasCoin>, gas_obj_id: ObjectID) {
        assert!(!coins.is_empty(), "Not enough gas objects to run test.");
        let signer = self.context.address;
        let wallet_context = self.wallet_context();
        let primary_coin = coins.remove(0);
        let primary_coin_id = *primary_coin.id();
        let original_value = primary_coin.value();

        // Split
        info!("Testing coin split.");
        let amounts = vec![1, (original_value - 2) / 2];

        let data = wallet_context
            .gateway
            .split_coin(signer, *primary_coin.id(), amounts, Some(gas_obj_id), 5000)
            .await
            .expect("Failed to get transaction data for coin split");

        let split_response = self
            .sign_and_execute(data, "coin split")
            .await
            .to_split_coin_response()
            .unwrap();

        // Verify new coins
        let _ = futures::future::join_all(
            split_response
                .new_coins
                .iter()
                .map(|coin_info| {
                    self.verify_object(
                        coin_info.reference.object_id,
                        Owner::AddressOwner(self.context.address),
                        false,
                        true,
                    )
                })
                .collect::<Vec<_>>(),
        )
        .await;

        // Merge
        info!("Testing coin merge.");
        // We on purpose linearize the merge operations, otherwise the primary coin may be locked
        for new_coin in &split_response.new_coins {
            let coin_to_merge = new_coin.reference.object_id;
            debug!(
                "Merging coin {} back to {}.",
                coin_to_merge, primary_coin_id
            );
            self.merge_coin(signer, primary_coin_id, coin_to_merge, gas_obj_id)
                .await;
            debug!("Verifying the merged coin {} is deleted.", coin_to_merge);
            self.verify_object(
                coin_to_merge,
                Owner::AddressOwner(self.context.address),
                true,
                true,
            )
            .await;
        }

        // Owner still owns the primary coin
        debug!(
            "Verifying owner still owns the primary coin {}",
            *primary_coin.id()
        );
        let primary_after_merge = self
            .verify_object(
                primary_coin_id,
                Owner::AddressOwner(self.context.address),
                false,
                true,
            )
            .await
            .unwrap();
        assert_eq!(
            primary_after_merge.value(),
            original_value,
            "Split-then-merge yields unexpected coin value, expect {}, got {}",
            original_value,
            primary_after_merge.value(),
        );
    }

    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas_obj_id: ObjectID,
    ) {
        let wallet_context = self.wallet_context();
        let data = wallet_context
            .gateway
            .merge_coins(signer, primary_coin, coin_to_merge, Some(gas_obj_id), 5000)
            .await
            .expect("Failed to get transaction data for coin merge");
        self.sign_and_execute(data, "coin merge").await;
    }

    async fn sign_and_execute(&self, txn_data: TransactionData, desc: &str) -> TransactionResponse {
        let signature = self
            .wallet_context()
            .keystore
            .sign(&self.context.address, &txn_data.to_bytes())
            .unwrap_or_else(|e| panic!("Failed to sign transaction for {}. {}", desc, e));
        self.wallet_context()
            .gateway
            .execute_transaction(Transaction::new(txn_data, signature))
            .await
            .unwrap_or_else(|e| panic!("Failed to execute transaction for {}. {}", desc, e))
    }

    async fn test_get_gas(&self) -> Vec<GasCoin> {
        let client = reqwest::Client::new();
        let gas_url = format!("{}/gas", self.context.faucet_address);

        info!("Testing coin request from faucet {}", gas_url);
        let data = HashMap::from([("recipient", encode_bytes_hex(&self.context.address))]);
        let map = HashMap::from([("FixedAmountRequest", data)]);

        let response = client
            .post(&gas_url)
            .json(&map)
            .send()
            .await
            .unwrap()
            .json::<FaucetResponse>()
            .await
            .unwrap();

        if let Some(error) = response.error {
            panic!("Failed to get gas tokens with error: {}", error)
        }
        let gas_coins = futures::future::join_all(
            response
                .transferred_gas_objects
                .iter()
                .map(|coin_info| {
                    self.verify_object(
                        coin_info.id,
                        Owner::AddressOwner(self.context.address),
                        false,
                        true,
                    )
                })
                .collect::<Vec<_>>(),
        )
        .await;

        gas_coins
            .into_iter()
            .map(|o| o.expect("Expect object to be active but deleted."))
            .collect()
    }

    async fn test_call_contract(&mut self, gas_obj_id: ObjectID) {
        info!("Testing call move contract.");
        let wallet_context = &mut self.context.wallet_context;

        let args_json = json!([EXAMPLE_NFT_NAME, EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_URL,]);
        let mut args = vec![];
        for a in args_json.as_array().unwrap() {
            args.push(SuiJsonValue::new(a.clone()).unwrap());
        }
        let (_, effects) = call_move(
            ObjectID::from(SUI_FRAMEWORK_ADDRESS),
            "devnet_nft",
            "mint",
            vec![],
            Some(gas_obj_id),
            5000,
            args,
            wallet_context,
        )
        .await
        .expect("Failed to call move contract");

        let nft_id = effects
            .created
            .first()
            .expect("Failed to create NFT")
            .reference
            .object_id;
        let object_read = wallet_context
            .gateway
            .get_object(nft_id)
            .await
            .expect("Failed to get created NFT object");
        if let GetObjectDataResponse::Exists(_sui_object) = object_read {
            // all good
        } else {
            panic!("NFT object do not exist or was deleted");
        }
    }

    /// Verify Gas Coin exists with expected value and owner
    async fn verify_object(
        &self,
        obj_id: ObjectID,
        expected_owner: Owner,
        is_deleted: bool,
        is_sui_coin: bool,
    ) -> Option<GasCoin> {
        debug!(
            "Verifying object: {} (is {}a sui coin), owned by {}. Expect to be {}.",
            obj_id,
            if is_sui_coin { "" } else { "not " },
            expected_owner,
            if is_deleted { "deleted" } else { "alive" },
        );
        let object_id = obj_id;
        let object_info = self
            .wallet_context()
            .gateway
            .get_object(object_id)
            .await
            .unwrap_or_else(|err| {
                panic!(
                    "Failed to get object info (id: {}) from gateway, err: {err}",
                    obj_id
                )
            });
        match object_info {
            GetObjectDataResponse::NotExists(_) => {
                panic!("Gateway can't find gas object {}", object_id)
            }
            GetObjectDataResponse::Deleted(_) => {
                if !is_deleted {
                    panic!("Gas object {} was deleted", object_id);
                }
                None
            }
            GetObjectDataResponse::Exists(object) => {
                if is_deleted {
                    panic!("Expect Gas object {} deleted, but it is not", object_id);
                }
                assert_eq!(
                    object.owner, expected_owner,
                    "Gas coin {} does not belong to {}, but {}",
                    object_id, expected_owner, object.owner
                );
                if is_sui_coin {
                    let move_obj = object
                        .data
                        .try_as_move()
                        .unwrap_or_else(|| panic!("Object {} is not a move object", object_id));

                    let gas_coin = GasCoin::try_from(&move_obj.fields).unwrap_or_else(|err| {
                        panic!("Object {} is not a gas coin, {}", object_id, err)
                    });
                    return Some(gas_coin);
                }
                None
            }
        }
    }

    pub fn setup(options: ClusterTestOpt) -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        let wallet_config_path = temp_dir.path().join("wallet.yaml");

        let (gateway_addr, faucet_addr) = match options.env {
            Env::Prod => (PROD_GATEWAY_ADDR.into(), PROD_FAUCET_ADDR.into()),
            Env::Staging => (STAGING_GATEWAY_ADDR.into(), STAGING_FAUCET_ADDR.into()),
            Env::Custom => (
                options
                    .gateway_address
                    .expect("Expect 'gateway_address' for Env::Custom"),
                options
                    .faucet_address
                    .expect("Expect 'faucet_address' for Env::Custom"),
            ),
        };

        info!("Use gateway: {}", &gateway_addr);
        info!("Use facet: {}", &faucet_addr);
        let keystore_path = temp_dir.path().join(SUI_KEYSTORE_FILENAME);
        let keystore = KeystoreType::File(keystore_path);
        let new_address = keystore.init().unwrap().add_random_key().unwrap();
        SuiClientConfig {
            accounts: vec![new_address],
            keystore,
            gateway: GatewayType::RPC(gateway_addr),
            active_address: Some(new_address),
        }
        .persisted(&wallet_config_path)
        .save()
        .unwrap();

        info!(
            "Initialize wallet from config path: {:?}",
            wallet_config_path
        );

        let wallet_context = WalletContext::new(&wallet_config_path).unwrap();

        ClusterTest {
            context: TestContext {
                wallet_context,
                address: new_address,
                faucet_address: faucet_addr,
            },
        }
    }

    fn wallet_context(&self) -> &WalletContext {
        &self.context.wallet_context
    }
}

#[tokio::main]
async fn main() {
    let _guard = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();

    let options = ClusterTestOpt::parse();
    let mut test = ClusterTest::setup(options);

    // Run tests
    let mut coins = test.test_get_gas().await;
    let gas_obj_id = *coins.remove(0).id();
    test.test_transfer(&mut coins, gas_obj_id).await;
    test.test_merge_and_split(&mut coins, gas_obj_id).await;
    test.test_call_contract(gas_obj_id).await;
}
