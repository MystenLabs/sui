// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::http_server::{HttpServerBuilder, HttpServerHandle};
use move_core_types::identifier::Identifier;

use sui::config::{PersistedConfig, WalletConfig};
use sui::keystore::{Keystore, SuiKeystore};
use sui::rest_gateway::responses::ObjectResponse;
use sui::rpc_gateway::RpcGatewayClient;
use sui::rpc_gateway::TransactionBytes;
use sui::rpc_gateway::{RpcCallArg, RpcGatewayServer};
use sui::rpc_gateway::{RpcGatewayImpl, SignedTransaction};
use sui::sui_commands::SuiNetwork;
use sui::sui_json::{resolve_move_function_args, SuiJsonCallArg, SuiJsonValue};
use sui::{SUI_GATEWAY_CONFIG, SUI_WALLET_CONFIG};
use sui_core::gateway_state::gateway_responses::TransactionResponse;
use sui_framework::build_move_package_to_bytes;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::json_schema::Base64;
use sui_types::object::ObjectRead;
use sui_types::SUI_FRAMEWORK_ADDRESS;

use crate::rpc_server_tests::sui_network::start_test_network;

mod sui_network;

#[tokio::test]
async fn test_get_objects() -> Result<(), anyhow::Error> {
    let test_network = setup_test_network().await?;
    let http_client = test_network.http_client;
    let address = test_network.accounts.first().unwrap();

    http_client.sync_account_state(*address).await?;
    let result: ObjectResponse = http_client.get_owned_objects(*address).await?;
    let result = result
        .objects
        .into_iter()
        .map(|o| o.to_object_ref())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(5, result.len());
    Ok(())
}

#[tokio::test]
async fn test_transfer_coin() -> Result<(), anyhow::Error> {
    let test_network = setup_test_network().await?;
    let http_client = test_network.http_client;
    let address = test_network.accounts.first().unwrap();
    http_client.sync_account_state(*address).await?;
    let result: ObjectResponse = http_client.get_owned_objects(*address).await?;
    let objects = result
        .objects
        .into_iter()
        .map(|o| o.to_object_ref())
        .collect::<Result<Vec<_>, _>>()?;

    let tx_data: TransactionBytes = http_client
        .transfer_coin(
            *address,
            objects.first().unwrap().0,
            objects.last().unwrap().0,
            1000,
            *address,
        )
        .await?;

    let keystore = SuiKeystore::load_or_create(&test_network.working_dir.join("wallet.key"))?;
    let signature = keystore.sign(address, &tx_data.tx_bytes)?;

    let tx_response: TransactionResponse = http_client
        .execute_transaction(SignedTransaction::new(tx_data.tx_bytes, signature))
        .await?;

    let (_cert, effect) = tx_response.to_effect_response()?;
    assert_eq!(2, effect.mutated.len());

    Ok(())
}

#[tokio::test]
async fn test_publish() -> Result<(), anyhow::Error> {
    let test_network = setup_test_network().await?;
    let http_client = test_network.http_client;
    let address = test_network.accounts.first().unwrap();
    http_client.sync_account_state(*address).await?;
    let result: ObjectResponse = http_client.get_owned_objects(*address).await?;
    let objects = result
        .objects
        .into_iter()
        .map(|o| o.to_object_ref())
        .collect::<Result<Vec<_>, _>>()?;

    let gas = objects.first().unwrap();

    let compiled_modules = build_move_package_to_bytes(
        Path::new("../sui_programmability/examples/fungible_tokens"),
        false,
    )?
    .into_iter()
    .map(Base64)
    .collect::<Vec<_>>();

    let tx_data: TransactionBytes = http_client
        .publish(*address, compiled_modules, gas.0, 10000)
        .await?;

    let keystore = SuiKeystore::load_or_create(&test_network.working_dir.join("wallet.key"))?;
    let signature = keystore.sign(address, &tx_data.tx_bytes)?;

    let tx_response: TransactionResponse = http_client
        .execute_transaction(SignedTransaction::new(tx_data.tx_bytes, signature))
        .await?;

    let response = tx_response.to_publish_response()?;
    assert_eq!(2, response.created_objects.len());
    Ok(())
}

#[tokio::test]
async fn test_move_call() -> Result<(), anyhow::Error> {
    let test_network = setup_test_network().await?;
    let http_client = test_network.http_client;
    let address = test_network.accounts.first().unwrap();
    http_client.sync_account_state(*address).await?;
    let result: ObjectResponse = http_client.get_owned_objects(*address).await?;
    let objects = result
        .objects
        .into_iter()
        .map(|o| o.to_object_ref())
        .collect::<Result<Vec<_>, _>>()?;

    let gas = objects.first().unwrap();

    let package_id = ObjectID::new(SUI_FRAMEWORK_ADDRESS.into_bytes());
    let package: ObjectRead = http_client.get_object_info(package_id).await?;
    let package = package.into_object()?;
    let module = Identifier::new("ObjectBasics")?;
    let function = Identifier::new("create")?;

    let json_args = resolve_move_function_args(
        &package,
        module.clone(),
        function.clone(),
        vec![
            SuiJsonValue::from_str("10000")?,
            SuiJsonValue::from_str(&format!("\"0x{}\"", address))?,
        ],
    )?;
    let mut args = Vec::with_capacity(json_args.len());
    for json_arg in json_args {
        args.push(match json_arg {
            SuiJsonCallArg::Pure(bytes) => RpcCallArg::Pure(Base64(bytes)),
            SuiJsonCallArg::Object(id) => match http_client.get_object_info(id).await? {
                ObjectRead::Exists(_, obj, _) if obj.is_shared() => RpcCallArg::SharedObject(id),
                _ => RpcCallArg::ImmOrOwnedObject(id),
            },
        })
    }

    let tx_data: TransactionBytes = http_client
        .move_call(
            *address, package_id, module, function, None, args, gas.0, 1000,
        )
        .await?;

    let keystore = SuiKeystore::load_or_create(&test_network.working_dir.join("wallet.key"))?;
    let signature = keystore.sign(address, &tx_data.tx_bytes)?;

    let tx_response: TransactionResponse = http_client
        .execute_transaction(SignedTransaction::new(tx_data.tx_bytes, signature))
        .await?;

    let (_cert, effect) = tx_response.to_effect_response()?;
    assert_eq!(1, effect.created.len());
    Ok(())
}

async fn setup_test_network() -> Result<TestNetwork, anyhow::Error> {
    let working_dir = tempfile::tempdir()?.path().to_path_buf();
    let _network = start_test_network(&working_dir, None).await?;
    let (server_addr, rpc_server_handle) =
        start_rpc_gateway(&working_dir.join(SUI_GATEWAY_CONFIG)).await?;
    let wallet_conf: WalletConfig = PersistedConfig::read(&working_dir.join(SUI_WALLET_CONFIG))?;
    let http_client = HttpClientBuilder::default().build(format!("http://{}", server_addr))?;
    Ok(TestNetwork {
        _network,
        _rpc_server: rpc_server_handle,
        accounts: wallet_conf.accounts,
        http_client,
        working_dir,
    })
}

struct TestNetwork {
    _network: SuiNetwork,
    _rpc_server: HttpServerHandle,
    accounts: Vec<SuiAddress>,
    http_client: HttpClient,
    working_dir: PathBuf,
}

async fn start_rpc_gateway(
    config_path: &Path,
) -> Result<(SocketAddr, HttpServerHandle), anyhow::Error> {
    let server = HttpServerBuilder::default().build("127.0.0.1:0").await?;
    let addr = server.local_addr()?;
    let handle = server.start(RpcGatewayImpl::new(config_path)?.into_rpc())?;
    Ok((addr, handle))
}
