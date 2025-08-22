// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FieldMask;
use std::path::PathBuf;
use std::str::FromStr;
use sui_macros::sim_test;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2::coin_treasury::SupplyState;
use sui_rpc::proto::sui::rpc::v2beta2::live_data_service_client::LiveDataServiceClient;
use sui_rpc::proto::sui::rpc::v2beta2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2beta2::{
    ExecutedTransaction, GetCoinInfoRequest, GetCoinInfoResponse,
};
use sui_sdk_types::TypeTag as SdkTypeTag;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, TransactionData};
use sui_types::{
    parse_sui_struct_tag, TypeTag, SUI_COIN_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
};
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_coin_info_sui() {
    let test_cluster = TestClusterBuilder::new().build().await;

    // SUI's CoinData is created during genesis but needs migrate_receiving to be called
    migrate_receiving(&test_cluster, "0x2::sui::SUI", None).await;

    let mut grpc_client = LiveDataServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let coin_type_sdk: SdkTypeTag = "0x2::sui::SUI".parse().unwrap();
    let request = GetCoinInfoRequest {
        coin_type: Some(coin_type_sdk.to_string()),
    };

    let GetCoinInfoResponse {
        coin_type,
        metadata,
        treasury,
        regulated_metadata,
    } = grpc_client
        .get_coin_info(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(coin_type, Some(coin_type_sdk.to_string()));

    let metadata = metadata.unwrap();

    let metadata_object_id = metadata.id.as_ref().unwrap();
    assert!(ObjectID::from_str(metadata_object_id).is_ok(),);
    assert_eq!(metadata.decimals, Some(9));
    assert_eq!(metadata.symbol, Some("SUI".to_owned()));
    assert_eq!(metadata.name, Some("Sui".to_string()));
    assert_eq!(metadata.description, Some("".to_string()));
    assert!(metadata.icon_url.is_some());
    assert!(metadata.metadata_cap_id.is_none());

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

    assert!(regulated_metadata.is_none());
}

#[sim_test]
async fn test_get_coin_info_registry_coin() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = get_grpc_client(&test_cluster).await;

    let address = test_cluster.get_address_0();

    // Publish coin package using create_currency_v2
    let (package_id, treasury_cap, metadata_cap, publish_tx) =
        publish_registry_coin(&test_cluster, address).await;

    let coin_type = format!("{}::registry_coin::REGISTRY_COIN", package_id);

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

    migrate_receiving(&test_cluster, &coin_type, Some(&publish_tx)).await;

    // Check dynamic fields of CoinRegistry after migrate_receiving
    let _coin_registry_fields = grpc_client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(SuiAddress::from(SUI_COIN_REGISTRY_OBJECT_ID).to_string()),
            read_mask: Some(FieldMask::from_str("object_id,version,object_type")),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    // Now that wait_for_transaction waits for checkpoint inclusion,
    // the coin registry index should be updated
    let response = grpc_client
        .get_coin_info(GetCoinInfoRequest {
            coin_type: Some(coin_type.clone()),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.coin_type, Some(coin_type.clone()));

    let metadata = response.metadata.expect("Expected metadata to be present");
    // Verify metadata.id is a valid object ID
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
    assert!(
        ObjectID::from_str(metadata.metadata_cap_id.as_ref().unwrap()).is_ok(),
        "metadata.metadata_cap_id should be a valid ObjectID"
    );
    assert_eq!(metadata.metadata_cap_id.unwrap(), metadata_cap.to_string());
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

    // No regulated metadata for this coin
    assert!(response.regulated_metadata.is_none());

    // Phase 2: Register the supply (consuming the TreasuryCap) and verify RPC reflects the update
    // Get the CoinRegistry object's initial shared version
    let registry_initial_version = SequenceNumber::from(1);

    // Get the updated treasury cap reference after minting
    let updated_treasury_cap_obj = test_cluster
        .get_object_from_fullnode_store(&treasury_cap)
        .await
        .unwrap();
    let updated_treasury_cap_ref = updated_treasury_cap_obj.compute_object_reference();

    // Build a transaction to register the supply
    let mut ptb = ProgrammableTransactionBuilder::new();

    // Add the CoinRegistry as a mutable shared object
    let registry_arg = ptb
        .obj(ObjectArg::SharedObject {
            id: SUI_COIN_REGISTRY_OBJECT_ID,
            initial_shared_version: registry_initial_version,
            mutable: true,
        })
        .unwrap();

    // Add the treasury cap as an owned object with updated reference
    let treasury_cap_arg = ptb
        .obj(ObjectArg::ImmOrOwnedObject(updated_treasury_cap_ref))
        .unwrap();

    // Call register_supply
    ptb.programmable_move_call(
        package_id,
        "registry_coin".parse().unwrap(),
        "register_supply".parse().unwrap(),
        vec![],
        vec![registry_arg, treasury_cap_arg],
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
        .get_coin_info(GetCoinInfoRequest {
            coin_type: Some(coin_type.clone()),
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

    let (package_id, treasury_cap, metadata_cap, deny_cap, publish_tx) =
        publish_regulated_coin(&test_cluster, address).await;

    let coin_type = format!("{}::regulated_coin::REGULATED_COIN", package_id);

    migrate_receiving(&test_cluster, &coin_type, Some(&publish_tx)).await;

    let response = grpc_client
        .get_coin_info(GetCoinInfoRequest {
            coin_type: Some(coin_type.clone()),
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
    assert!(ObjectID::from_str(metadata.metadata_cap_id.as_ref().unwrap()).is_ok(),);
    assert_eq!(metadata.metadata_cap_id.unwrap(), metadata_cap.to_string());
    assert!(response.treasury.is_some());
    let treasury = response.treasury.unwrap();
    assert!(ObjectID::from_str(treasury.id.as_ref().unwrap()).is_ok(),);
    assert_eq!(treasury.id.unwrap(), treasury_cap.to_string());
    assert_eq!(treasury.total_supply.unwrap(), 0);
    assert_eq!(treasury.supply_state, Some(SupplyState::Unknown as i32),);

    assert!(response.regulated_metadata.is_some());
    let regulated = response.regulated_metadata.unwrap();
    assert!(ObjectID::from_str(regulated.coin_metadata_object.as_ref().unwrap()).is_ok(),);
    assert!(ObjectID::from_str(regulated.deny_cap_object.as_ref().unwrap()).is_ok(),);
    assert_eq!(regulated.deny_cap_object.unwrap(), deny_cap.to_string());
}

#[sim_test]
async fn test_legacy_coin_from_registry() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut grpc_client = get_grpc_client(&test_cluster).await;
    let sender = test_cluster.get_address_0();

    // Publish the legacy coin package that uses create_currency (v1)
    let (package_id, _metadata_id, treasury_cap_id, publish_tx) =
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

    migrate_receiving(&test_cluster, &coin_type, Some(&publish_tx)).await;

    let response = grpc_client
        .get_coin_info(GetCoinInfoRequest {
            coin_type: Some(coin_type.clone()),
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
    assert!(metadata.metadata_cap_id.is_none());

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

    assert!(response.regulated_metadata.is_none());

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
        .get_coin_info(GetCoinInfoRequest {
            coin_type: Some(coin_type.clone()),
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
    let request = GetCoinInfoRequest {
        coin_type: Some("invalid::coin::type::format".to_string()),
    };

    let result = grpc_client.get_coin_info(request).await;
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("invalid coin_type"));

    // Test with non-existent coin type
    let fake_coin_type =
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef::fakecoin::FAKECOIN";
    let request = GetCoinInfoRequest {
        coin_type: Some(fake_coin_type.to_string()),
    };

    let result = grpc_client.get_coin_info(request).await;
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
    assert!(error.message().contains("Coin type"));
    assert!(error.message().contains("not found"));
}

async fn migrate_receiving(
    test_cluster: &test_cluster::TestCluster,
    coin_type: &str,
    _publish_tx: Option<&ExecutedTransaction>,
) {
    let coin_type_tag: sui_types::TypeTag = coin_type.parse().unwrap();

    let registry_initial_version = SequenceNumber::from(1);

    let mut channel = tonic::transport::Channel::from_shared(test_cluster.rpc_url().to_owned())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut grpc_client = LiveDataServiceClient::new(channel.clone());

    let registry_owned = grpc_client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(SuiAddress::from(SUI_COIN_REGISTRY_OBJECT_ID).to_string()),
            read_mask: Some(FieldMask::from_str(
                "object_id,version,digest,object_type,owner",
            )),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    // Parse the target coin type for normalized comparison
    let target_coin_type: TypeTag = coin_type.parse().unwrap();

    let coin_data = registry_owned
        .iter()
        .find(|obj| {
            obj.object_type
                .as_ref()
                .and_then(|t| {
                    // Parse the object type as a struct tag
                    parse_sui_struct_tag(t).ok().and_then(|struct_tag| {
                        // Check if this is a CoinData type and extract the type parameter
                        if struct_tag.module.as_str() == "coin_registry"
                            && struct_tag.name.as_str() == "CoinData"
                            && struct_tag.type_params.len() == 1
                        {
                            // Compare the normalized type parameters
                            struct_tag.type_params.first().map(|type_param| {
                                type_param.to_canonical_string(false)
                                    == target_coin_type.to_canonical_string(false)
                            })
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("CoinData for {} not found in CoinRegistry", coin_type));

    let coin_data_id = ObjectID::from_str(coin_data.object_id.as_ref().unwrap()).unwrap();

    let receiving_coin_data = registry_owned
        .iter()
        .find(|obj| {
            obj.object_id
                .as_ref()
                .map(|id| id.parse::<ObjectID>().ok() == Some(coin_data_id))
                .unwrap_or(false)
        })
        .expect("CoinData not found in CoinRegistry's owned objects");

    let coin_data_object_ref = (
        receiving_coin_data
            .object_id
            .as_ref()
            .unwrap()
            .parse()
            .unwrap(),
        SequenceNumber::from(receiving_coin_data.version.unwrap()),
        receiving_coin_data
            .digest
            .as_ref()
            .unwrap()
            .parse()
            .unwrap(),
    );

    let mut ptb = ProgrammableTransactionBuilder::new();

    let registry_arg = ptb
        .obj(ObjectArg::SharedObject {
            id: SUI_COIN_REGISTRY_OBJECT_ID,
            initial_shared_version: registry_initial_version,
            mutable: true,
        })
        .unwrap();

    let receiving_arg = ptb.obj(ObjectArg::Receiving(coin_data_object_ref)).unwrap();

    ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        "coin_registry".parse().unwrap(),
        "migrate_receiving".parse().unwrap(),
        vec![coin_type_tag],
        vec![registry_arg, receiving_arg],
    );

    let pt = ptb.finish();
    let sender = test_cluster.get_address_0();

    let gas_objects = grpc_client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(sender.to_string()),
            read_mask: Some(FieldMask::from_str("object_id,version,digest,object_type")),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    let gas_object = gas_objects
        .iter()
        .find(|obj| {
            obj.object_type
                .as_ref()
                .map(|t| t.contains("coin::Coin") && t.contains("sui::SUI"))
                .unwrap_or(false)
        })
        .expect("Gas object not found");

    let gas_object_ref = (
        gas_object.object_id.as_ref().unwrap().parse().unwrap(),
        SequenceNumber::from(gas_object.version.unwrap()),
        gas_object.digest.as_ref().unwrap().parse().unwrap(),
    );

    let tx_data =
        TransactionData::new_programmable(sender, vec![gas_object_ref], pt, 50_000_000, 1000);

    let tx_response = test_cluster.sign_and_execute_transaction(&tx_data).await;

    let _checkpoint_tx =
        super::super::wait_for_transaction(&mut channel, &tx_response.digest.to_string()).await;
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
) -> (ObjectID, ObjectID, ObjectID, ExecutedTransaction) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "registry_coin"]);

    let (package_id, transaction) =
        super::super::publish_package(test_cluster, address, path).await;

    // Extract treasury cap and metadata cap from transaction
    let treasury_cap = find_object_by_type(&transaction, "TreasuryCap");
    let metadata_cap = find_object_by_type(&transaction, "MetadataCap");

    (package_id, treasury_cap, metadata_cap, transaction)
}

async fn publish_regulated_coin(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
) -> (ObjectID, ObjectID, ObjectID, ObjectID, ExecutedTransaction) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "regulated_coin"]);

    let (package_id, transaction) =
        super::super::publish_package(test_cluster, address, path).await;

    // Extract caps from transaction
    let treasury_cap = find_object_by_type(&transaction, "TreasuryCap");
    let metadata_cap = find_object_by_type(&transaction, "MetadataCap");
    let deny_cap = find_object_by_type(&transaction, "DenyCapV2");

    (
        package_id,
        treasury_cap,
        metadata_cap,
        deny_cap,
        transaction,
    )
}

async fn publish_legacy_coin(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
) -> (ObjectID, ObjectID, ObjectID, ExecutedTransaction) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "legacy_coin"]);

    let (package_id, transaction) =
        super::super::publish_package(test_cluster, address, path).await;

    // Extract treasury cap and metadata object from transaction
    let treasury_cap = find_object_by_type(&transaction, "TreasuryCap");
    let metadata = find_object_by_type(&transaction, "CoinMetadata");

    (package_id, metadata, treasury_cap, transaction)
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
