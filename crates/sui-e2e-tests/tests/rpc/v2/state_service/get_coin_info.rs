// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FieldMask;
use std::path::PathBuf;
use std::str::FromStr;
use sui_macros::sim_test;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoResponse;
use sui_rpc::proto::sui::rpc::v2::coin_metadata::MetadataCapState;
use sui_rpc::proto::sui::rpc::v2::coin_treasury::SupplyState;
use sui_rpc::proto::sui::rpc::v2::regulated_coin_metadata::CoinRegulatedState;
use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::coin_registry::Currency;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::SharedObjectMutability;
use sui_types::transaction::{ObjectArg, TransactionData};
use sui_types::{SUI_COIN_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID, TypeTag};
use test_cluster::TestClusterBuilder;

// SUI doesn't use the CoinRegistry - it was created before the CoinRegistry system existed and has
// not been migrated.
#[sim_test]
async fn get_coin_info_sui() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = get_grpc_client(&test_cluster).await;

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
    assert!(ObjectID::from_str(metadata_object_id).is_ok());
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

    let regulated_metadata = regulated_metadata.unwrap();
    assert_eq!(
        regulated_metadata.coin_regulated_state,
        Some(CoinRegulatedState::Unregulated as i32)
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
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();

    // Add the treasury cap as an owned object with updated reference
    let treasury_cap_arg = ptb
        .obj(ObjectArg::ImmOrOwnedObject(updated_treasury_cap_ref))
        .unwrap();

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

    // Query again to verify the supply state is now Fixed
    let response_after_register = grpc_client
        .get_coin_info({
            let mut request = GetCoinInfoRequest::default();
            request.coin_type = Some(coin_type.clone());
            request
        })
        .await
        .unwrap()
        .into_inner();

    let treasury_after = response_after_register.treasury.unwrap();
    assert_eq!(
        treasury_after.supply_state,
        Some(SupplyState::Fixed as i32),
        "After register_supply, treasury should have Fixed supply state"
    );
    assert_eq!(treasury_after.total_supply.unwrap(), 5_000_000);
}

#[sim_test]
async fn test_get_coin_info_burnonly_coin() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = get_grpc_client(&test_cluster).await;

    let address = test_cluster.get_address_0();

    // Publish coin package using new_currency_with_otw
    let (package_id, treasury_cap, _metadata_cap) =
        publish_burnonly_coin(&test_cluster, address).await;

    let coin_type = format!("{}::burnonly_coin::BURNONLY_COIN", package_id);

    finalize_registration(&test_cluster, package_id, &coin_type).await;

    // Mint some coins before registering as BurnOnly
    let mint_amount = 10_000_000u64;

    let treasury_cap_obj = test_cluster
        .get_object_from_fullnode_store(&treasury_cap)
        .await
        .unwrap();
    let treasury_cap_ref = treasury_cap_obj.compute_object_reference();

    let mut ptb = ProgrammableTransactionBuilder::new();
    let treasury_cap_arg = ptb
        .obj(ObjectArg::ImmOrOwnedObject(treasury_cap_ref))
        .unwrap();
    let amount_arg = ptb.pure(mint_amount).unwrap();
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
    let _mint_tx = test_cluster.sign_and_execute_transaction(&tx_data).await;

    // Register the supply as BurnOnly
    let updated_treasury_cap_obj = test_cluster
        .get_object_from_fullnode_store(&treasury_cap)
        .await
        .unwrap();
    let updated_treasury_cap_ref = updated_treasury_cap_obj.compute_object_reference();

    let mut ptb = ProgrammableTransactionBuilder::new();

    // Derive the Currency object ID
    let coin_type_tag = move_core_types::language_storage::StructTag {
        address: package_id.into(),
        module: move_core_types::identifier::Identifier::new("burnonly_coin").unwrap(),
        name: move_core_types::identifier::Identifier::new("BURNONLY_COIN").unwrap(),
        type_params: vec![],
    };

    let currency_id = Currency::derive_object_id(coin_type_tag.into()).unwrap();
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

    let currency_arg = ptb
        .obj(ObjectArg::SharedObject {
            id: currency_id,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();

    let treasury_cap_arg = ptb
        .obj(ObjectArg::ImmOrOwnedObject(updated_treasury_cap_ref))
        .unwrap();

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

    let _register_tx = test_cluster.sign_and_execute_transaction(&tx_data).await;

    // Query the coin info
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
    assert_eq!(metadata.decimals, Some(9));
    assert_eq!(metadata.name, Some("BurnOnly Coin".to_string()));
    assert_eq!(metadata.symbol, Some("BURNONLY".to_string()));

    let treasury = response.treasury.unwrap();
    assert_eq!(
        treasury.supply_state,
        Some(SupplyState::BurnOnly as i32),
        "After register_supply_as_burnonly, treasury should have BurnOnly supply state"
    );
    assert_eq!(treasury.total_supply.unwrap(), 10_000_000);
}

#[sim_test]
async fn test_get_coin_info_regulated_coin() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = get_grpc_client(&test_cluster).await;

    let address = test_cluster.get_address_0();

    // Publish regulated coin package
    let (package_id, _treasury_cap, _metadata_cap, deny_cap) =
        publish_regulated_coin(&test_cluster, address).await;

    let coin_type = format!("{}::regulated_coin::REGULATED_COIN", package_id);

    finalize_registration(&test_cluster, package_id, &coin_type).await;

    // Query the coin info
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
    assert_eq!(metadata.decimals, Some(9));
    assert_eq!(metadata.name, Some("Regulated Coin".to_string()));
    assert_eq!(metadata.symbol, Some("REG".to_string()));

    let regulated_metadata = response
        .regulated_metadata
        .expect("Expected regulated_metadata");
    assert_eq!(
        regulated_metadata.coin_regulated_state,
        Some(CoinRegulatedState::Regulated as i32),
        "Expected coin to be regulated"
    );
    assert!(regulated_metadata.id.is_none());
    assert!(regulated_metadata.coin_metadata_object.is_none());
    assert_eq!(
        regulated_metadata.deny_cap_object.unwrap(),
        deny_cap.to_string()
    );
}

#[sim_test]
async fn test_get_coin_info_non_otw_coin() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = get_grpc_client(&test_cluster).await;

    let address = test_cluster.get_address_0();

    // Publish non-OTW coin package using new_currency (without OTW)
    let (package_id, _treasury_cap, metadata_cap) =
        publish_non_otw_coin(&test_cluster, address).await;

    let coin_type = format!("{}::non_otw_coin::MyCoin", package_id);

    // Note: For non-OTW coins, the finalize in Move code already completes registration
    // We don't need to call finalize_registration separately

    // Query the coin info
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
    assert_eq!(metadata.decimals, Some(7));
    assert_eq!(metadata.name, Some("Non-OTW Coin".to_string()));
    assert_eq!(metadata.symbol, Some("NONOTW".to_string()));
    assert_eq!(
        metadata.description,
        Some("Non-OTW coin for testing GetCoinInfo with new_currency (without OTW)".to_string())
    );
    assert_eq!(
        metadata.icon_url,
        Some("https://example.com/non_otw.png".to_string())
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
    assert_eq!(treasury.total_supply.unwrap(), 0);
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
}

#[sim_test]
async fn test_get_coin_info_legacy_coin() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = get_grpc_client(&test_cluster).await;

    let address = test_cluster.get_address_0();

    // Publish legacy coin package using create_currency v1 API
    let (package_id, _treasury_cap, metadata_id) =
        publish_legacy_coin(&test_cluster, address).await;

    let coin_type = format!("{}::legacy_coin::LEGACY_COIN", package_id);

    // Query the coin info - should fallback to index-based lookup
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
    assert_eq!(metadata.id.unwrap(), metadata_id.to_string());
    assert_eq!(metadata.decimals, Some(8));
    assert_eq!(metadata.name, Some("Legacy Coin".to_string()));
    assert_eq!(metadata.symbol, Some("LEGACY".to_string()));
    assert_eq!(
        metadata.description,
        Some("Legacy coin for testing GetCoinInfo fallback".to_string())
    );
    assert_eq!(
        metadata.icon_url,
        Some("https://example.com/legacy.png".to_string())
    );
    assert!(
        metadata.metadata_cap_state.is_none(),
        "Legacy coins don't have metadata caps"
    );

    let treasury = response.treasury.unwrap();
    assert_eq!(treasury.total_supply.unwrap(), 0);
    assert_eq!(
        treasury.supply_state,
        Some(SupplyState::Unknown as i32),
        "Legacy coins have Unknown supply state when TreasuryCap is not owned by 0x0"
    );
}

// Helper function to publish the registry coin package
async fn publish_registry_coin(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
) -> (ObjectID, ObjectID, ObjectID) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "registry_coin"]);

    let (package_id_obj, transaction) =
        super::super::publish_package(test_cluster, address, path).await;
    let package_id = package_id_obj;

    // Get treasury cap and metadata cap from changed objects
    let treasury_cap = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            if o.object_type
                .as_ref()
                .map(|t| t.contains("TreasuryCap"))
                .unwrap_or(false)
            {
                o.object_id
                    .as_ref()
                    .and_then(|id| ObjectID::from_str(id).ok())
            } else {
                None
            }
        })
        .unwrap();

    let metadata_cap = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            if o.object_type
                .as_ref()
                .map(|t| t.contains("MetadataCap"))
                .unwrap_or(false)
            {
                o.object_id
                    .as_ref()
                    .and_then(|id| ObjectID::from_str(id).ok())
            } else {
                None
            }
        })
        .unwrap();

    (package_id, treasury_cap, metadata_cap)
}

// Helper function to publish the burnonly coin package
async fn publish_burnonly_coin(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
) -> (ObjectID, ObjectID, ObjectID) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "burnonly_coin"]);

    let (package_id_obj, transaction) =
        super::super::publish_package(test_cluster, address, path).await;
    let package_id = package_id_obj;

    // Get treasury cap and metadata cap from changed objects
    let treasury_cap = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            if o.object_type
                .as_ref()
                .map(|t| t.contains("TreasuryCap"))
                .unwrap_or(false)
            {
                o.object_id
                    .as_ref()
                    .and_then(|id| ObjectID::from_str(id).ok())
            } else {
                None
            }
        })
        .unwrap();

    let metadata_cap = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            if o.object_type
                .as_ref()
                .map(|t| t.contains("MetadataCap"))
                .unwrap_or(false)
            {
                o.object_id
                    .as_ref()
                    .and_then(|id| ObjectID::from_str(id).ok())
            } else {
                None
            }
        })
        .unwrap();

    (package_id, treasury_cap, metadata_cap)
}

// Helper function to publish the regulated coin package
async fn publish_regulated_coin(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
) -> (ObjectID, ObjectID, ObjectID, ObjectID) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "regulated_coin"]);

    let (package_id_obj, transaction) =
        super::super::publish_package(test_cluster, address, path).await;
    let package_id = package_id_obj;

    // Get treasury cap, metadata cap, and deny cap from changed objects
    let treasury_cap = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            if o.object_type
                .as_ref()
                .map(|t| t.contains("TreasuryCap"))
                .unwrap_or(false)
            {
                o.object_id
                    .as_ref()
                    .and_then(|id| ObjectID::from_str(id).ok())
            } else {
                None
            }
        })
        .unwrap();

    let metadata_cap = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            if o.object_type
                .as_ref()
                .map(|t| t.contains("MetadataCap"))
                .unwrap_or(false)
            {
                o.object_id
                    .as_ref()
                    .and_then(|id| ObjectID::from_str(id).ok())
            } else {
                None
            }
        })
        .unwrap();

    let deny_cap = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            if o.object_type
                .as_ref()
                .map(|t| t.contains("DenyCapV2"))
                .unwrap_or(false)
            {
                o.object_id
                    .as_ref()
                    .and_then(|id| ObjectID::from_str(id).ok())
            } else {
                None
            }
        })
        .unwrap();

    (package_id, treasury_cap, metadata_cap, deny_cap)
}

// Helper function to publish the legacy coin package
async fn publish_legacy_coin(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
) -> (ObjectID, ObjectID, ObjectID) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "legacy_coin"]);

    let (package_id_obj, transaction) =
        super::super::publish_package(test_cluster, address, path).await;
    let package_id = package_id_obj;

    // Get treasury cap and metadata object from changed objects
    let treasury_cap = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            if o.object_type
                .as_ref()
                .map(|t| t.contains("TreasuryCap"))
                .unwrap_or(false)
            {
                o.object_id
                    .as_ref()
                    .and_then(|id| ObjectID::from_str(id).ok())
            } else {
                None
            }
        })
        .unwrap();

    let metadata_id = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            if o.object_type
                .as_ref()
                .map(|t| t.contains("CoinMetadata"))
                .unwrap_or(false)
            {
                o.object_id
                    .as_ref()
                    .and_then(|id| ObjectID::from_str(id).ok())
            } else {
                None
            }
        })
        .unwrap();

    (package_id, treasury_cap, metadata_id)
}

// Helper function to publish the non-OTW coin package
async fn publish_non_otw_coin(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
) -> (ObjectID, ObjectID, ObjectID) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "non_otw_coin"]);

    let (package_id_obj, _transaction) =
        super::super::publish_package(test_cluster, address, path).await;
    let package_id = package_id_obj;

    // Now call create_currency function to create the coin
    let mut ptb = ProgrammableTransactionBuilder::new();

    // Get the CoinRegistry shared object
    let registry_obj_response = {
        let mut ledger_client = {
            use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
            LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
                .await
                .expect("Failed to connect to ledger service")
        };

        use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
        ledger_client
            .get_object({
                let mut request = GetObjectRequest::default();
                request.object_id = Some(SUI_COIN_REGISTRY_OBJECT_ID.to_string());
                request.read_mask = Some(FieldMask::from_str("version,owner"));
                request
            })
            .await
            .expect("Failed to get CoinRegistry object")
            .into_inner()
    };

    use sui_rpc::proto::sui::rpc::v2::owner::OwnerKind;
    let registry_initial_version = registry_obj_response
        .object
        .and_then(|obj| obj.owner)
        .and_then(|owner| {
            if owner.kind == Some(OwnerKind::Shared as i32) {
                owner
                    .version
                    .map(sui_types::base_types::SequenceNumber::from)
            } else {
                None
            }
        })
        .expect("CoinRegistry should be a shared object with an initial version");

    // Add the CoinRegistry as a mutable shared object
    let registry_arg = ptb
        .obj(ObjectArg::SharedObject {
            id: SUI_COIN_REGISTRY_OBJECT_ID,
            initial_shared_version: registry_initial_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();

    // Call create_currency with the registry
    ptb.programmable_move_call(
        package_id,
        "non_otw_coin".parse().unwrap(),
        "create_currency".parse().unwrap(),
        vec![],
        vec![registry_arg],
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

    // Execute the create_currency transaction
    let create_tx = test_cluster.sign_and_execute_transaction(&tx_data).await;

    // Get treasury cap and metadata cap from created objects
    use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
    let effects = create_tx.effects.as_ref().unwrap();

    let mut treasury_cap = None;
    let mut metadata_cap = None;

    for o in effects.created() {
        let obj = test_cluster
            .get_object_from_fullnode_store(&o.reference.object_id)
            .await
            .unwrap();

        if let Some(type_) = obj.type_() {
            let type_str = type_.to_string();
            if type_str.contains("TreasuryCap") {
                treasury_cap = Some(o.reference.object_id);
            } else if type_str.contains("MetadataCap") {
                metadata_cap = Some(o.reference.object_id);
            }
        }
    }

    let treasury_cap = treasury_cap.expect("TreasuryCap not found");
    let metadata_cap = metadata_cap.expect("MetadataCap not found");

    (package_id, treasury_cap, metadata_cap)
}

// Helper function to finalize registration for CoinRegistry coins
async fn finalize_registration(
    test_cluster: &test_cluster::TestCluster,
    _package_id: ObjectID,
    coin_type: &str,
) {
    use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
    use sui_rpc::proto::sui::rpc::v2::owner::OwnerKind;
    use sui_rpc::proto::sui::rpc::v2::{GetObjectRequest, ListOwnedObjectsRequest};
    use sui_types::base_types::SequenceNumber;

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
            mutability: SharedObjectMutability::Mutable,
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
    let mut client = sui_rpc::Client::new(test_cluster.rpc_url().to_owned()).unwrap();

    let _finalize_tx = super::super::execute_transaction(&mut client, &signed_tx).await;
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

async fn get_grpc_client(
    test_cluster: &test_cluster::TestCluster,
) -> StateServiceClient<tonic::transport::Channel> {
    StateServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap()
}
