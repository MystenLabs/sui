// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_macros::sim_test;
use sui_move_build::BuildConfig;
use sui_rpc::proto::sui::rpc::v2beta2::live_data_service_client::LiveDataServiceClient;
use sui_rpc::proto::sui::rpc::v2beta2::{ExecutedTransaction, GasCostSummary};
use sui_rpc::proto::sui::rpc::v2beta2::{GetBalanceRequest, ListBalancesRequest};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    Argument, CallArg, Command, ObjectArg, TransactionData, TransactionKind,
};
use sui_types::{base_types::SuiAddress, Identifier};
use test_cluster::TestClusterBuilder;
const SUI_COIN_TYPE: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI";
const INITIAL_SUI_BALANCE: u64 = 150000000000000000;

#[sim_test]
async fn test_balance_apis() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut grpc_client = get_grpc_client(&test_cluster).await;

    let address = test_cluster.get_address_0();

    verify_balances(
        &mut grpc_client,
        address,
        &[(SUI_COIN_TYPE, INITIAL_SUI_BALANCE)],
    )
    .await;
}

#[sim_test]
async fn test_balance_changes_on_transfer() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut grpc_client = get_grpc_client(&test_cluster).await;

    let sender = test_cluster.get_address_0();
    let receiver = test_cluster.get_address_1();

    // Transfer some SUI
    let transfer_amount = 1000000;
    let txn = sui_test_transaction_builder::make_transfer_sui_transaction(
        &test_cluster.wallet,
        Some(receiver),
        Some(transfer_amount),
    )
    .await;

    let (_, gas_used) = execute_transaction(&test_cluster, &txn).await;

    verify_balances(
        &mut grpc_client,
        sender,
        &[(
            SUI_COIN_TYPE,
            INITIAL_SUI_BALANCE - transfer_amount - gas_used,
        )],
    )
    .await;

    verify_balances(
        &mut grpc_client,
        receiver,
        &[(SUI_COIN_TYPE, INITIAL_SUI_BALANCE + transfer_amount)],
    )
    .await;
}

#[sim_test]
async fn test_custom_coin_balance() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut grpc_client = get_grpc_client(&test_cluster).await;

    let address = test_cluster.get_address_0();

    // Publish trusted coin package
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "trusted_coin"]);
    let compiled_package = BuildConfig::new_for_testing().build(&path).unwrap();
    let compiled_modules_bytes = compiled_package.get_package_bytes(false);
    let dependencies = compiled_package.get_dependency_storage_package_ids();

    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();
    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.publish_immutable(compiled_modules_bytes, dependencies);
    let ptb = builder.finish();
    let gas_data = sui_types::transaction::GasData {
        payment: vec![(gas_object.0, gas_object.1, gas_object.2)],
        owner: address,
        price: gas_price,
        budget: 100_000_000,
    };

    let kind = TransactionKind::ProgrammableTransaction(ptb);
    let tx_data = TransactionData::new_with_gas_data(kind, address, gas_data);
    let txn = test_cluster.wallet.sign_transaction(&tx_data).await;

    let (transaction, publish_gas_used) = execute_transaction(&test_cluster, &txn).await;

    // Extract package ID from changed objects
    let package_id = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            use sui_rpc::proto::sui::rpc::v2beta2::changed_object::OutputObjectState;
            if o.output_state == Some(OutputObjectState::PackageWrite as i32) {
                o.object_id.clone()
            } else {
                None
            }
        })
        .unwrap();

    // Get treasury cap object from changed objects
    let treasury_cap = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find(|obj| {
            obj.object_type
                .as_ref()
                .map(|t| t.contains("TreasuryCap"))
                .unwrap_or(false)
        })
        .unwrap();

    // Mint some coins
    let mint_amount = 1_000_000; // 10 TRUSTED (with 2 decimals)
    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id.clone().parse().unwrap(),
            Identifier::new("trusted_coin").unwrap(),
            Identifier::new("mint").unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject((
                    treasury_cap.object_id.as_ref().unwrap().parse().unwrap(),
                    treasury_cap.output_version.unwrap().into(),
                    treasury_cap
                        .output_digest
                        .as_ref()
                        .unwrap()
                        .parse()
                        .unwrap(),
                ))),
                CallArg::Pure(bcs::to_bytes(&mint_amount).unwrap()),
            ],
        )
        .unwrap();
    let ptb = builder.finish();
    let tx_data = TestTransactionBuilder::new(address, gas_object, gas_price)
        .programmable(ptb)
        .build();
    let txn = test_cluster.wallet.sign_transaction(&tx_data).await;
    let (_, mint_gas_used) = execute_transaction(&test_cluster, &txn).await;

    // Check balances after minting
    let coin_type = format!("{}::trusted_coin::TRUSTED_COIN", package_id);
    verify_balances(
        &mut grpc_client,
        address,
        &[
            (
                SUI_COIN_TYPE,
                INITIAL_SUI_BALANCE - publish_gas_used - mint_gas_used,
            ),
            (&coin_type, mint_amount),
        ],
    )
    .await;

    // Transfer some of the custom coin from address_0 to address_1
    let address_1 = test_cluster.get_address_1();
    let transfer_amount = 300_000;

    // Query for the minted coin owned by address_0
    let sui_client = test_cluster.sui_client();
    let coins = sui_client
        .coin_read_api()
        .get_coins(address, Some(coin_type.clone()), None, Some(1))
        .await
        .unwrap();

    assert_eq!(
        coins.data.len(),
        1,
        "Expected exactly 1 coin, found {}",
        coins.data.len()
    );
    let coin = &coins.data[0];

    // Build and execute split-and-transfer transaction
    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();

    let transfer_gas_used = split_and_transfer_coin(
        &test_cluster,
        address,
        address_1,
        (gas_object.0, gas_object.1, gas_object.2),
        (coin.coin_object_id, coin.version, coin.digest),
        transfer_amount,
        gas_price,
    )
    .await;

    // Verify balances after transfer
    verify_balances(
        &mut grpc_client,
        address,
        &[
            (
                SUI_COIN_TYPE,
                INITIAL_SUI_BALANCE - publish_gas_used - mint_gas_used - transfer_gas_used,
            ),
            (&coin_type, mint_amount - transfer_amount),
        ],
    )
    .await;

    verify_balances(
        &mut grpc_client,
        address_1,
        &[
            (SUI_COIN_TYPE, INITIAL_SUI_BALANCE),
            (&coin_type, transfer_amount),
        ],
    )
    .await;

    // Test that address_3 returns 0 balance for the TRUSTED coin (not error since coin exists)
    let address_2 = test_cluster.get_address_2();
    let balance_response = grpc_client
        .get_balance({
            let mut message = GetBalanceRequest::default();
            message.owner = Some(address_2.to_string());
            message.coin_type = Some(coin_type.to_string());
            message
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(
        balance_response.balance.as_ref().unwrap().balance.unwrap(),
        0,
        "Expected 0 balance for address_3 with TRUSTED coin type"
    );
}

#[sim_test]
async fn test_multiple_concurrent_balance_changes() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut grpc_client = get_grpc_client(&test_cluster).await;

    let address_0 = test_cluster.get_address_0();
    let address_1 = test_cluster.get_address_1();
    let address_2 = test_cluster.get_address_2();

    let transfer_0_to_1 = 5_000_000;
    let transfer_1_to_2 = 3_000_000;
    let transfer_2_to_1 = 1_000_000;

    // Build all transactions upfront, but don't execute them.
    let tx_0 =
        build_split_and_transfer_transaction(&test_cluster, address_0, address_1, transfer_0_to_1)
            .await;

    let tx_1 =
        build_split_and_transfer_transaction(&test_cluster, address_1, address_2, transfer_1_to_2)
            .await;

    let tx_2 =
        build_split_and_transfer_transaction(&test_cluster, address_2, address_1, transfer_2_to_1)
            .await;

    // Sign all transactions
    let signed_tx_0 = test_cluster.wallet.sign_transaction(&tx_0).await;
    let signed_tx_1 = test_cluster.wallet.sign_transaction(&tx_1).await;
    let signed_tx_2 = test_cluster.wallet.sign_transaction(&tx_2).await;

    // Submit all transactions concurrently
    let channel = tonic::transport::Channel::from_shared(test_cluster.rpc_url().to_owned())
        .unwrap()
        .connect()
        .await
        .unwrap();

    let mut channel_0 = channel.clone();
    let mut channel_1 = channel.clone();
    let mut channel_2 = channel;

    let future_0 = super::super::execute_transaction(&mut channel_0, &signed_tx_0);
    let future_1 = super::super::execute_transaction(&mut channel_1, &signed_tx_1);
    let future_2 = super::super::execute_transaction(&mut channel_2, &signed_tx_2);

    // Wait for all transactions to complete
    let (result_0, result_1, result_2) = tokio::join!(future_0, future_1, future_2);

    // Calculate gas used from results
    let gas_used_0 = calculate_gas_used(
        result_0
            .effects
            .as_ref()
            .unwrap()
            .gas_used
            .as_ref()
            .unwrap(),
    );
    let gas_used_1 = calculate_gas_used(
        result_1
            .effects
            .as_ref()
            .unwrap()
            .gas_used
            .as_ref()
            .unwrap(),
    );
    let gas_used_2 = calculate_gas_used(
        result_2
            .effects
            .as_ref()
            .unwrap()
            .gas_used
            .as_ref()
            .unwrap(),
    );

    // Verify final balances after all transfers
    // address_0: sent 5M and paid gas
    verify_balances(
        &mut grpc_client,
        address_0,
        &[(
            SUI_COIN_TYPE,
            INITIAL_SUI_BALANCE - transfer_0_to_1 - gas_used_0,
        )],
    )
    .await;

    // address_1: received 5M, sent 3M, received 1M, paid gas
    // Net: +5M - 3M + 1M - gas = +3M - gas
    verify_balances(
        &mut grpc_client,
        address_1,
        &[(
            SUI_COIN_TYPE,
            INITIAL_SUI_BALANCE + transfer_0_to_1 - transfer_1_to_2 + transfer_2_to_1 - gas_used_1,
        )],
    )
    .await;

    // address_2: received 3M, sent 1M, paid gas
    // Net: +3M - 1M - gas = +2M - gas
    verify_balances(
        &mut grpc_client,
        address_2,
        &[(
            SUI_COIN_TYPE,
            INITIAL_SUI_BALANCE + transfer_1_to_2 - transfer_2_to_1 - gas_used_2,
        )],
    )
    .await;
}

#[sim_test]
async fn test_fresh_address_with_no_coins() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut grpc_client = get_grpc_client(&test_cluster).await;

    // Generate a new address that has never received any coins
    let fresh_address = SuiAddress::random_for_testing_only();

    // Get balance for SUI
    let response = grpc_client
        .get_balance({
            let mut message = GetBalanceRequest::default();
            message.owner = Some(fresh_address.to_string());
            message.coin_type = Some(SUI_COIN_TYPE.to_string());
            message
        })
        .await
        .unwrap()
        .into_inner();

    // Should return zero balance
    assert_eq!(0, response.balance.as_ref().unwrap().balance.unwrap());

    // List all balances for fresh address
    let list_response = grpc_client
        .list_balances({
            let mut message = ListBalancesRequest::default();
            message.owner = Some(fresh_address.to_string());
            message
        })
        .await
        .unwrap()
        .into_inner();

    // Should return empty list
    assert!(list_response.balances.is_empty());
    assert!(list_response.next_page_token.is_none());
}

#[sim_test]
async fn test_invalid_requests() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut grpc_client = get_grpc_client(&test_cluster).await;

    // Test with missing owner
    let mut request = GetBalanceRequest::default();
    request.coin_type = Some(SUI_COIN_TYPE.to_string());
    let result = grpc_client.get_balance(request).await;
    assert!(result.is_err(), "Expected error for missing owner");
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("missing owner"),
        "Expected error message to contain 'missing owner', but got: {}",
        error.message()
    );

    // Test with missing coin type - should error
    let address = test_cluster.get_address_0();
    let result = grpc_client
        .get_balance({
            let mut message = GetBalanceRequest::default();
            message.owner = Some(address.to_string());
            message
        })
        .await;
    assert!(result.is_err(), "Expected error for missing coin_type");
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("missing coin_type"),
        "Expected error message to contain 'missing coin_type', but got: {}",
        error.message()
    );

    // Test with invalid address format
    let result = grpc_client
        .get_balance({
            let mut message = GetBalanceRequest::default();
            message.owner = Some("not_a_hex_address".to_string());
            message.coin_type = Some(SUI_COIN_TYPE.to_string());
            message
        })
        .await;
    assert!(result.is_err(), "Expected error for invalid address format");
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("invalid owner"),
        "Expected error message to contain 'invalid owner', but got: {}",
        error.message()
    );

    // Test with invalid coin type format
    let result = grpc_client
        .get_balance({
            let mut message = GetBalanceRequest::default();
            message.owner = Some(address.to_string());
            message.coin_type = Some("invalid::coin::type::format".to_string());
            message
        })
        .await;
    assert!(result.is_err(), "Expected error for invalid coin type");
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("invalid coin_type"),
        "Expected error message to contain 'invalid coin_type', but got: {}",
        error.message()
    );

    // Test with non-existent coin type (well-formed but doesn't exist)
    let fake_coin_type =
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef::fakecoin::FAKECOIN";
    let result = grpc_client
        .get_balance({
            let mut message = GetBalanceRequest::default();
            message.owner = Some(address.to_string());
            message.coin_type = Some(fake_coin_type.to_string());
            message
        })
        .await;
    assert!(result.is_err(), "Expected error for non-existent coin type");
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("coin type does not exist"),
        "Expected error message to contain 'coin type does not exist', but got: {}",
        error.message()
    );

    // Test ListBalancesRequest with missing owner
    let result = grpc_client
        .list_balances(ListBalancesRequest::default())
        .await;
    assert!(
        result.is_err(),
        "Expected error for missing owner in list request"
    );
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("missing owner"),
        "Expected error message to contain 'missing owner', but got: {}",
        error.message()
    );

    // Test corrupted page token
    let result = grpc_client
        .list_balances({
            let mut message = ListBalancesRequest::default();
            message.owner = Some(address.to_string());
            message.page_token = Some(vec![0xFF, 0xDE, 0xAD, 0xBE, 0xEF].into());
            message
        })
        .await;
    assert!(result.is_err(), "Expected error for corrupted page token");
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
}

fn calculate_gas_used(gas_summary: &GasCostSummary) -> u64 {
    gas_summary.computation_cost.unwrap_or(0) + gas_summary.storage_cost.unwrap_or(0)
        - gas_summary.storage_rebate.unwrap_or(0)
}

async fn get_grpc_client(
    test_cluster: &test_cluster::TestCluster,
) -> LiveDataServiceClient<tonic::transport::Channel> {
    LiveDataServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap()
}

/// Execute a transaction and return both the transaction and the gas used
async fn execute_transaction(
    test_cluster: &test_cluster::TestCluster,
    txn: &sui_types::transaction::Transaction,
) -> (ExecutedTransaction, u64) {
    let mut channel = tonic::transport::Channel::from_shared(test_cluster.rpc_url().to_owned())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let transaction = super::super::execute_transaction(&mut channel, txn).await;
    let gas_summary = transaction
        .effects
        .as_ref()
        .unwrap()
        .gas_used
        .as_ref()
        .unwrap();
    let gas_used = calculate_gas_used(gas_summary);
    (transaction, gas_used)
}

/// Build a transaction that splits and transfers SUI
async fn build_split_and_transfer_transaction(
    test_cluster: &test_cluster::TestCluster,
    sender: SuiAddress,
    recipient: SuiAddress,
    amount: u64,
) -> TransactionData {
    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();
    let sui_client = test_cluster.sui_client();

    // Get coins (one for gas, one to split)
    let coins = sui_client
        .coin_read_api()
        .get_coins(sender, Some(SUI_COIN_TYPE.to_string()), None, None)
        .await
        .unwrap();

    // Use first coin as gas
    let gas_coin = &coins.data[0];
    let gas_object = (gas_coin.coin_object_id, gas_coin.version, gas_coin.digest);

    // Use second coin to split from
    let transfer_coin = &coins.data[1];
    let coin = (
        transfer_coin.coin_object_id,
        transfer_coin.version,
        transfer_coin.digest,
    );

    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin)).unwrap();
    let amt_arg = builder.pure(amount).unwrap();
    let split_result = builder.command(Command::SplitCoins(coin_arg, vec![amt_arg]));
    let split_coin = match split_result {
        Argument::Result(idx) => Argument::NestedResult(idx, 0),
        _ => panic!("Expected Result argument"),
    };
    builder.transfer_arg(recipient, split_coin);

    let ptb = builder.finish();
    TransactionData::new_programmable(sender, vec![gas_object], ptb, 100_000_000, gas_price)
}

/// Split coins from source and transfer to recipient, returning gas used
async fn split_and_transfer_coin(
    test_cluster: &test_cluster::TestCluster,
    sender: SuiAddress,
    recipient: SuiAddress,
    gas_object: (
        sui_types::base_types::ObjectID,
        sui_types::base_types::SequenceNumber,
        sui_types::base_types::ObjectDigest,
    ),
    coin: (
        sui_types::base_types::ObjectID,
        sui_types::base_types::SequenceNumber,
        sui_types::base_types::ObjectDigest,
    ),
    amount: u64,
    gas_price: u64,
) -> u64 {
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin)).unwrap();
    let amt_arg = builder.pure(amount).unwrap();
    let split_result = builder.command(Command::SplitCoins(coin_arg, vec![amt_arg]));
    let split_coin = match split_result {
        Argument::Result(idx) => Argument::NestedResult(idx, 0),
        _ => panic!("Expected Result argument"),
    };
    builder.transfer_arg(recipient, split_coin);

    let ptb = builder.finish();
    let tx_data =
        TransactionData::new_programmable(sender, vec![gas_object], ptb, 100_000_000, gas_price);
    let txn = test_cluster.wallet.sign_transaction(&tx_data).await;
    let (_, gas_used) = execute_transaction(test_cluster, &txn).await;
    gas_used
}

async fn verify_balances(
    grpc_client: &mut LiveDataServiceClient<tonic::transport::Channel>,
    address: SuiAddress,
    expected_balances: &[(&str, u64)],
) {
    // Verify each balance using get_balance
    for (coin_type, expected_balance) in expected_balances {
        let balance = grpc_client
            .get_balance({
                let mut message = GetBalanceRequest::default();
                message.owner = Some(address.to_string());
                message.coin_type = Some(coin_type.to_string());
                message
            })
            .await
            .unwrap()
            .into_inner()
            .balance
            .unwrap()
            .balance
            .unwrap();

        assert_eq!(
            balance, *expected_balance,
            "Balance mismatch for {} at address {}: expected {}, got {}",
            coin_type, address, expected_balance, balance
        );
    }

    // Also verify using list_balances
    let list_response = grpc_client
        .list_balances({
            let mut message = ListBalancesRequest::default();
            message.owner = Some(address.to_string());
            message
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(
        list_response.balances.len(),
        expected_balances.len(),
        "Expected {} coin types, but found {}",
        expected_balances.len(),
        list_response.balances.len()
    );

    for (coin_type, expected_balance) in expected_balances {
        let found = list_response
            .balances
            .iter()
            .find(|b| b.coin_type.as_ref() == Some(&coin_type.to_string()))
            .unwrap_or_else(|| panic!("Coin type {} not found in list_balances", coin_type));

        assert_eq!(
            found.balance,
            Some(*expected_balance),
            "Balance mismatch in list_balances for {} at address {}",
            coin_type,
            address
        );
    }
}
