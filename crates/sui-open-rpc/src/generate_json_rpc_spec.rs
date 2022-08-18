// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;

use clap::ArgEnum;
use clap::Parser;
use hyper::body::Buf;
use hyper::{Body, Client, Method, Request};
use move_package::BuildConfig;
use pretty_assertions::assert_str_eq;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sui_types::messages::Transaction;

use crate::examples::RpcExampleProvider;
use sui::client_commands::{SuiClientCommandResult, SuiClientCommands, WalletContext};
use sui::client_commands::{EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_NAME, EXAMPLE_NFT_URL};
use sui_config::genesis_config::GenesisConfig;
use sui_config::SUI_CLIENT_CONFIG;
use sui_json::SuiJsonValue;
use sui_json_rpc::api::EventStreamingApiOpenRpc;
use sui_json_rpc::api::RpcReadApiClient;
use sui_json_rpc::api::RpcTransactionBuilderClient;
use sui_json_rpc::api::WalletSyncApiClient;
use sui_json_rpc::bcs_api::BcsApiImpl;
use sui_json_rpc::gateway_api::{GatewayWalletSyncApiImpl, RpcGatewayImpl, TransactionBuilderImpl};
use sui_json_rpc::read_api::{FullNodeApi, ReadApi};
use sui_json_rpc::sui_rpc_doc;
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    GetObjectDataResponse, MoveFunctionArgType, ObjectValueKind, SuiObjectInfo,
    SuiTransactionResponse, TransactionBytes,
};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use test_utils::network::{start_rpc_test_network, TestNetwork};

mod examples;

#[derive(Debug, Parser, Clone, Copy, ArgEnum)]
enum Action {
    Print,
    Test,
    Record,
}

#[derive(Debug, Parser)]
#[clap(
    name = "Sui format generator",
    about = "Trace serde (de)serialization to generate format descriptions for Sui types"
)]
struct Options {
    #[clap(arg_enum, default_value = "Record", ignore_case = true)]
    action: Action,
}

const FILE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/openrpc.json",);

const MOVE_SAMPLE_FILE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/move.json",);

const OBJECT_SAMPLE_FILE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/objects.json",);

const TRANSACTION_SAMPLE_FILE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/samples/transactions.json",);

const OWNED_OBJECT_SAMPLE_FILE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/samples/owned_objects.json",);

#[tokio::main]
async fn main() {
    let options = Options::parse();

    let mut open_rpc = sui_rpc_doc();
    open_rpc.add_module(TransactionBuilderImpl::rpc_doc_module());
    open_rpc.add_module(RpcGatewayImpl::rpc_doc_module());
    open_rpc.add_module(ReadApi::rpc_doc_module());
    open_rpc.add_module(FullNodeApi::rpc_doc_module());
    open_rpc.add_module(BcsApiImpl::rpc_doc_module());
    open_rpc.add_module(EventStreamingApiOpenRpc::module_doc());
    // TODO: Re-enable this when event read API is ready
    //open_rpc.add_module(EventReadApiOpenRpc::module_doc());
    open_rpc.add_module(GatewayWalletSyncApiImpl::rpc_doc_module());

    open_rpc.add_examples(RpcExampleProvider::new().examples());

    match options.action {
        Action::Print => {
            let content = serde_json::to_string_pretty(&open_rpc).unwrap();
            println!("{content}");
            let (move_info, objects, txs, addresses) = create_response_sample().await.unwrap();
            println!("{}", serde_json::to_string_pretty(&move_info).unwrap());
            println!("{}", serde_json::to_string_pretty(&objects).unwrap());
            println!("{}", serde_json::to_string_pretty(&txs).unwrap());
            println!("{}", serde_json::to_string_pretty(&addresses).unwrap());
        }
        Action::Record => {
            let content = serde_json::to_string_pretty(&open_rpc).unwrap();
            let mut f = File::create(FILE_PATH).unwrap();
            writeln!(f, "{content}").unwrap();
            let (move_info, objects, txs, addresses) = create_response_sample().await.unwrap();
            let content = serde_json::to_string_pretty(&move_info).unwrap();
            let mut f = File::create(MOVE_SAMPLE_FILE_PATH).unwrap();
            writeln!(f, "{content}").unwrap();
            let content = serde_json::to_string_pretty(&objects).unwrap();
            let mut f = File::create(OBJECT_SAMPLE_FILE_PATH).unwrap();
            writeln!(f, "{content}").unwrap();
            let content = serde_json::to_string_pretty(&txs).unwrap();
            let mut f = File::create(TRANSACTION_SAMPLE_FILE_PATH).unwrap();
            writeln!(f, "{content}").unwrap();
            let content = serde_json::to_string_pretty(&addresses).unwrap();
            let mut f = File::create(OWNED_OBJECT_SAMPLE_FILE_PATH).unwrap();
            writeln!(f, "{content}").unwrap();
        }
        Action::Test => {
            let reference = std::fs::read_to_string(FILE_PATH).unwrap();
            let content = serde_json::to_string_pretty(&open_rpc).unwrap() + "\n";
            assert_str_eq!(&reference, &content);
        }
    }
}

async fn create_response_sample() -> Result<
    (
        MoveResponseSample,
        ObjectResponseSample,
        TransactionResponseSample,
        BTreeMap<SuiAddress, Vec<SuiObjectInfo>>,
    ),
    anyhow::Error,
> {
    let network = start_rpc_test_network(Some(GenesisConfig::custom_genesis(1, 4, 30))).await?;
    let working_dir = network.network.dir();
    let config = working_dir.join(SUI_CLIENT_CONFIG);

    let mut context = WalletContext::new(&config).await?;
    let address = context.keystore.addresses().first().cloned().unwrap();

    context.gateway.sync_account_state(address).await?;

    // Create coin response
    let coins = context
        .gateway
        .get_objects_owned_by_address(address)
        .await?;
    let coin = context
        .gateway
        .get_object(coins.first().unwrap().object_id)
        .await?;

    let example_move_function_arg_types = create_move_function_arg_type_response()?;

    let (example_nft_tx, example_nft) = get_nft_response(&mut context).await?;
    let (move_package, publish) = create_package_object_response(&mut context).await?;
    let (hero_package, hero) = create_hero_response(&mut context, &coins).await?;
    let transfer = create_transfer_response(&mut context, address, &coins).await?;
    let transfer_sui = create_transfer_sui_response(&mut context, address, &coins).await?;
    let coin_split = create_coin_split_response(&mut context, &coins).await?;
    let error = create_error_response(address, hero_package, context, &network).await?;

    // address and owned objects
    let mut owned_objects = BTreeMap::new();
    for account in network.accounts {
        network.http_client.sync_account_state(account).await?;
        let objects: Vec<SuiObjectInfo> = network
            .http_client
            .get_objects_owned_by_address(account)
            .await?;
        owned_objects.insert(account, objects);
    }

    let move_info = MoveResponseSample {
        move_function_arg_types: example_move_function_arg_types,
    };

    let objects = ObjectResponseSample {
        example_nft,
        coin,
        move_package,
        hero,
    };

    let txs = TransactionResponseSample {
        move_call: example_nft_tx,
        transfer,
        transfer_sui,
        coin_split,
        publish,
        error,
    };

    Ok((move_info, objects, txs, owned_objects))
}

fn create_move_function_arg_type_response() -> Result<Vec<MoveFunctionArgType>, anyhow::Error> {
    Ok(vec![
        MoveFunctionArgType::Pure,
        MoveFunctionArgType::Object(ObjectValueKind::ByImmutableReference),
        MoveFunctionArgType::Object(ObjectValueKind::ByMutableReference),
        MoveFunctionArgType::Object(ObjectValueKind::ByValue),
    ])
}

async fn create_package_object_response(
    context: &mut WalletContext,
) -> Result<(GetObjectDataResponse, SuiTransactionResponse), anyhow::Error> {
    let package_path = ["sui_programmability", "examples", "move_tutorial"]
        .into_iter()
        .collect();
    let build_config = BuildConfig::default();
    let result = SuiClientCommands::Publish {
        package_path,
        build_config,
        gas: None,
        gas_budget: 10000,
    }
    .execute(context)
    .await?;
    if let SuiClientCommandResult::Publish(response) = result {
        Ok((
            context
                .gateway
                .get_object(
                    response
                        .parsed_data
                        .clone()
                        .unwrap()
                        .to_publish_response()?
                        .package
                        .object_id,
                )
                .await?,
            response,
        ))
    } else {
        panic!()
    }
}

async fn create_transfer_response(
    context: &mut WalletContext,
    address: SuiAddress,
    coins: &[SuiObjectInfo],
) -> Result<SuiTransactionResponse, anyhow::Error> {
    let response = SuiClientCommands::Transfer {
        to: address,
        coin_object_id: coins.first().unwrap().object_id,
        gas: None,
        gas_budget: 1000,
    }
    .execute(context)
    .await?;
    if let SuiClientCommandResult::Transfer(_, certificate, effects) = response {
        Ok(SuiTransactionResponse {
            certificate,
            effects,
            timestamp_ms: None,
            parsed_data: None,
        })
    } else {
        panic!()
    }
}

async fn create_transfer_sui_response(
    context: &mut WalletContext,
    address: SuiAddress,
    coins: &[SuiObjectInfo],
) -> Result<SuiTransactionResponse, anyhow::Error> {
    let response = SuiClientCommands::TransferSui {
        to: address,
        sui_coin_object_id: coins.first().unwrap().object_id,
        gas_budget: 1000,
        amount: Some(10),
    }
    .execute(context)
    .await?;
    if let SuiClientCommandResult::TransferSui(certificate, effects) = response {
        Ok(SuiTransactionResponse {
            certificate,
            effects,
            timestamp_ms: None,
            parsed_data: None,
        })
    } else {
        panic!()
    }
}

async fn create_hero_response(
    context: &mut WalletContext,
    coins: &[SuiObjectInfo],
) -> Result<(ObjectID, GetObjectDataResponse), anyhow::Error> {
    // Create hero response
    let package_path = ["sui_programmability", "examples", "games"]
        .into_iter()
        .collect();
    let build_config = BuildConfig::default();
    let result = SuiClientCommands::Publish {
        package_path,
        gas: None,
        build_config,
        gas_budget: 10000,
    }
    .execute(context)
    .await?;
    if let SuiClientCommandResult::Publish(response) = result {
        let publish_resp = response.parsed_data.unwrap().to_publish_response().unwrap();
        let package_id = publish_resp.package.object_id;
        let game_info = publish_resp
            .created_objects
            .iter()
            .find(|o| o.data.type_().unwrap().ends_with("GameInfo"))
            .unwrap();

        let game_info = SuiJsonValue::new(json!(game_info.reference.object_id.to_hex_literal()))?;
        let coin = SuiJsonValue::new(json!(coins.first().unwrap().object_id.to_hex_literal()))?;
        let result = SuiClientCommands::Call {
            package: package_id,
            module: "hero".to_string(),
            function: "acquire_hero".to_string(),
            type_args: vec![],
            args: vec![game_info, coin],
            gas: None,
            gas_budget: 10000,
        }
        .execute(context)
        .await?;

        if let SuiClientCommandResult::Call(_, effect) = result {
            let hero = effect.created.first().unwrap();
            Ok((
                package_id,
                context.gateway.get_object(hero.reference.object_id).await?,
            ))
        } else {
            panic!()
        }
    } else {
        panic!()
    }
}

async fn create_error_response(
    address: SuiAddress,
    hero_package: ObjectID,
    context: WalletContext,
    network: &TestNetwork,
) -> Result<Value, anyhow::Error> {
    // Cannot use wallet command as it will return Err if tx status is Error
    // Using hyper to get the raw response instead
    let response: TransactionBytes = network
        .http_client
        .move_call(
            address,
            hero_package,
            "hero".to_string(),
            "new_game".to_string(),
            vec![],
            vec![],
            None,
            100,
        )
        .await?;

    let signer = context.keystore.signer(address);

    let tx = Transaction::from_data(response.to_data().unwrap(), &signer);

    let (tx_data, sig_scheme, signature_bytes, pub_key) = tx.to_network_data_for_execution();

    let client = Client::new();
    let request = Request::builder()
        .uri(network.rpc_url.clone())
        .method(Method::POST)
        .header("Content-Type", "application/json")
        .body(Body::from(format!(
            "{{ \"jsonrpc\": \"2.0\",\"method\": \"sui_executeTransaction\",\"params\": [{}, {}, {}, {}],\"id\": 1 }}",
            json![tx_data],
            json![sig_scheme],
            json![signature_bytes],
            json![pub_key]
        )))?;

    let res = client.request(request).await?;
    let body = hyper::body::aggregate(res).await?;
    let result: Map<String, Value> = serde_json::from_reader(body.reader())?;
    Ok(result["result"].clone())
}

async fn create_coin_split_response(
    context: &mut WalletContext,
    coins: &[SuiObjectInfo],
) -> Result<SuiTransactionResponse, anyhow::Error> {
    // create coin_split response
    let result = SuiClientCommands::SplitCoin {
        coin_id: coins.first().unwrap().object_id,
        amounts: vec![20, 20, 20, 20, 20],
        gas: None,
        gas_budget: 1000,
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::SplitCoin(resp) = result {
        Ok(resp)
    } else {
        panic!()
    }
}

async fn get_nft_response(
    context: &mut WalletContext,
) -> Result<(SuiTransactionResponse, GetObjectDataResponse), anyhow::Error> {
    // Create example-nft response
    let args_json = json!([EXAMPLE_NFT_NAME, EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_URL]);
    let args = args_json
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(SuiJsonValue::new)
        .collect::<Result<_, _>>()?;

    let result = SuiClientCommands::Call {
        package: ObjectID::from(SUI_FRAMEWORK_ADDRESS),
        module: "devnet_nft".to_string(),
        function: "mint".to_string(),
        type_args: vec![],
        args,
        gas: None,
        gas_budget: 10000,
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::Call(certificate, effects) = result {
        let object = context
            .gateway
            .get_object(effects.created.first().unwrap().reference.object_id)
            .await?;
        let tx = SuiTransactionResponse {
            certificate,
            effects,
            timestamp_ms: None,
            parsed_data: None,
        };
        Ok((tx, object))
    } else {
        panic!()
    }
}

#[derive(Serialize)]
struct MoveResponseSample {
    pub move_function_arg_types: Vec<MoveFunctionArgType>,
}

#[derive(Serialize)]
struct ObjectResponseSample {
    pub example_nft: GetObjectDataResponse,
    pub coin: GetObjectDataResponse,
    pub move_package: GetObjectDataResponse,
    pub hero: GetObjectDataResponse,
}

#[derive(Serialize)]
struct TransactionResponseSample {
    pub move_call: SuiTransactionResponse,
    pub transfer: SuiTransactionResponse,
    pub transfer_sui: SuiTransactionResponse,
    pub coin_split: SuiTransactionResponse,
    pub publish: SuiTransactionResponse,
    pub error: Value,
}
