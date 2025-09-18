// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FieldMask;
use std::path::PathBuf;
use std::str::FromStr;
use sui_macros::sim_test;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2::coin_metadata::MetadataCapState;
use sui_rpc::proto::sui::rpc::v2beta2::coin_treasury::SupplyState;
use sui_rpc::proto::sui::rpc::v2beta2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2beta2::live_data_service_client::LiveDataServiceClient;
use sui_rpc::proto::sui::rpc::v2beta2::owner::OwnerKind;
use sui_rpc::proto::sui::rpc::v2beta2::regulated_coin_metadata::CoinRegulatedState;
use sui_rpc::proto::sui::rpc::v2beta2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2beta2::{
    ExecutedTransaction, GetCoinInfoRequest, GetCoinInfoResponse, GetObjectRequest,
};
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::coin_registry::Currency;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, TransactionData};
use sui_types::{TypeTag, SUI_COIN_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::TestClusterBuilder;

// SUI doesn't use the CoinRegistry - it was created before the CoinRegistry system existed and has
// not been migrated.
#[sim_test]
async fn get_coin_info_sui() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = LiveDataServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let coin_type_sdk: TypeTag = "0x2::sui::SUI".parse().unwrap();
    let mut request = GetCoinInfoRequest::default();
    request.coin_type = Some(coin_type_sdk.to_string());

    let GetCoinInfoResponse {
        coin_type,
        metadata,
        treasury,
        regulated_metadata,
        ..
    } = grpc_client
        .get_coin_info(request)
        .await
        .unwrap()
        .into_inner();

    let expected_type = coin_type_sdk.to_canonical_string(true);
    assert_eq!(coin_type, Some(expected_type));

    let metadata = metadata.unwrap();

    let metadata_object_id = metadata.id.as_ref().unwrap();
    assert!(ObjectID::from_str(metadata_object_id).is_ok(),);
    assert_eq!(metadata.decimals, Some(9));
    assert_eq!(metadata.symbol, Some("SUI".to_owned()));
    assert_eq!(metadata.name, Some("Sui".to_string()));
    assert_eq!(metadata.description, Some("".to_string()));
    assert!(metadata.icon_url.is_none());
    assert!(metadata.metadata_cap_state.is_none());

    let treasury = treasury.unwrap();
    assert!(treasury.id.is_none());
    assert_eq!(
        treasury.total_supply,
        Some(sui_types::gas_coin::TOTAL_SUPPLY_MIST)
    );
    assert_eq!(
        treasury.supply_state,
        Some(SupplyState::Fixed as i32),
        "SUI should have Fixed supply state"
    );

    // SUI is not in CoinRegistry, so regulation state is Unknown
    let regulated_metadata = regulated_metadata.unwrap();
    assert_eq!(
        regulated_metadata.coin_regulated_state,
        Some(CoinRegulatedState::Unknown as i32)
    );
}

#[sim_test]
async fn test_get_coin_info_registry_coin() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = get_grpc_client(&test_cluster).await;

    let address = test_cluster.get_address_0();

    // Publish coin package using new_currency_with_otw
    let (package_id, treasury_cap, metadata_cap) =
        publish_registry_coin(&test_cluster, address).await;

    let coin_type = format!("{}::registry_coin::REGISTRY_COIN", package_id);

    finalize_registration(&test_cluster, package_id, &coin_type).await;

    // Mint some coins to test Fixed supply state
    let mint_amount = 5_000_000u64;

    let treasury_cap_obj = test_cluster
        .get_object_from_fullnode_store(&treasury_cap)
        .await
        .unwrap();
    let treasury_cap_ref = treasury_cap_obj.compute_object_reference();

    // Build a transaction to mint coins
    let mut ptb = ProgrammableTransactionBuilder::new();
    let treasury_cap_arg = ptb
        .obj(ObjectArg::ImmOrOwnedObject(treasury_cap_ref))
        .unwrap();
    let amount_arg = ptb.pure(mint_amount).unwrap();
    let recipient_arg = ptb.pure(address).unwrap();

    ptb.programmable_move_call(
        package_id,
        "registry_coin".parse().unwrap(),
        "mint".parse().unwrap(),
        vec![],
        vec![treasury_cap_arg, amount_arg, recipient_arg],
    );

    let pt = ptb.finish();
    let gas_price = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();
    let tx_data = TransactionData::new_programmable(address, vec![gas], pt, 50_000_000, gas_price);

    // Execute the mint transaction
    let _mint_tx = test_cluster.sign_and_execute_transaction(&tx_data).await;

    let response = grpc_client
        .get_coin_info({
            let mut request = GetCoinInfoRequest::default();
            request.coin_type = Some(coin_type.clone());
            request
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.coin_type, Some(coin_type.clone()));

    let metadata = response.metadata.expect("Expected metadata to be present");
    let metadata_object_id = metadata.id.as_ref().unwrap();
    assert!(
        ObjectID::from_str(metadata_object_id).is_ok(),
        "metadata.id should be a valid ObjectID"
    );
    assert_eq!(metadata.decimals, Some(6));
    assert_eq!(metadata.name, Some("Registry Coin".to_string()));
    assert_eq!(metadata.symbol, Some("REGISTRY".to_string()));
    assert_eq!(
        metadata.description,
        Some("Registry coin for testing GetCoinInfo with CoinRegistry".to_string())
    );
    assert_eq!(
        metadata.icon_url,
        Some("https://example.com/registry.png".to_string())
    );
    // Check that metadata cap is claimed with the correct ID
    if metadata.metadata_cap_state == Some(MetadataCapState::Claimed as i32) {
        let cap_id = metadata
            .metadata_cap_id
            .as_ref()
            .expect("Expected metadata_cap_id when state is Claimed");
        assert!(
            ObjectID::from_str(cap_id).is_ok(),
            "metadata_cap_state.claimed should be a valid ObjectID"
        );
        assert_eq!(cap_id, &metadata_cap.to_string());
    } else {
        panic!("Expected metadata_cap_state to be Claimed");
    }
    assert!(response.treasury.is_some());
    let treasury = response.treasury.unwrap();
    assert!(
        ObjectID::from_str(treasury.id.as_ref().unwrap()).is_ok(),
        "treasury.id should be a valid ObjectID"
    );

    assert_eq!(treasury.total_supply.unwrap(), 5_000_000);
    assert_eq!(
        treasury.supply_state,
        Some(SupplyState::Unknown as i32),
        "Treasury cap not owned by 0x0 should have Unknown supply state"
    );

    let regulated_metadata = response.regulated_metadata.unwrap();
    assert_eq!(
        regulated_metadata.coin_regulated_state,
        Some(CoinRegulatedState::Unregulated as i32)
    );

    // Phase 2: Register the supply (consuming the TreasuryCap) and verify RPC reflects the update
    // Get the CoinRegistry object's initial shared version

    // Get the updated treasury cap reference after minting
    let updated_treasury_cap_obj = test_cluster
        .get_object_from_fullnode_store(&treasury_cap)
        .await
        .unwrap();
    let updated_treasury_cap_ref = updated_treasury_cap_obj.compute_object_reference();

    // Build a transaction to register the supply
    let mut ptb = ProgrammableTransactionBuilder::new();

    // Derive the Currency object ID using the same method as get_coin_info
    let coin_type_tag = move_core_types::language_storage::StructTag {
        address: package_id.into(),
        module: move_core_types::identifier::Identifier::new("registry_coin").unwrap(),
        name: move_core_types::identifier::Identifier::new("REGISTRY_COIN").unwrap(),
        type_params: vec![],
    };

    let currency_id = Currency::derive_object_id(coin_type_tag.into()).unwrap();

    // Get the Currency object to find its initial shared version
    let currency_obj = test_cluster
        .get_object_from_fullnode_store(&currency_id)
        .await
        .unwrap();

    let initial_shared_version = match currency_obj.owner {
        sui_types::object::Owner::Shared {
            initial_shared_version,
        } => initial_shared_version,
        _ => panic!("Currency object should be shared"),
    };

    // Add the Currency object at the derived address as a shared object
    let currency_arg = ptb
        .obj(ObjectArg::SharedObject {
            id: currency_id,
            initial_shared_version,
            mutable: true,
        })
        .unwrap();

    // Add the treasury cap as an owned object with updated reference
    let treasury_cap_arg = ptb
        .obj(ObjectArg::ImmOrOwnedObject(updated_treasury_cap_ref))
        .unwrap();

    // Call register_supply with Currency object instead of CoinRegistry
    ptb.programmable_move_call(
        package_id,
        "registry_coin".parse().unwrap(),
        "register_supply".parse().unwrap(),
        vec![],
        vec![currency_arg, treasury_cap_arg],
    );

    let pt = ptb.finish();
    let gas = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();
    let tx_data = TransactionData::new_programmable(address, vec![gas], pt, 50_000_000, gas_price);

    // Execute the register_supply transaction
    let _register_tx = test_cluster.sign_and_execute_transaction(&tx_data).await;

    // Query coin info again to verify the supply is now registered
    let response = grpc_client
        .get_coin_info({
            let mut request = GetCoinInfoRequest::default();
            request.coin_type = Some(coin_type.clone());
            request
        })
        .await
        .unwrap()
        .into_inner();

    assert!(response.treasury.is_some());
    let treasury_after = response.treasury.unwrap();

    // Now the supply should match the minted amount
    assert_eq!(
        treasury_after.total_supply.unwrap(),
        mint_amount,
        "Total supply should equal the minted amount after registering supply"
    );
    assert_eq!(
        treasury_after.supply_state,
        Some(SupplyState::Fixed as i32),
        "After register_supply, the supply state should be Fixed"
    );
}

#[sim_test]
async fn test_regulated_coin_info() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = get_grpc_client(&test_cluster).await;

    let address = test_cluster.get_address_0();

    let (package_id, treasury_cap, metadata_cap, deny_cap) =
        publish_regulated_coin(&test_cluster, address).await;

    let coin_type = format!("{}::regulated_coin::REGULATED_COIN", package_id);

    // Complete the registration by calling finalize_registration
    finalize_registration(&test_cluster, package_id, &coin_type).await;

    let response = grpc_client
        .get_coin_info({
            let mut request = GetCoinInfoRequest::default();
            request.coin_type = Some(coin_type.clone());
            request
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.coin_type, Some(coin_type.clone()));

    let metadata = response.metadata.unwrap();
    assert!(ObjectID::from_str(metadata.id.as_ref().unwrap()).is_ok(),);
    assert_eq!(metadata.decimals, Some(9));
    assert_eq!(metadata.name, Some("Regulated Coin".to_string()));
    assert_eq!(metadata.symbol, Some("REG".to_string()));
    assert_eq!(
        metadata.description,
        Some("Regulated coin for testing GetCoinInfo with CoinRegistry".to_string())
    );
    assert_eq!(
        metadata.icon_url,
        Some("https://example.com/regulated.png".to_string())
    );
    // Check that metadata cap is claimed with the correct ID
    if metadata.metadata_cap_state == Some(MetadataCapState::Claimed as i32) {
        let cap_id = metadata
            .metadata_cap_id
            .as_ref()
            .expect("Expected metadata_cap_id when state is Claimed");
        assert!(
            ObjectID::from_str(cap_id).is_ok(),
            "metadata_cap_state.claimed should be a valid ObjectID"
        );
        assert_eq!(cap_id, &metadata_cap.to_string());
    } else {
        panic!("Expected metadata_cap_state to be Claimed");
    }
    assert!(response.treasury.is_some());
    let treasury = response.treasury.unwrap();
    assert!(ObjectID::from_str(treasury.id.as_ref().unwrap()).is_ok(),);
    assert_eq!(treasury.id.unwrap(), treasury_cap.to_string());
    assert_eq!(treasury.total_supply.unwrap(), 0);
    assert_eq!(treasury.supply_state, Some(SupplyState::Unknown as i32),);

    let regulated = response
        .regulated_metadata
        .expect("Expected regulated_metadata");
    assert_eq!(
        regulated.coin_regulated_state,
        Some(CoinRegulatedState::Regulated as i32),
        "Expected coin to be regulated"
    );
    // CoinRegistry coins don't have separate RegulatedCoinMetadata objects
    assert!(regulated.id.is_none());
    assert!(regulated.coin_metadata_object.is_none());
    assert!(ObjectID::from_str(regulated.deny_cap_object.as_ref().unwrap()).is_ok(),);
    assert_eq!(regulated.deny_cap_object.unwrap(), deny_cap.to_string());
}

#[sim_test]
async fn test_legacy_coin_from_registry() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut grpc_client = get_grpc_client(&test_cluster).await;
    let sender = test_cluster.get_address_0();

    // Publish the legacy coin package that uses create_currency (v1)
    let (package_id, _metadata_id, treasury_cap_id) =
        publish_legacy_coin(&test_cluster, sender).await;
    let coin_type = format!("{}::legacy_coin::LEGACY_COIN", package_id);

    let mint_amount = 1_000_000_000u64;

    let treasury_cap_obj = test_cluster
        .get_object_from_fullnode_store(&treasury_cap_id)
        .await
        .unwrap();
    let treasury_cap_ref = treasury_cap_obj.compute_object_reference();

    let mut ptb = ProgrammableTransactionBuilder::new();
    let treasury_cap_arg = ptb
        .obj(ObjectArg::ImmOrOwnedObject(treasury_cap_ref))
        .unwrap();
    let amount_arg = ptb.pure(mint_amount).unwrap();
    let recipient_arg = ptb.pure(sender).unwrap();

    ptb.programmable_move_call(
        package_id,
        "legacy_coin".parse().unwrap(),
        "mint".parse().unwrap(),
        vec![],
        vec![treasury_cap_arg, amount_arg, recipient_arg],
    );

    let pt = ptb.finish();
    let gas_price = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();
    let tx_data = TransactionData::new_programmable(sender, vec![gas], pt, 50_000_000, gas_price);

    let _mint_tx = test_cluster.sign_and_execute_transaction(&tx_data).await;

    let response = grpc_client
        .get_coin_info({
            let mut request = GetCoinInfoRequest::default();
            request.coin_type = Some(coin_type.clone());
            request
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.coin_type, Some(coin_type.clone()));

    let metadata = response.metadata.as_ref().unwrap();
    assert!(metadata.id.is_some());
    let metadata_object_id = metadata.id.as_ref().unwrap();
    assert!(ObjectID::from_str(metadata_object_id).is_ok(),);
    assert_eq!(metadata.decimals, Some(8));
    assert_eq!(metadata.symbol, Some("LEGACY".to_string()));
    assert_eq!(metadata.name, Some("Legacy Coin".to_string()));
    assert_eq!(
        metadata.description,
        Some("Legacy coin for testing GetCoinInfo fallback".to_string())
    );
    assert_eq!(
        metadata.icon_url,
        Some("https://example.com/legacy.png".to_string())
    );
    assert!(metadata.metadata_cap_state.is_none());

    assert!(response.treasury.is_some());
    let treasury = response.treasury.unwrap();
    assert!(
        ObjectID::from_str(treasury.id.as_ref().unwrap()).is_ok(),
        "treasury.id should be a valid ObjectID"
    );

    assert_eq!(
        treasury.total_supply.unwrap(),
        mint_amount,
        "Total supply should equal the minted amount, verifying SupplyState::Fixed deserialization works"
    );
    assert_eq!(
        treasury.supply_state,
        Some(SupplyState::Unknown as i32),
        "Legacy coin treasury cap not owned by 0x0 should have Unknown supply state"
    );

    // Legacy coins from index return Unknown regulation state
    let regulated_metadata = response.regulated_metadata.unwrap();
    assert_eq!(
        regulated_metadata.coin_regulated_state,
        Some(CoinRegulatedState::Unknown as i32)
    );

    // Phase 2: Send the treasury cap to 0x0 to make the supply Fixed the old fashioned way.
    let updated_treasury_cap_obj = test_cluster
        .get_object_from_fullnode_store(&treasury_cap_id)
        .await
        .unwrap();
    let updated_treasury_cap_ref = updated_treasury_cap_obj.compute_object_reference();

    let mut ptb = ProgrammableTransactionBuilder::new();
    let treasury_cap_arg = ptb
        .obj(ObjectArg::ImmOrOwnedObject(updated_treasury_cap_ref))
        .unwrap();
    let zero_address_arg = ptb.pure(SuiAddress::ZERO).unwrap();

    ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        "transfer".parse().unwrap(),
        "public_transfer".parse().unwrap(),
        vec![TypeTag::from_str(&format!("0x2::coin::TreasuryCap<{}>", coin_type)).unwrap()],
        vec![treasury_cap_arg, zero_address_arg],
    );

    let pt = ptb.finish();
    let gas = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();
    let tx_data = TransactionData::new_programmable(sender, vec![gas], pt, 50_000_000, gas_price);

    let _transfer_tx = test_cluster.sign_and_execute_transaction(&tx_data).await;

    let response = grpc_client
        .get_coin_info({
            let mut request = GetCoinInfoRequest::default();
            request.coin_type = Some(coin_type.clone());
            request
        })
        .await
        .unwrap()
        .into_inner();

    assert!(response.treasury.is_some());
    let treasury_after = response.treasury.unwrap();

    assert_eq!(
        treasury_after.id.unwrap(),
        treasury_cap_id.to_string(),
        "Treasury cap object ID should remain unchanged"
    );

    assert_eq!(
        treasury_after.supply_state,
        Some(SupplyState::Fixed as i32),
        "After transferring treasury cap to 0x0, the supply state should be Fixed"
    );

    assert_eq!(
        treasury_after.total_supply.unwrap(),
        mint_amount,
        "Total supply should remain unchanged after transferring treasury cap"
    );
}

#[sim_test]
async fn test_invalid_coin_type() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut grpc_client = get_grpc_client(&test_cluster).await;

    // Test with malformed coin type
    let mut request = GetCoinInfoRequest::default();
    request.coin_type = Some("invalid::coin::type::format".to_string());

    let result = grpc_client.get_coin_info(request).await;
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("invalid coin_type"));

    // Test with non-existent coin type
    let fake_coin_type =
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef::fakecoin::FAKECOIN";
    let mut request = GetCoinInfoRequest::default();
    request.coin_type = Some(fake_coin_type.to_string());

    let result = grpc_client.get_coin_info(request).await;
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
    assert!(error.message().contains("Coin type"));
    assert!(error.message().contains("not found"));
}

#[sim_test]
async fn test_burnonly_coin_info() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut grpc_client = get_grpc_client(&test_cluster).await;
    let address = test_cluster.get_address_0();

    // Publish burnonly coin package
    let (package_id, treasury_cap, metadata_cap) =
        publish_burnonly_coin(&test_cluster, address).await;

    let coin_type = format!("{}::burnonly_coin::BURNONLY_COIN", package_id);

    // Complete the registration
    finalize_registration(&test_cluster, package_id, &coin_type).await;

    // First mint some coins before registering as BurnOnly
    let initial_mint_amount = 10_000_000_000u64; // 10 coins with 9 decimals

    let treasury_cap_obj = test_cluster
        .get_object_from_fullnode_store(&treasury_cap)
        .await
        .unwrap();
    let treasury_cap_ref = treasury_cap_obj.compute_object_reference();

    // Build a transaction to mint coins
    let mut ptb = ProgrammableTransactionBuilder::new();
    let treasury_cap_arg = ptb
        .obj(ObjectArg::ImmOrOwnedObject(treasury_cap_ref))
        .unwrap();
    let amount_arg = ptb.pure(initial_mint_amount).unwrap();
    let recipient_arg = ptb.pure(address).unwrap();

    ptb.programmable_move_call(
        package_id,
        "burnonly_coin".parse().unwrap(),
        "mint".parse().unwrap(),
        vec![],
        vec![treasury_cap_arg, amount_arg, recipient_arg],
    );

    let pt = ptb.finish();
    let gas_price = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();
    let tx_data = TransactionData::new_programmable(address, vec![gas], pt, 50_000_000, gas_price);

    // Execute the mint transaction
    let _mint_tx = test_cluster.sign_and_execute_transaction(&tx_data).await;

    // Query coin info before registering as BurnOnly
    let response_before = grpc_client
        .get_coin_info({
            let mut request = GetCoinInfoRequest::default();
            request.coin_type = Some(coin_type.clone());
            request
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response_before.coin_type, Some(coin_type.clone()));

    let metadata = response_before.metadata.expect("Expected metadata");
    assert_eq!(metadata.decimals, Some(9));
    assert_eq!(metadata.name, Some("BurnOnly Coin".to_string()));
    assert_eq!(metadata.symbol, Some("BURNONLY".to_string()));
    assert_eq!(
        metadata.description,
        Some(
            "BurnOnly coin for testing GetCoinInfo with CoinRegistry BurnOnly supply state"
                .to_string()
        )
    );
    assert_eq!(
        metadata.icon_url,
        Some("https://example.com/burnonly.png".to_string())
    );
    // Check that metadata cap is claimed with the correct ID
    if metadata.metadata_cap_state == Some(MetadataCapState::Claimed as i32) {
        let cap_id = metadata
            .metadata_cap_id
            .as_ref()
            .expect("Expected metadata_cap_id when state is Claimed");
        assert_eq!(cap_id, &metadata_cap.to_string());
    } else {
        panic!("Expected metadata_cap_state to be Claimed");
    }

    let treasury_before = response_before.treasury.unwrap();
    assert_eq!(treasury_before.total_supply.unwrap(), initial_mint_amount);
    assert_eq!(
        treasury_before.supply_state,
        Some(SupplyState::Unknown as i32),
        "Before registering as BurnOnly, supply state should be Unknown"
    );

    // Now register the supply as BurnOnly
    // Get the updated treasury cap reference after minting
    let updated_treasury_cap_obj = test_cluster
        .get_object_from_fullnode_store(&treasury_cap)
        .await
        .unwrap();
    let updated_treasury_cap_ref = updated_treasury_cap_obj.compute_object_reference();

    // Derive the Currency object ID
    let coin_type_tag = move_core_types::language_storage::StructTag {
        address: package_id.into(),
        module: move_core_types::identifier::Identifier::new("burnonly_coin").unwrap(),
        name: move_core_types::identifier::Identifier::new("BURNONLY_COIN").unwrap(),
        type_params: vec![],
    };

    let currency_id = Currency::derive_object_id(coin_type_tag.into()).unwrap();

    // Get the Currency object to find its initial shared version
    let currency_obj = test_cluster
        .get_object_from_fullnode_store(&currency_id)
        .await
        .unwrap();

    let initial_shared_version = match currency_obj.owner {
        sui_types::object::Owner::Shared {
            initial_shared_version,
        } => initial_shared_version,
        _ => panic!("Currency object should be shared"),
    };

    // Build a transaction to register supply as BurnOnly
    let mut ptb = ProgrammableTransactionBuilder::new();

    // Add the Currency object as a shared object
    let currency_arg = ptb
        .obj(ObjectArg::SharedObject {
            id: currency_id,
            initial_shared_version,
            mutable: true,
        })
        .unwrap();

    // Add the treasury cap as an owned object
    let treasury_cap_arg = ptb
        .obj(ObjectArg::ImmOrOwnedObject(updated_treasury_cap_ref))
        .unwrap();

    // Call register_supply_as_burnonly
    ptb.programmable_move_call(
        package_id,
        "burnonly_coin".parse().unwrap(),
        "register_supply_as_burnonly".parse().unwrap(),
        vec![],
        vec![currency_arg, treasury_cap_arg],
    );

    let pt = ptb.finish();
    let gas = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();
    let tx_data = TransactionData::new_programmable(address, vec![gas], pt, 50_000_000, gas_price);

    // Execute the register_supply_as_burnonly transaction
    let _register_tx = test_cluster.sign_and_execute_transaction(&tx_data).await;

    // Query coin info again to verify the supply is now BurnOnly
    let response_after = grpc_client
        .get_coin_info({
            let mut request = GetCoinInfoRequest::default();
            request.coin_type = Some(coin_type.clone());
            request
        })
        .await
        .unwrap()
        .into_inner();

    let treasury_after = response_after.treasury.unwrap();

    assert_eq!(
        treasury_after.total_supply.unwrap(),
        initial_mint_amount,
        "Total supply should remain the same after registering as BurnOnly"
    );
    assert_eq!(
        treasury_after.supply_state,
        Some(SupplyState::BurnOnly as i32),
        "After register_supply_as_burnonly, the supply state should be BurnOnly"
    );

    let regulated_metadata_after = response_after.regulated_metadata.unwrap();
    assert_eq!(
        regulated_metadata_after.coin_regulated_state,
        Some(CoinRegulatedState::Unregulated as i32)
    );
}

async fn finalize_registration(
    test_cluster: &test_cluster::TestCluster,
    _package_id: ObjectID,
    coin_type: &str,
) {
    let mut ledger_client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .expect("Failed to connect to ledger service");

    // Get the CoinRegistry object info to find its initial version
    let registry_obj_response = ledger_client
        .get_object({
            let mut request = GetObjectRequest::default();
            request.object_id = Some(SUI_COIN_REGISTRY_OBJECT_ID.to_string());
            request.read_mask = Some(FieldMask::from_str("version,owner"));
            request
        })
        .await
        .expect("Failed to get CoinRegistry object")
        .into_inner();

    let registry_initial_version = registry_obj_response
        .object
        .and_then(|obj| obj.owner)
        .and_then(|owner| {
            if owner.kind == Some(OwnerKind::Shared as i32) {
                owner.version.map(SequenceNumber::from)
            } else {
                None
            }
        })
        .expect("CoinRegistry should be a shared object with an initial version");

    // Now find the Currency object that was transferred to the CoinRegistry
    let mut grpc_client = get_grpc_client(test_cluster).await;

    let registry_owned = grpc_client
        .list_owned_objects({
            let mut request = ListOwnedObjectsRequest::default();
            request.owner = Some(SuiAddress::from(SUI_COIN_REGISTRY_OBJECT_ID).to_string());
            request.read_mask = Some(FieldMask::from_str(
                "object_id,version,digest,object_type,owner",
            ));
            request
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    // Parse the target coin type for normalized comparison
    let target_coin_type: TypeTag = coin_type.parse().unwrap();

    let currency = registry_owned
        .iter()
        .find(|obj| {
            let obj_type = obj.object_type.as_ref();

            obj_type
                .and_then(|t| {
                    // Parse the object type as a struct tag
                    match sui_types::parse_sui_struct_tag(t) {
                        Ok(struct_tag) => {
                            // Check if this is a Currency type and extract the type parameter
                            if struct_tag.module.as_str() == "coin_registry"
                                && struct_tag.name.as_str() == "Currency"
                                && struct_tag.type_params.len() == 1
                            {
                                // Compare the normalized type parameters
                                let matches = struct_tag
                                    .type_params
                                    .first()
                                    .map(|type_param| {
                                        let param_str = type_param.to_canonical_string(false);
                                        let target_str =
                                            target_coin_type.to_canonical_string(false);
                                        param_str == target_str
                                    })
                                    .unwrap_or(false);

                                Some(matches)
                            } else {
                                None
                            }
                        }
                        Err(_) => None,
                    }
                })
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("Currency for {} not found in CoinRegistry", coin_type));

    let currency_object_ref = (
        currency.object_id.as_ref().unwrap().parse().unwrap(),
        SequenceNumber::from(currency.version.unwrap()),
        currency.digest.as_ref().unwrap().parse().unwrap(),
    );

    let mut ptb = ProgrammableTransactionBuilder::new();

    // Add the CoinRegistry as a mutable shared object
    let registry_arg = ptb
        .obj(ObjectArg::SharedObject {
            id: SUI_COIN_REGISTRY_OBJECT_ID,
            initial_shared_version: registry_initial_version,
            mutable: true,
        })
        .unwrap();

    // Pass the Currency as a Receiving object
    let receiving_arg = ptb.obj(ObjectArg::Receiving(currency_object_ref)).unwrap();

    // Call finalize_registration directly in the framework
    ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        "coin_registry".parse().unwrap(),
        "finalize_registration".parse().unwrap(),
        vec![target_coin_type.clone()],
        vec![registry_arg, receiving_arg],
    );

    let pt = ptb.finish();
    let sender = test_cluster.get_address_0();

    let gas_price = test_cluster.get_reference_gas_price().await;

    let gas = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();

    let tx_data = TransactionData::new_programmable(sender, vec![gas], pt, 50_000_000, gas_price);

    // Sign the transaction
    let signed_tx = test_cluster.wallet.sign_transaction(&tx_data).await;

    // Execute the finalize_registration transaction and wait for checkpoint
    let mut client = sui_rpc::client::Client::new(test_cluster.rpc_url().to_owned()).unwrap();

    let _finalize_tx = super::super::execute_transaction(&mut client, &signed_tx).await;
}

async fn get_grpc_client(
    test_cluster: &test_cluster::TestCluster,
) -> LiveDataServiceClient<tonic::transport::Channel> {
    LiveDataServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap()
}

async fn publish_registry_coin(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
) -> (ObjectID, ObjectID, ObjectID) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "registry_coin"]);

    let (package_id, transaction) =
        super::super::publish_package(test_cluster, address, path).await;

    // Extract treasury cap and metadata cap from transaction
    let treasury_cap = find_object_by_type(&transaction, "TreasuryCap");
    let metadata_cap = find_object_by_type(&transaction, "MetadataCap");

    (package_id, treasury_cap, metadata_cap)
}

async fn publish_regulated_coin(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
) -> (ObjectID, ObjectID, ObjectID, ObjectID) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "regulated_coin"]);

    let (package_id, transaction) =
        super::super::publish_package(test_cluster, address, path).await;

    // Extract caps from transaction
    let treasury_cap = find_object_by_type(&transaction, "TreasuryCap");
    let metadata_cap = find_object_by_type(&transaction, "MetadataCap");
    let deny_cap = find_object_by_type(&transaction, "DenyCapV2");

    (package_id, treasury_cap, metadata_cap, deny_cap)
}

async fn publish_legacy_coin(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
) -> (ObjectID, ObjectID, ObjectID) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "legacy_coin"]);

    let (package_id, transaction) =
        super::super::publish_package(test_cluster, address, path).await;

    // Extract treasury cap and metadata object from transaction
    let treasury_cap = find_object_by_type(&transaction, "TreasuryCap");
    let metadata = find_object_by_type(&transaction, "CoinMetadata");

    (package_id, metadata, treasury_cap)
}

fn find_object_by_type(transaction: &ExecutedTransaction, type_substr: &str) -> ObjectID {
    transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find(|obj| {
            obj.object_type
                .as_ref()
                .map(|t| t.contains(type_substr))
                .unwrap_or(false)
        })
        .and_then(|obj| obj.object_id.as_ref().map(|id| id.parse().unwrap()))
        .unwrap_or_else(|| {
            panic!(
                "Object with type containing '{}' not found in transaction",
                type_substr
            )
        })
}

async fn publish_burnonly_coin(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
) -> (ObjectID, ObjectID, ObjectID) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "burnonly_coin"]);

    let (package_id, transaction) =
        super::super::publish_package(test_cluster, address, path).await;

    // Extract treasury cap and metadata cap from transaction
    let treasury_cap = find_object_by_type(&transaction, "TreasuryCap");
    let metadata_cap = find_object_by_type(&transaction, "MetadataCap");

    (package_id, treasury_cap, metadata_cap)
}
