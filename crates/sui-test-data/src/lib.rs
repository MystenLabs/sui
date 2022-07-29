// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use hyper::body::Buf;
use hyper::{Body, Client, Method, Request};
use move_package::BuildConfig;
use serde::Serialize;
use serde_json::{json, Map, Value};

use sui::client_commands::{SuiClientCommandResult, SuiClientCommands, WalletContext};
use sui::client_commands::{EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_NAME, EXAMPLE_NFT_URL};
use sui_config::SUI_CLIENT_CONFIG;
use sui_json::SuiJsonValue;
use sui_json_rpc::api::RpcReadApiClient;
use sui_json_rpc::api::RpcTransactionBuilderClient;
use sui_json_rpc::api::WalletSyncApiClient;
use sui_json_rpc_types::{
    GetObjectDataResponse, SuiObjectInfo, TransactionBytes, TransactionEffectsResponse,
    TransactionResponse,
};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::SuiSignature;
use sui_types::sui_serde::{Base64, Encoding};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use test_utils::network::{TestNetwork};

#[derive(Serialize)]
pub struct ObjectResponseSample {
    pub example_nft: GetObjectDataResponse,
    pub coin: GetObjectDataResponse,
    pub move_package: GetObjectDataResponse,
    pub hero: GetObjectDataResponse,
}

#[derive(Serialize)]
pub struct TransactionResponseSample {
    pub move_call: TransactionResponse,
    pub transfer: TransactionResponse,
    pub transfer_sui: TransactionResponse,
    pub coin_split: TransactionResponse,
    pub publish: TransactionResponse,
    pub error: Value,
}


pub async fn create_test_data(network: &TestNetwork) -> Result<
    (
        ObjectResponseSample,
        TransactionResponseSample,
        BTreeMap<SuiAddress, Vec<SuiObjectInfo>>,
    ),
    anyhow::Error,
> {
    let working_dir = network.network.dir();
    let config = working_dir.join(SUI_CLIENT_CONFIG);

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
    let transfer_sui = create_transfer_sui_response(&mut context, address, &coins).await?;
    let coin_split = create_coin_split_response(&mut context, &coins).await?;
    let error = create_error_response(address, hero_package, context, &network).await?;

    // address and owned objects
    let mut owned_objects = BTreeMap::new();
    for account in &network.accounts {
        network.http_client.sync_account_state(*account).await?;
        let objects: Vec<SuiObjectInfo> = network
            .http_client
            .get_objects_owned_by_address(*account)
            .await?;
        owned_objects.insert(account.clone(), objects);
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
        transfer_sui,
        coin_split,
        publish,
        error,
    };

    Ok((objects, txs, owned_objects))
}

async fn create_package_object_response(
    context: &mut WalletContext,
) -> Result<(GetObjectDataResponse, TransactionResponse), anyhow::Error> {
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
    let response = SuiClientCommands::Transfer {
        to: address,
        coin_object_id: coins.first().unwrap().object_id,
        gas: None,
        gas_budget: 1000,
    }
    .execute(context)
    .await?;
    if let SuiClientCommandResult::Transfer(_, certificate, effects) = response {
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

async fn create_transfer_sui_response(
    context: &mut WalletContext,
    address: SuiAddress,
    coins: &[SuiObjectInfo],
) -> Result<TransactionResponse, anyhow::Error> {
    let response = SuiClientCommands::TransferSui {
        to: address,
        sui_coin_object_id: coins.first().unwrap().object_id,
        gas_budget: 1000,
        amount: Some(10),
    }
    .execute(context)
    .await?;
    if let SuiClientCommandResult::TransferSui(certificate, effects) = response {
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
        let package_id = response.package.object_id;
        let game_info = response
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

    let signature = context
        .keystore
        .sign(&address, &response.tx_bytes.to_vec()?)?;
    let flag_bytes = Base64::encode(&[signature.flag_byte()]);
    let signature_byte = Base64::encode(signature.signature_bytes());
    let pub_key = Base64::encode(signature.public_key_bytes());
    let tx_data = response.tx_bytes.encoded();

    let client = Client::new();
    let request = Request::builder()
        .uri(network.rpc_url.clone())
        .method(Method::POST)
        .header("Content-Type", "application/json")
        .body(Body::from(format!(
            "{{ \"jsonrpc\": \"2.0\",\"method\": \"sui_executeTransaction\",\"params\": [\"{}\", \"{}\", \"{}\", \"{}\"],\"id\": 1 }}",
            tx_data,
            flag_bytes,
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
    let result = SuiClientCommands::SplitCoin {
        coin_id: coins.first().unwrap().object_id,
        amounts: vec![20, 20, 20, 20, 20],
        gas: None,
        gas_budget: 1000,
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::SplitCoin(resp) = result {
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
