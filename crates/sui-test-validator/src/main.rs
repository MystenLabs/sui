use anyhow::Result;

use std::collections::BTreeMap;

use clap::{Parser, ValueHint};
use hyper::{Body, Client, Method, Request};
use move_package::BuildConfig;
use serde_json::json;

use sui::client_commands::{SuiClientCommandResult, SuiClientCommands, WalletContext};
use sui::client_commands::{EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_NAME, EXAMPLE_NFT_URL};
use sui_config::genesis_config::GenesisConfig;
use sui_config::SUI_CLIENT_CONFIG;
use sui_json::SuiJsonValue;
use sui_json_rpc::api::RpcReadApiClient;
use sui_json_rpc::api::RpcTransactionBuilderClient;
use sui_json_rpc::api::WalletSyncApiClient;
use sui_json_rpc_types::{
    GetObjectDataResponse, SuiObjectInfo, TransactionBytes, TransactionResponse,
};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::SuiSignature;
use sui_types::sui_serde::{Base64, Encoding};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use test_utils::network::{start_rpc_test_network_with_fullnode, TestNetwork};

/// Start a Sui validator and fullnode for easy testing.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Config directory to use
    #[clap(short, long, parse(from_os_str), value_hint = ValueHint::DirPath)]
    config: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let network = create_network(&args).await?;
    println!("RPC URL: {}", network.rpc_url);
    // TODO: Make the debug data optional:
    create_response_sample(&network).await?;

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        for node in network.network.validators() {
            node.health_check().await?;
        }

        interval.tick().await;
    }
}

async fn create_network(args: &Args) -> Result<TestNetwork, anyhow::Error> {
    let config_dir = if let Some(path) = &args.config {
        Some(path.as_path())
    } else {
        None
    };

    let network = start_rpc_test_network_with_fullnode(
        Some(GenesisConfig::for_local_testing()),
        1,
        config_dir,
    )
    .await?;

    // Let nodes connect to one another
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    Ok(network)
}

async fn create_response_sample(network: &TestNetwork) -> Result<(), anyhow::Error> {
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

    get_nft_response(&mut context).await?;
    create_package_object_response(&mut context).await?;
    let hero_package = create_hero_response(&mut context, &coins).await?;
    create_transfer_response(&mut context, address, &coins).await?;
    create_transfer_sui_response(&mut context, address, &coins).await?;
    create_coin_split_response(&mut context, &coins).await?;
    create_error_response(address, hero_package, context, &network).await?;

    // address and owned objects
    let mut owned_objects = BTreeMap::new();
    for account in &network.accounts {
        network.http_client.sync_account_state(*account).await?;
        let objects: Vec<SuiObjectInfo> = network
            .http_client
            .get_objects_owned_by_address(*account)
            .await?;
        owned_objects.insert(account, objects);
    }

    Ok(())
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
) -> Result<(), anyhow::Error> {
    let response = SuiClientCommands::Transfer {
        to: address,
        coin_object_id: coins.first().unwrap().object_id,
        gas: None,
        gas_budget: 1000,
    }
    .execute(context)
    .await?;
    if let SuiClientCommandResult::Transfer(..) = response {
        Ok(())
    } else {
        panic!()
    }
}

async fn create_transfer_sui_response(
    context: &mut WalletContext,
    address: SuiAddress,
    coins: &[SuiObjectInfo],
) -> Result<(), anyhow::Error> {
    let response = SuiClientCommands::TransferSui {
        to: address,
        sui_coin_object_id: coins.first().unwrap().object_id,
        gas_budget: 1000,
        amount: Some(10),
    }
    .execute(context)
    .await?;
    if let SuiClientCommandResult::TransferSui(..) = response {
        Ok(())
    } else {
        panic!()
    }
}

async fn create_hero_response(
    context: &mut WalletContext,
    coins: &[SuiObjectInfo],
) -> Result<ObjectID, anyhow::Error> {
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

        if let SuiClientCommandResult::Call(..) = result {
            Ok(package_id)
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
) -> Result<(), anyhow::Error> {
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
    let _body = hyper::body::aggregate(res).await?;
    Ok(())
}

async fn create_coin_split_response(
    context: &mut WalletContext,
    coins: &[SuiObjectInfo],
) -> Result<(), anyhow::Error> {
    // create coin_split response
    let result = SuiClientCommands::SplitCoin {
        coin_id: coins.first().unwrap().object_id,
        amounts: vec![20, 20, 20, 20, 20],
        gas: None,
        gas_budget: 1000,
    }
    .execute(context)
    .await?;

    if let SuiClientCommandResult::SplitCoin(..) = result {
        Ok(())
    } else {
        panic!()
    }
}

async fn get_nft_response(context: &mut WalletContext) -> Result<(), anyhow::Error> {
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

    if let SuiClientCommandResult::Call(..) = result {
        Ok(())
    } else {
        panic!()
    }
}
