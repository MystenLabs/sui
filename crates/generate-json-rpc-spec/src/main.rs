// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::ArgEnum;
use clap::Parser;
use pretty_assertions::assert_str_eq;
use serde::Serialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;

use sui::config::SUI_WALLET_CONFIG;
use sui::wallet_commands::{WalletCommandResult, WalletCommands, WalletContext};
use sui::wallet_commands::{EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_NAME, EXAMPLE_NFT_URL};
use sui_core::gateway_types::{
    GetObjectInfoResponse, TransactionEffectsResponse, TransactionResponse,
};
use sui_gateway::api::SuiRpcModule;
use sui_gateway::json_rpc::sui_rpc_doc;
use sui_gateway::read_api::{FullNodeApi, ReadApi};
use sui_gateway::rpc_gateway::{GatewayReadApiImpl, RpcGatewayImpl, TransactionBuilderImpl};
use sui_json::SuiJsonValue;
use sui_types::base_types::{ObjectID, ObjectInfo, SuiAddress};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use test_utils::network::start_test_network;

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

#[tokio::main]
async fn main() {
    let options = Options::parse();

    let mut open_rpc = sui_rpc_doc();
    open_rpc.add_module(TransactionBuilderImpl::rpc_doc_module());
    open_rpc.add_module(RpcGatewayImpl::rpc_doc_module());
    open_rpc.add_module(GatewayReadApiImpl::rpc_doc_module());
    open_rpc.add_module(ReadApi::rpc_doc_module());
    open_rpc.add_module(FullNodeApi::rpc_doc_module());

    match options.action {
        Action::Print => {
            let content = serde_json::to_string_pretty(&open_rpc).unwrap();
            println!("{content}");
            let (objects, txs) = create_response_sample().await.unwrap();
            println!("{}", serde_json::to_string_pretty(&objects).unwrap());
            println!("{}", serde_json::to_string_pretty(&txs).unwrap());
        }
        Action::Record => {
            let content = serde_json::to_string_pretty(&open_rpc).unwrap();
            let mut f = File::create(FILE_PATH).unwrap();
            writeln!(f, "{content}").unwrap();
            let (objects, txs) = create_response_sample().await.unwrap();
            let content = serde_json::to_string_pretty(&objects).unwrap();
            let mut f = File::create(OBJECT_SAMPLE_FILE_PATH).unwrap();
            writeln!(f, "{content}").unwrap();
            let content = serde_json::to_string_pretty(&txs).unwrap();
            let mut f = File::create(TRANSACTION_SAMPLE_FILE_PATH).unwrap();
            writeln!(f, "{content}").unwrap();
        }
        Action::Test => {
            let reference = std::fs::read_to_string(FILE_PATH).unwrap();
            let content = serde_json::to_string_pretty(&open_rpc).unwrap() + "\n";
            assert_str_eq!(&reference, &content);
        }
    }
}

async fn create_response_sample(
) -> Result<(ObjectResponseSample, TransactionResponseSample), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;
    let working_dir = working_dir.path();
    let _network = start_test_network(working_dir, None).await?;
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
        .get_object_info(coins.first().unwrap().object_id)
        .await?;

    let (example_nft_tx, example_nft) = get_nft_response(&mut context).await?;
    let move_package = create_package_object_response(&mut context).await?;
    let hero = create_hero_response(&mut context, &coins).await?;
    let transfer = create_transfer_response(&mut context, address, &coins).await?;
    let coin_split = create_coin_split_response(&mut context, &coins).await?;

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
    };

    Ok((objects, txs))
}

async fn create_package_object_response(
    context: &mut WalletContext,
) -> Result<GetObjectInfoResponse, anyhow::Error> {
    let result = WalletCommands::Publish {
        path: "sui_programmability/tutorial".to_string(),
        gas: None,
        gas_budget: 10000,
    }
    .execute(context)
    .await?;
    if let WalletCommandResult::Publish(response) = result {
        Ok(context
            .gateway
            .get_object_info(response.package.object_id)
            .await?)
    } else {
        panic!()
    }
}

async fn create_transfer_response(
    context: &mut WalletContext,
    address: SuiAddress,
    coins: &[ObjectInfo],
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
            },
        ))
    } else {
        panic!()
    }
}

async fn create_hero_response(
    context: &mut WalletContext,
    coins: &[ObjectInfo],
) -> Result<GetObjectInfoResponse, anyhow::Error> {
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
            module: "Hero".to_string(),
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
            Ok(context
                .gateway
                .get_object_info(hero.reference.object_id)
                .await?)
        } else {
            panic!()
        }
    } else {
        panic!()
    }
}

async fn create_coin_split_response(
    context: &mut WalletContext,
    coins: &[ObjectInfo],
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
) -> Result<(TransactionResponse, GetObjectInfoResponse), anyhow::Error> {
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
        module: "DevNetNFT".to_string(),
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
            .get_object_info(effects.created.first().unwrap().reference.object_id)
            .await?;
        let tx = TransactionResponse::EffectResponse(TransactionEffectsResponse {
            certificate,
            effects,
        });
        Ok((tx, object))
    } else {
        panic!()
    }
}

#[derive(Serialize)]
struct ObjectResponseSample {
    pub example_nft: GetObjectInfoResponse,
    pub coin: GetObjectInfoResponse,
    pub move_package: GetObjectInfoResponse,
    pub hero: GetObjectInfoResponse,
}

#[derive(Serialize)]
struct TransactionResponseSample {
    pub move_call: TransactionResponse,
    pub transfer: TransactionResponse,
    pub coin_split: TransactionResponse,
}
