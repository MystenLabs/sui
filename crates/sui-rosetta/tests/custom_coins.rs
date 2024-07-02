#[allow(dead_code)]
mod rosetta_client;
#[path = "custom_coins/test_coin_utils.rs"]
mod test_coin_utils;

use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, Currency, NetworkIdentifier,
    SuiEnv,
};
use test_cluster::TestClusterBuilder;
use test_coin_utils::{init_package, mint};

use crate::rosetta_client::{start_rosetta_test_server, RosettaEndpoint};

#[tokio::test]
async fn test_custom_coin_balance() {
    // mint coins to `test_culset.get_address_1()` and `test_culset.get_address_2()`
    const COIN1_BALANCE: u64 = 100_000_000;
    const COIN2_BALANCE: u64 = 200_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(&client, keystore, sender).await.unwrap();

    let address1 = test_cluster.get_address_1();
    let address2 = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, address1), (COIN2_BALANCE, address2)];

    let _mint_res = mint(&client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    // setup AccountBalanceRequest
    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };
    // Verify initial balance and stake
    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address: address1,
            sub_account: None,
        },
        block_identifier: Default::default(),
        currencies: vec![Currency {
            symbol: "TEST_COIN".to_string(),
            decimals: 6,
        }],
    };

    println!("request: {}", serde_json::to_string_pretty(&request).unwrap());
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await;
    println!("response: {}", serde_json::to_string_pretty(&response).unwrap());
    assert_eq!(response.balances[0].value, COIN1_BALANCE as i128);
}
