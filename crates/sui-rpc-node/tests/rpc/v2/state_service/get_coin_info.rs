// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ports
//! `sui-e2e-tests/tests/rpc/v2/state_service/get_coin_info.rs`.
//! The original file is built around `TestClusterBuilder` helpers
//! like `cluster.sign_and_execute_transaction` and
//! `cluster.get_object_from_fullnode_store`. We replace both with
//! the [`LocalCluster`] equivalents (Simulacrum-backed
//! `execute_transaction` and the in-process store lookup).

use std::path::PathBuf;
use std::str::FromStr;

use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoResponse;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::coin_metadata::MetadataCapState;
use sui_rpc::proto::sui::rpc::v2::coin_treasury::SupplyState;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::regulated_coin_metadata::CoinRegulatedState;
use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;
use sui_types::SUI_COIN_REGISTRY_OBJECT_ID;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::TypeTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::coin_registry::Currency;
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::SharedObjectMutability;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

async fn state_client(cluster: &LocalCluster) -> StateServiceClient<Channel> {
    StateServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

fn data_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sui-e2e-tests")
        .join("tests")
        .join("rpc")
        .join("data")
        .join(name)
}

#[tokio::test]
async fn get_coin_info_sui() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut client = state_client(&cluster).await;

    let coin_type_sdk: TypeTag = "0x2::sui::SUI".parse().unwrap();
    let mut request = GetCoinInfoRequest::default();
    request.coin_type = Some(coin_type_sdk.to_string());

    let GetCoinInfoResponse {
        coin_type,
        metadata,
        treasury,
        regulated_metadata,
        ..
    } = client.get_coin_info(request).await.unwrap().into_inner();

    let expected_type = coin_type_sdk.to_canonical_string(true);
    assert_eq!(coin_type, Some(expected_type));

    let metadata = metadata.unwrap();
    let metadata_object_id = metadata.id.as_ref().unwrap();
    assert!(ObjectID::from_str(metadata_object_id).is_ok());
    assert_eq!(metadata.decimals, Some(9));
    assert_eq!(metadata.symbol.as_deref(), Some("SUI"));
    assert_eq!(metadata.name.as_deref(), Some("Sui"));
    assert_eq!(metadata.description.as_deref(), Some(""));
    assert!(metadata.icon_url.is_none());
    assert!(metadata.metadata_cap_state.is_none());

    let treasury = treasury.unwrap();
    assert!(treasury.id.is_none());
    assert_eq!(
        treasury.total_supply,
        Some(sui_types::gas_coin::TOTAL_SUPPLY_MIST),
    );
    assert_eq!(
        treasury.supply_state,
        Some(SupplyState::Fixed as i32),
        "SUI should have Fixed supply state",
    );

    let regulated_metadata = regulated_metadata.unwrap();
    assert_eq!(
        regulated_metadata.coin_regulated_state,
        Some(CoinRegulatedState::Unregulated as i32),
    );
}

#[tokio::test]
async fn invalid_coin_type() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut client = state_client(&cluster).await;

    // Malformed type tag.
    let mut request = GetCoinInfoRequest::default();
    request.coin_type = Some("invalid::coin::type::format".to_string());
    let err = client.get_coin_info(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("invalid coin_type"));

    // Well-formed type but no coin of that type exists.
    let mut request = GetCoinInfoRequest::default();
    request.coin_type = Some(
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef::fakecoin::FAKECOIN"
            .to_string(),
    );
    let err = client.get_coin_info(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
    assert!(err.message().contains("Coin type"));
    assert!(err.message().contains("not found"));
}

#[tokio::test]
async fn legacy_coin_metadata_and_treasury() {
    let cluster = LocalCluster::new().await.unwrap();
    let (sender, keypair, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let (package_id, effects) = cluster
        .publish_package(sender, &keypair, gas, data_path("legacy_coin"))
        .await
        .unwrap();
    cluster.create_checkpoint().await.unwrap();

    let coin_type = format!("{}::legacy_coin::LEGACY_COIN", package_id);
    let metadata_id = find_object_by_type(&cluster, &effects, |t| {
        t.to_canonical_string(true)
            .contains("::coin::CoinMetadata<")
    })
    .await
    .0;

    let mut client = state_client(&cluster).await;
    let response = client
        .get_coin_info({
            let mut req = GetCoinInfoRequest::default();
            req.coin_type = Some(coin_type.clone());
            req
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.coin_type, Some(coin_type.clone()));
    let metadata = response.metadata.expect("metadata present");
    assert_eq!(metadata.id.unwrap(), metadata_id.to_string());
    assert_eq!(metadata.decimals, Some(8));
    assert_eq!(metadata.name.as_deref(), Some("Legacy Coin"));
    assert_eq!(metadata.symbol.as_deref(), Some("LEGACY"));
    assert_eq!(
        metadata.description.as_deref(),
        Some("Legacy coin for testing GetCoinInfo fallback"),
    );
    assert_eq!(
        metadata.icon_url.as_deref(),
        Some("https://example.com/legacy.png"),
    );
    assert!(
        metadata.metadata_cap_state.is_none(),
        "legacy coins predate the metadata cap concept",
    );

    let treasury = response.treasury.unwrap();
    assert_eq!(treasury.total_supply.unwrap(), 0);
    assert_eq!(
        treasury.supply_state,
        Some(SupplyState::Unknown as i32),
        "legacy coins report Unknown supply unless the TreasuryCap lives at 0x0",
    );
}

#[tokio::test]
async fn registry_coin_with_minted_supply() {
    let cluster = LocalCluster::new().await.unwrap();
    let (sender, keypair, gas) = cluster.funded_account(100_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let (package_id, publish_effects) = cluster
        .publish_package(sender, &keypair, gas, data_path("registry_coin"))
        .await
        .unwrap();
    cluster.create_checkpoint().await.unwrap();
    let rgp = cluster.reference_gas_price().await;
    let mut gas_ref = publish_effects.gas_object().unwrap().0;

    let treasury_cap = find_object_by_type(&cluster, &publish_effects, |t| {
        t.to_canonical_string(true).contains("::coin::TreasuryCap<")
    })
    .await;
    let metadata_cap = find_object_by_type(&cluster, &publish_effects, |t| {
        t.to_canonical_string(true).contains("::MetadataCap<")
    })
    .await
    .0;

    let coin_type = format!("{}::registry_coin::REGISTRY_COIN", package_id);

    // Finalize registration so the CoinRegistry sees the new
    // currency.
    gas_ref = finalize_registration(&cluster, sender, &keypair, gas_ref, &coin_type, rgp).await;

    // Mint coins — total_supply ends up at `mint_amount`, but the
    // treasury cap still belongs to `sender` so the supply state
    // is `Unknown`.
    let mint_amount = 5_000_000u64;
    let mut builder = ProgrammableTransactionBuilder::new();
    let cap_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(treasury_cap))
        .unwrap();
    let amount_arg = builder.pure(mint_amount).unwrap();
    let recipient_arg = builder.pure(sender).unwrap();
    builder.programmable_move_call(
        package_id,
        "registry_coin".parse().unwrap(),
        "mint".parse().unwrap(),
        vec![],
        vec![cap_arg, amount_arg, recipient_arg],
    );
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(sender, vec![gas_ref], pt, 50_000_000, rgp),
        &keypair,
    );
    let (mint_effects, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_none(), "mint must succeed: {err:?}");
    gas_ref = mint_effects.gas_object().unwrap().0;
    let new_treasury_cap = mint_effects
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| id == &treasury_cap.0)
        .map(|(oref, _)| oref)
        .expect("mint must mutate the treasury cap");
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;
    let response = client
        .get_coin_info({
            let mut req = GetCoinInfoRequest::default();
            req.coin_type = Some(coin_type.clone());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.coin_type, Some(coin_type.clone()));

    let metadata = response.metadata.expect("metadata present");
    assert_eq!(metadata.decimals, Some(6));
    assert_eq!(metadata.symbol.as_deref(), Some("REGISTRY"));
    assert_eq!(metadata.name.as_deref(), Some("Registry Coin"));
    assert_eq!(
        metadata.description.as_deref(),
        Some("Registry coin for testing GetCoinInfo with CoinRegistry"),
    );
    assert_eq!(
        metadata.icon_url.as_deref(),
        Some("https://example.com/registry.png"),
    );
    assert_eq!(
        metadata.metadata_cap_state,
        Some(MetadataCapState::Claimed as i32),
    );
    assert_eq!(
        metadata.metadata_cap_id.as_deref(),
        Some(metadata_cap.to_string().as_str())
    );

    let treasury = response.treasury.unwrap();
    assert_eq!(treasury.total_supply, Some(mint_amount));
    assert_eq!(
        treasury.supply_state,
        Some(SupplyState::Unknown as i32),
        "TreasuryCap owned by the sender => Unknown supply",
    );
    let regulated = response.regulated_metadata.unwrap();
    assert_eq!(
        regulated.coin_regulated_state,
        Some(CoinRegulatedState::Unregulated as i32),
    );

    // Register the supply — consumes the TreasuryCap and flips
    // the supply state to Fixed.
    let coin_type_tag = move_core_types::language_storage::StructTag {
        address: package_id.into(),
        module: move_core_types::identifier::Identifier::new("registry_coin").unwrap(),
        name: move_core_types::identifier::Identifier::new("REGISTRY_COIN").unwrap(),
        type_params: vec![],
    };
    let currency_id = Currency::derive_object_id(coin_type_tag.into()).unwrap();
    let currency_initial_version = shared_initial_version(&cluster, currency_id).await;

    let mut builder = ProgrammableTransactionBuilder::new();
    let currency_arg = builder
        .obj(ObjectArg::SharedObject {
            id: currency_id,
            initial_shared_version: currency_initial_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let cap_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(new_treasury_cap))
        .unwrap();
    builder.programmable_move_call(
        package_id,
        "registry_coin".parse().unwrap(),
        "register_supply".parse().unwrap(),
        vec![],
        vec![currency_arg, cap_arg],
    );
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(sender, vec![gas_ref], pt, 50_000_000, rgp),
        &keypair,
    );
    let (_, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_none(), "register_supply must succeed: {err:?}");
    cluster.create_checkpoint().await.unwrap();

    let response = client
        .get_coin_info({
            let mut req = GetCoinInfoRequest::default();
            req.coin_type = Some(coin_type.clone());
            req
        })
        .await
        .unwrap()
        .into_inner();
    let treasury = response.treasury.unwrap();
    assert_eq!(treasury.total_supply, Some(mint_amount));
    assert_eq!(
        treasury.supply_state,
        Some(SupplyState::Fixed as i32),
        "After register_supply the treasury becomes Fixed",
    );
}

#[tokio::test]
async fn burnonly_coin_after_register_burnonly_supply() {
    let cluster = LocalCluster::new().await.unwrap();
    let (sender, keypair, gas) = cluster.funded_account(100_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let (package_id, publish_effects) = cluster
        .publish_package(sender, &keypair, gas, data_path("burnonly_coin"))
        .await
        .unwrap();
    cluster.create_checkpoint().await.unwrap();
    let rgp = cluster.reference_gas_price().await;
    let mut gas_ref = publish_effects.gas_object().unwrap().0;

    let treasury_cap = find_object_by_type(&cluster, &publish_effects, |t| {
        t.to_canonical_string(true).contains("::coin::TreasuryCap<")
    })
    .await;

    let coin_type = format!("{}::burnonly_coin::BURNONLY_COIN", package_id);
    gas_ref = finalize_registration(&cluster, sender, &keypair, gas_ref, &coin_type, rgp).await;

    // Mint, then register supply as burn-only.
    let mint_amount = 10_000_000u64;
    let mut builder = ProgrammableTransactionBuilder::new();
    let cap_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(treasury_cap))
        .unwrap();
    let amount_arg = builder.pure(mint_amount).unwrap();
    let recipient_arg = builder.pure(sender).unwrap();
    builder.programmable_move_call(
        package_id,
        "burnonly_coin".parse().unwrap(),
        "mint".parse().unwrap(),
        vec![],
        vec![cap_arg, amount_arg, recipient_arg],
    );
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(sender, vec![gas_ref], pt, 50_000_000, rgp),
        &keypair,
    );
    let (mint_effects, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_none(), "mint must succeed: {err:?}");
    gas_ref = mint_effects.gas_object().unwrap().0;
    let new_treasury_cap = mint_effects
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| id == &treasury_cap.0)
        .map(|(oref, _)| oref)
        .expect("mint must mutate the treasury cap");
    cluster.create_checkpoint().await.unwrap();

    let coin_type_tag = move_core_types::language_storage::StructTag {
        address: package_id.into(),
        module: move_core_types::identifier::Identifier::new("burnonly_coin").unwrap(),
        name: move_core_types::identifier::Identifier::new("BURNONLY_COIN").unwrap(),
        type_params: vec![],
    };
    let currency_id = Currency::derive_object_id(coin_type_tag.into()).unwrap();
    let currency_initial_version = shared_initial_version(&cluster, currency_id).await;

    let mut builder = ProgrammableTransactionBuilder::new();
    let currency_arg = builder
        .obj(ObjectArg::SharedObject {
            id: currency_id,
            initial_shared_version: currency_initial_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let cap_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(new_treasury_cap))
        .unwrap();
    builder.programmable_move_call(
        package_id,
        "burnonly_coin".parse().unwrap(),
        "register_supply_as_burnonly".parse().unwrap(),
        vec![],
        vec![currency_arg, cap_arg],
    );
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(sender, vec![gas_ref], pt, 50_000_000, rgp),
        &keypair,
    );
    let (_, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(
        err.is_none(),
        "register_supply_as_burnonly must succeed: {err:?}"
    );
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;
    let response = client
        .get_coin_info({
            let mut req = GetCoinInfoRequest::default();
            req.coin_type = Some(coin_type.clone());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.coin_type, Some(coin_type));

    let metadata = response.metadata.expect("metadata present");
    assert_eq!(metadata.decimals, Some(9));
    assert_eq!(metadata.name.as_deref(), Some("BurnOnly Coin"));
    assert_eq!(metadata.symbol.as_deref(), Some("BURNONLY"));

    let treasury = response.treasury.unwrap();
    assert_eq!(treasury.total_supply, Some(mint_amount));
    assert_eq!(treasury.supply_state, Some(SupplyState::BurnOnly as i32),);
}

#[tokio::test]
async fn regulated_coin_reports_regulated_metadata() {
    let cluster = LocalCluster::new().await.unwrap();
    let (sender, keypair, gas) = cluster.funded_account(100_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let (package_id, publish_effects) = cluster
        .publish_package(sender, &keypair, gas, data_path("regulated_coin"))
        .await
        .unwrap();
    cluster.create_checkpoint().await.unwrap();
    let rgp = cluster.reference_gas_price().await;
    let gas_ref = publish_effects.gas_object().unwrap().0;

    let deny_cap = find_object_by_type(&cluster, &publish_effects, |t| {
        t.to_canonical_string(true).contains("::coin::DenyCapV2<")
    })
    .await
    .0;

    let coin_type = format!("{}::regulated_coin::REGULATED_COIN", package_id);
    let _ = finalize_registration(&cluster, sender, &keypair, gas_ref, &coin_type, rgp).await;

    let mut client = state_client(&cluster).await;
    let response = client
        .get_coin_info({
            let mut req = GetCoinInfoRequest::default();
            req.coin_type = Some(coin_type.clone());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.coin_type, Some(coin_type));

    let metadata = response.metadata.expect("metadata present");
    assert_eq!(metadata.decimals, Some(9));
    assert_eq!(metadata.name.as_deref(), Some("Regulated Coin"));
    assert_eq!(metadata.symbol.as_deref(), Some("REG"));

    let regulated = response
        .regulated_metadata
        .expect("regulated metadata present");
    assert_eq!(
        regulated.coin_regulated_state,
        Some(CoinRegulatedState::Regulated as i32),
    );
    assert!(regulated.id.is_none());
    assert!(regulated.coin_metadata_object.is_none());
    assert_eq!(regulated.deny_cap_object.unwrap(), deny_cap.to_string());
}

#[tokio::test]
async fn non_otw_coin_after_create_currency() {
    let cluster = LocalCluster::new().await.unwrap();
    let (sender, keypair, gas) = cluster.funded_account(100_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let (package_id, publish_effects) = cluster
        .publish_package(sender, &keypair, gas, data_path("non_otw_coin"))
        .await
        .unwrap();
    cluster.create_checkpoint().await.unwrap();
    let rgp = cluster.reference_gas_price().await;
    let gas_ref = publish_effects.gas_object().unwrap().0;

    // The non-OTW package defers currency creation to an explicit
    // `create_currency` call against the CoinRegistry.
    let registry_initial_version =
        shared_initial_version(&cluster, SUI_COIN_REGISTRY_OBJECT_ID).await;
    let mut builder = ProgrammableTransactionBuilder::new();
    let registry_arg = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_COIN_REGISTRY_OBJECT_ID,
            initial_shared_version: registry_initial_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    builder.programmable_move_call(
        package_id,
        "non_otw_coin".parse().unwrap(),
        "create_currency".parse().unwrap(),
        vec![],
        vec![registry_arg],
    );
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(sender, vec![gas_ref], pt, 50_000_000, rgp),
        &keypair,
    );
    let (create_effects, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_none(), "create_currency must succeed: {err:?}");
    cluster.create_checkpoint().await.unwrap();

    // The non-OTW coin module uses `b"MyCoin"` as the type
    // parameter name (see `data/non_otw_coin/sources`).
    let coin_type = format!("{}::non_otw_coin::MyCoin", package_id);

    let metadata_cap = find_object_by_type(&cluster, &create_effects, |t| {
        t.to_canonical_string(true).contains("::MetadataCap<")
    })
    .await
    .0;

    let mut client = state_client(&cluster).await;
    let response = client
        .get_coin_info({
            let mut req = GetCoinInfoRequest::default();
            req.coin_type = Some(coin_type.clone());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.coin_type, Some(coin_type));

    let metadata = response.metadata.expect("metadata present");
    assert_eq!(metadata.decimals, Some(7));
    assert_eq!(metadata.name.as_deref(), Some("Non-OTW Coin"));
    assert_eq!(metadata.symbol.as_deref(), Some("NONOTW"));
    assert_eq!(
        metadata.description.as_deref(),
        Some("Non-OTW coin for testing GetCoinInfo with new_currency (without OTW)"),
    );
    assert_eq!(
        metadata.icon_url.as_deref(),
        Some("https://example.com/non_otw.png"),
    );
    assert_eq!(
        metadata.metadata_cap_state,
        Some(MetadataCapState::Claimed as i32),
    );
    assert_eq!(
        metadata.metadata_cap_id.as_deref(),
        Some(metadata_cap.to_string().as_str())
    );

    let treasury = response.treasury.expect("treasury present");
    assert_eq!(treasury.total_supply.unwrap(), 0);
    assert_eq!(treasury.supply_state, Some(SupplyState::Unknown as i32));
}

/// Read the CoinRegistry's `Currency<T>` row for `coin_type` and
/// call `0x2::coin_registry::finalize_registration` so the
/// registry materialises a stable `Currency<T>` at its derived
/// address. Returns the post-finalize gas object ref.
async fn finalize_registration(
    cluster: &LocalCluster,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas: ObjectRef,
    coin_type: &str,
    rgp: u64,
) -> ObjectRef {
    let target_type: TypeTag = coin_type.parse().unwrap();
    let registry_initial_version =
        shared_initial_version(cluster, SUI_COIN_REGISTRY_OBJECT_ID).await;

    // The CoinRegistry stores a temporary `Currency<T>` as an
    // address-owned object under the registry address — find it
    // via the rpc-api's `list_owned_objects`.
    let mut ledger = LedgerServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap();
    let mut state = state_client(cluster).await;
    let owned = state
        .list_owned_objects({
            let mut req = ListOwnedObjectsRequest::default();
            req.owner = Some(SuiAddress::from(SUI_COIN_REGISTRY_OBJECT_ID).to_string());
            req.read_mask = Some(FieldMask::from_str(
                "object_id,version,digest,object_type,owner",
            ));
            req
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    let currency = owned
        .iter()
        .find(|obj| {
            let Some(obj_type) = obj.object_type.as_deref() else {
                return false;
            };
            match sui_types::parse_sui_struct_tag(obj_type) {
                Ok(tag) => {
                    tag.module.as_str() == "coin_registry"
                        && tag.name.as_str() == "Currency"
                        && tag.type_params.len() == 1
                        && tag.type_params[0].to_canonical_string(false)
                            == target_type.to_canonical_string(false)
                }
                Err(_) => false,
            }
        })
        .unwrap_or_else(|| panic!("Currency<{}> not yet in CoinRegistry", coin_type));

    let currency_ref: ObjectRef = (
        currency.object_id.as_ref().unwrap().parse().unwrap(),
        SequenceNumber::from(currency.version.unwrap()),
        currency.digest.as_ref().unwrap().parse().unwrap(),
    );

    // Confirm the resolved currency_ref is consistent with what
    // the ledger reports (sanity check that the indexer caught
    // up).
    let _ = ledger
        .get_object(GetObjectRequest::default().with_object_id(currency_ref.0.to_string()))
        .await
        .unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    let registry_arg = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_COIN_REGISTRY_OBJECT_ID,
            initial_shared_version: registry_initial_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let receiving_arg = builder.obj(ObjectArg::Receiving(currency_ref)).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        "coin_registry".parse().unwrap(),
        "finalize_registration".parse().unwrap(),
        vec![target_type],
        vec![registry_arg, receiving_arg],
    );
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(sender, vec![gas], pt, 50_000_000, rgp),
        keypair,
    );
    let (effects, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_none(), "finalize_registration must succeed: {err:?}");
    let gas_ref = effects.gas_object().unwrap().0;
    cluster.create_checkpoint().await.unwrap();
    gas_ref
}

async fn shared_initial_version(cluster: &LocalCluster, id: ObjectID) -> SequenceNumber {
    let obj = cluster
        .get_object(id)
        .await
        .unwrap_or_else(|| panic!("expected object {id} to exist"));
    match obj.owner {
        Owner::Shared {
            initial_shared_version,
        } => initial_shared_version,
        _ => panic!("expected {id} to be a shared object, got {:?}", obj.owner),
    }
}

async fn find_object_by_type(
    cluster: &LocalCluster,
    effects: &TransactionEffects,
    pred: impl Fn(&sui_types::base_types::MoveObjectType) -> bool,
) -> ObjectRef {
    for (oref, _) in effects.created().into_iter().chain(effects.mutated()) {
        let Some(obj) = cluster.get_object(oref.0).await else {
            continue;
        };
        if obj.type_().map(&pred).unwrap_or(false) {
            return oref;
        }
    }
    panic!("no created/mutated object matched the type predicate");
}
