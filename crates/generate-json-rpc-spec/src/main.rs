// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;

use clap::ArgEnum;
use clap::Parser;
use hyper::body::Buf;
use hyper::{Body, Client, Method, Request};
use pretty_assertions::assert_str_eq;
use serde::Serialize;
use serde_json::{json, Map, Value};

use sui::wallet_commands::{WalletCommandResult, WalletCommands, WalletContext};
use sui::wallet_commands::{EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_NAME, EXAMPLE_NFT_URL};
use sui_config::genesis_config::GenesisConfig;
use sui_config::SUI_WALLET_CONFIG;
use sui_json::SuiJsonValue;
use sui_json_rpc::bcs_api::BcsApiImpl;
use sui_json_rpc::gateway_api::{GatewayWalletSyncApiImpl, RpcGatewayImpl, TransactionBuilderImpl};
use sui_json_rpc::read_api::{FullNodeApi, ReadApi};
use sui_json_rpc::sui_rpc_doc;
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_api::rpc_types::{
    GetObjectDataResponse, SuiObjectInfo, TransactionEffectsResponse, TransactionResponse,
};
use sui_json_rpc_api::EventReadApiOpenRpc;
use sui_json_rpc_api::EventStreamingApiOpenRpc;
use sui_json_rpc_api::RpcReadApiClient;
use sui_json_rpc_api::RpcTransactionBuilderClient;
use sui_json_rpc_api::TransactionBytes;
use sui_json_rpc_api::WalletSyncApiClient;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::sui_serde::{Base64, Encoding};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use test_utils::network::{start_rpc_test_network, TestNetwork};

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

const FILE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sui-open-rpc/spec/openrpc.json",
);

const OBJECT_SAMPLE_FILE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sui-open-rpc/samples/objects.json",
);

const TRANSACTION_SAMPLE_FILE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sui-open-rpc/samples/transactions.json",
);

const OWNED_OBJECT_SAMPLE_FILE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sui-open-rpc/samples/owned_objects.json",
);

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
    open_rpc.add_module(EventReadApiOpenRpc::module_doc());
    open_rpc.add_module(GatewayWalletSyncApiImpl::rpc_doc_module());

    match options.action {
        Action::Print => {
            let content = serde_json::to_string_pretty(&open_rpc).unwrap();
            println!("{content}");
            let (objects, txs, addresses) = create_response_sample().await.unwrap();
            println!("{}", serde_json::to_string_pretty(&objects).unwrap());
            println!("{}", serde_json::to_string_pretty(&txs).unwrap());
            println!("{}", serde_json::to_string_pretty(&addresses).unwrap());
        }
        Action::Record => {
            let content = serde_json::to_string_pretty(&open_rpc).unwrap();
            let mut f = File::create(FILE_PATH).unwrap();
            writeln!(f, "{content}").unwrap();
            let (objects, txs, addresses) = create_response_sample().await.unwrap();
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
        ObjectResponseSample,
        TransactionResponseSample,
        BTreeMap<SuiAddress, Vec<SuiObjectInfo>>,
    ),
    anyhow::Error,
> {
    let network = start_rpc_test_network(Some(GenesisConfig::custom_genesis(1, 4, 30))).await?;
    let working_dir = network.network.dir();
    let config = working_dir.join(SUI_WALLET_CONFIG);

    let mut context = WalletContext::new(&config)?;
    let address = context.config.accounts.first().cloned().unwrap();

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

    let (example_nft_tx, example_nft) = get_nft_response(&mut context).await?;
    let (move_package, publish) = create_package_object_response(&mut context).await?;
    let (hero_package, hero) = create_hero_response(&mut context, &coins).await?;
    let transfer = create_transfer_response(&mut context, address, &coins).await?;
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

    let objects = ObjectResponseSample {
        example_nft,
        coin,
        move_package,
        hero,
    };

    let txs = TransactionResponseSample {
        move_call: example_nft_tx,
        transfer,
        coin_split,
        publish,
        error,
    };

    Ok((objects, txs, owned_objects))
}

async fn create_package_object_response(
    context: &mut WalletContext,
) -> Result<(GetObjectDataResponse, TransactionResponse), anyhow::Error> {
    let result = WalletCommands::Publish {
        path: "sui_programmability/examples/move_tutorial".to_string(),
        gas: None,
        gas_budget: 10000,
    }
    .execute(context)
    .await?;
    if let WalletCommandResult::Publish(response) = result {
        Ok((
            context
                .gateway
                .get_object(response.package.object_id)
                .await?,
            TransactionResponse::PublishResponse(response),
        ))
    } else {
        panic!()
    }
}

async fn create_transfer_response(
    context: &mut WalletContext,
    address: SuiAddress,
    coins: &[SuiObjectInfo],
) -> Result<TransactionResponse, anyhow::Error> {
    let response = WalletCommands::Transfer {
        to: address,
        coin_object_id: coins.first().unwrap().object_id,
        gas: None,
        gas_budget: 1000,
    }
    .execute(context)
    .await?;
    if let WalletCommandResult::Transfer(_, certificate, effects) = response {
        Ok(TransactionResponse::EffectResponse(
            TransactionEffectsResponse {
                certificate,
                effects,
                timestamp_ms: None,
            },
        ))
    } else {
        panic!()
    }
}

async fn create_hero_response(
    context: &mut WalletContext,
    coins: &[SuiObjectInfo],
) -> Result<(ObjectID, GetObjectDataResponse), anyhow::Error> {
    // Create hero response
    let result = WalletCommands::Publish {
        path: "sui_programmability/examples/games".to_string(),
        gas: None,
        gas_budget: 10000,
    }
    .execute(context)
    .await?;
    if let WalletCommandResult::Publish(response) = result {
        let package_id = response.package.object_id;
        let game_info = response
            .created_objects
            .iter()
            .find(|o| o.data.type_().unwrap().ends_with("GameInfo"))
            .unwrap();

        let game_info = SuiJsonValue::new(json!(game_info.reference.object_id.to_hex_literal()))?;
        let coin = SuiJsonValue::new(json!(coins.first().unwrap().object_id.to_hex_literal()))?;
        let result = WalletCommands::Call {
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

        if let WalletCommandResult::Call(_, effect) = result {
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

    let signature = context
        .keystore
        .sign(&address, &response.tx_bytes.to_vec()?)?;
    let signature_byte = Base64::encode(signature.signature_bytes());
    let pub_key = Base64::encode(signature.public_key_bytes());
    let tx_data = response.tx_bytes.encoded();

    let client = Client::new();
    let request = Request::builder()
        .uri(network.rpc_url.clone())
        .method(Method::POST)
        .header("Content-Type", "application/json")
        .body(Body::from(format!(
            "{{ \"jsonrpc\": \"2.0\",\"method\": \"sui_executeTransaction\",\"params\": [\"{}\", \"{}\", \"{}\"],\"id\": 1 }}",
            tx_data,
            signature_byte,
            pub_key
        )))?;

    let res = client.request(request).await?;
    let body = hyper::body::aggregate(res).await?;
    let result: Map<String, Value> = serde_json::from_reader(body.reader())?;
    Ok(result["result"].clone())
}

async fn create_coin_split_response(
    context: &mut WalletContext,
    coins: &[SuiObjectInfo],
) -> Result<TransactionResponse, anyhow::Error> {
    // create coin_split response
    let result = WalletCommands::SplitCoin {
        coin_id: coins.first().unwrap().object_id,
        amounts: vec![20, 20, 20, 20, 20],
        gas: None,
        gas_budget: 1000,
    }
    .execute(context)
    .await?;

    if let WalletCommandResult::SplitCoin(resp) = result {
        Ok(TransactionResponse::SplitCoinResponse(resp))
    } else {
        panic!()
    }
}

async fn get_nft_response(
    context: &mut WalletContext,
) -> Result<(TransactionResponse, GetObjectDataResponse), anyhow::Error> {
    // Create example-nft response
    let args_json = json!([EXAMPLE_NFT_NAME, EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_URL]);
    let args = args_json
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(SuiJsonValue::new)
        .collect::<Result<_, _>>()?;

    let result = WalletCommands::Call {
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

    if let WalletCommandResult::Call(certificate, effects) = result {
        let object = context
            .gateway
            .get_object(effects.created.first().unwrap().reference.object_id)
            .await?;
        let tx = TransactionResponse::EffectResponse(TransactionEffectsResponse {
            certificate,
            effects,
            timestamp_ms: None,
        });
        Ok((tx, object))
    } else {
        panic!()
    }
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
    pub move_call: TransactionResponse,
    pub transfer: TransactionResponse,
    pub coin_split: TransactionResponse,
    pub publish: TransactionResponse,
    pub error: Value,
}
