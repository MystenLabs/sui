use shared_crypto::intent::Intent;
use sui_config::{
    sui_config_dir, Config, PersistedConfig, SUI_CLIENT_CONFIG, SUI_KEYSTORE_FILENAME,
};
use sui_deepbookv3::DataReader;
use sui_json_rpc_types::SuiTypeTag;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore};
use sui_sdk::{
    rpc_types::SuiTransactionBlockResponseOptions,
    sui_client_config::{SuiClientConfig, SuiEnv},
    types::{
        programmable_transaction_builder::ProgrammableTransactionBuilder,
        quorum_driver_types::ExecuteTransactionRequestType,
        transaction::{Transaction, TransactionData},
    },
    wallet_context::WalletContext,
    SuiClient, SuiClientBuilder,
};

pub fn retrieve_wallet() -> anyhow::Result<WalletContext> {
    let wallet_conf = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
    let keystore_path = sui_config_dir()?.join(SUI_KEYSTORE_FILENAME);

    // check if a wallet exists and if not, create a wallet and a sui client config
    if !keystore_path.exists() {
        let keystore = FileBasedKeystore::new(&keystore_path)?;
        keystore.save()?;
    }

    if !wallet_conf.exists() {
        let keystore = FileBasedKeystore::new(&keystore_path)?;
        let mut client_config = SuiClientConfig::new(keystore.into());

        client_config.add_env(SuiEnv::testnet());
        client_config.add_env(SuiEnv::devnet());
        client_config.add_env(SuiEnv::localnet());

        if client_config.active_env.is_none() {
            client_config.active_env = client_config.envs.first().map(|env| env.alias.clone());
        }

        client_config.save(&wallet_conf)?;
        println!("Client config file is stored in {:?}.", &wallet_conf);
    }

    let keystore = FileBasedKeystore::new(&keystore_path)?;
    let mut client_config: SuiClientConfig = PersistedConfig::read(&wallet_conf)?;

    let addresses = keystore.addresses();
    let default_active_address = addresses.first().unwrap();

    client_config.active_address = Some(default_active_address.clone());
    client_config.save(&wallet_conf)?;

    let wallet = WalletContext::new(&wallet_conf, Some(std::time::Duration::from_secs(60)), None)?;

    Ok(wallet)
}

#[allow(dead_code)]
pub async fn execute_transaction(ptb: ProgrammableTransactionBuilder) {
    let sui_client = SuiClientBuilder::default().build_testnet().await.unwrap();
    println!("Sui testnet version: {}", sui_client.api_version());

    let mut wallet = retrieve_wallet().unwrap();
    let sender = wallet.active_address().unwrap();
    println!("Sender: {}", sender);

    let coins = sui_client
        .coin_read_api()
        .get_coins(sender, None, None, None)
        .await
        .unwrap();
    let coin = coins.data.into_iter().next().unwrap();

    let gas_budget = 10_000_000;
    let gas_price = sui_client
        .read_api()
        .get_reference_gas_price()
        .await
        .unwrap();

    let builder = ptb.finish();
    println!("{:?}", builder);

    // create the transaction data that will be sent to the network
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![coin.object_ref()],
        builder,
        gas_budget,
        gas_price,
    );

    // 4) sign transaction
    let keystore =
        FileBasedKeystore::new(&sui_config_dir().unwrap().join(SUI_KEYSTORE_FILENAME)).unwrap();
    let signature = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .unwrap();

    // 5) execute the transaction
    print!("Executing the transaction...");
    let transaction_response = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(tx_data, vec![signature]),
            SuiTransactionBlockResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();
    println!("{}", transaction_response);
}

pub async fn dry_run_transaction(
    sui_client: &SuiClient,
    ptb: ProgrammableTransactionBuilder,
) -> anyhow::Result<Vec<(Vec<u8>, SuiTypeTag)>> {
    let mut wallet = retrieve_wallet().unwrap();
    let sender = wallet.active_address().unwrap();
    println!("Sender: {}", sender);

    sui_client.dev_inspect_transaction(sender, ptb).await
}
