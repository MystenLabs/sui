// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use crate::RpcError;
use crate::RpcService;
use dynamic_field::{DOFWrapper, Field};
use sui_rpc::proto::sui::rpc::v2beta2::coin_treasury::SupplyState;
use sui_rpc::proto::sui::rpc::v2beta2::CoinMetadata;
use sui_rpc::proto::sui::rpc::v2beta2::CoinTreasury;
use sui_rpc::proto::sui::rpc::v2beta2::GetCoinInfoRequest;
use sui_rpc::proto::sui::rpc::v2beta2::GetCoinInfoResponse;
use sui_rpc::proto::sui::rpc::v2beta2::RegulatedCoinMetadata;
use sui_sdk_types::{Address, StructTag};
use sui_types::base_types::{ObjectID as SuiObjectID, SuiAddress};
use sui_types::coin_registry::CoinDataKey;
use sui_types::coin_registry::COIN_DATA_KEY_STRUCT_NAME;
use sui_types::coin_registry::COIN_REGISTRY_MODULE_NAME;
use sui_types::coin_registry::{self};
use sui_types::dynamic_field::DYNAMIC_OBJECT_FIELD_MODULE_NAME;
use sui_types::dynamic_field::DYNAMIC_OBJECT_FIELD_WRAPPER_STRUCT_NAME;
use sui_types::dynamic_field::{self};
use sui_types::sui_sdk_types_conversions::struct_tag_sdk_to_core;
use sui_types::{TypeTag, SUI_COIN_REGISTRY_OBJECT_ID};

const SUI_COIN_TREASURY: CoinTreasury = CoinTreasury {
    id: None,
    total_supply: Some(sui_types::gas_coin::TOTAL_SUPPLY_MIST),
    supply_state: Some(SupplyState::Fixed as i32),
};

#[derive(Debug)]
pub struct CoinNotFoundError(StructTag);

impl std::fmt::Display for CoinNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Coin type {} not found", self.0)
    }
}

impl std::error::Error for CoinNotFoundError {}

impl From<CoinNotFoundError> for crate::RpcError {
    fn from(value: CoinNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}

#[tracing::instrument(skip(service))]
pub fn get_coin_info(
    service: &RpcService,
    request: GetCoinInfoRequest,
) -> Result<GetCoinInfoResponse> {
    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?;

    let coin_type = request.coin_type().parse::<StructTag>().map_err(|e| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            format!("invalid coin_type: {e}"),
        )
    })?;

    let core_coin_type = struct_tag_sdk_to_core(coin_type.clone())?;

    match get_coin_info_from_registry(service, indexes, &coin_type, &core_coin_type) {
        Ok(Some(response)) => Ok(response),
        Ok(None) => get_coin_info_from_index(service, indexes, &coin_type, &core_coin_type)?
            .ok_or_else(|| CoinNotFoundError(coin_type).into()),
        Err(e) => Err(e),
    }
}

fn get_coin_info_from_registry(
    service: &RpcService,
    indexes: &dyn sui_types::storage::RpcIndexes,
    coin_type: &StructTag,
    core_coin_type: &move_core_types::language_storage::StructTag,
) -> Result<Option<GetCoinInfoResponse>> {
    // For dynamic object fields, the key type is Wrapper<CoinDataKey<T>>
    let coin_data_key_type = move_core_types::language_storage::StructTag {
        address: move_core_types::account_address::AccountAddress::from_hex_literal("0x2").unwrap(),
        module: move_core_types::identifier::Identifier::new(COIN_REGISTRY_MODULE_NAME.as_str())
            .unwrap(),
        name: move_core_types::identifier::Identifier::new(COIN_DATA_KEY_STRUCT_NAME.as_str())
            .unwrap(),
        type_params: vec![TypeTag::Struct(Box::new(core_coin_type.clone()))],
    };

    let wrapper_type_tag =
        TypeTag::Struct(Box::new(move_core_types::language_storage::StructTag {
            address: move_core_types::account_address::AccountAddress::from_hex_literal("0x2")
                .unwrap(),
            module: move_core_types::identifier::Identifier::new(
                DYNAMIC_OBJECT_FIELD_MODULE_NAME.as_str(),
            )
            .unwrap(),
            name: move_core_types::identifier::Identifier::new(
                DYNAMIC_OBJECT_FIELD_WRAPPER_STRUCT_NAME.as_str(),
            )
            .unwrap(),
            type_params: vec![TypeTag::Struct(Box::new(coin_data_key_type))],
        }));

    let coin_data_key_bytes = bcs::to_bytes(&CoinDataKey::new()).map_err(|e| {
        RpcError::new(
            tonic::Code::Internal,
            format!("Failed to serialize CoinDataKey: {}", e),
        )
    })?;

    let field_id = dynamic_field::derive_dynamic_field_id(
        SUI_COIN_REGISTRY_OBJECT_ID,
        &wrapper_type_tag,
        &coin_data_key_bytes,
    )
    .map_err(|e| {
        RpcError::new(
            tonic::Code::Internal,
            format!(
                "Failed to derive dynamic field ID for coin type {}: {}",
                core_coin_type, e
            ),
        )
    })?;

    let object_store = service.reader.inner();
    let field_obj = match object_store.get_object(&field_id) {
        Some(obj) => obj,
        None => return Ok(None), // Coin not registered in CoinRegistry
    };

    let move_obj = field_obj.data.try_as_move().ok_or_else(|| {
        RpcError::new(
            tonic::Code::Internal,
            format!(
                "Dynamic field for coin type {} is not a Move object",
                core_coin_type
            ),
        )
    })?;

    // For dynamic object fields containing CoinDataKey, we have:
    // Field<DOFWrapper<CoinDataKey<T>>, ObjectID>
    // Since CoinDataKey is an empty struct, DOFWrapper<CoinDataKey<T>> serializes to just [0x00]
    let field: Field<DOFWrapper<[u8; 1]>, SuiObjectID> = bcs::from_bytes(move_obj.contents())
        .map_err(|e| {
            RpcError::new(
                tonic::Code::Internal,
                format!(
                    "Failed to deserialize dynamic field for coin type {}: {}",
                    core_coin_type, e
                ),
            )
        })?;

    let coin_data_obj = object_store.get_object(&field.value).ok_or_else(|| {
        RpcError::new(
            tonic::Code::Internal,
            format!(
                "CoinData object {} for coin type {} not found",
                field.value, core_coin_type
            ),
        )
    })?;

    let coin_data_move_obj = coin_data_obj.data.try_as_move().ok_or_else(|| {
        RpcError::new(
            tonic::Code::Internal,
            format!(
                "CoinData for coin type {} is not a Move object",
                core_coin_type
            ),
        )
    })?;

    let coin_data = bcs::from_bytes::<coin_registry::CoinData>(coin_data_move_obj.contents())
        .map_err(|e| {
            RpcError::new(
                tonic::Code::Internal,
                format!(
                    "Failed to deserialize CoinData for coin type {}: {}",
                    core_coin_type, e
                ),
            )
        })?;
    let metadata = Some(CoinMetadata {
        id: Some(Address::from(coin_data.id.id.bytes).to_string()),
        decimals: Some(coin_data.decimals.into()),
        name: Some(coin_data.name),
        symbol: Some(coin_data.symbol),
        description: Some(coin_data.description),
        icon_url: Some(coin_data.icon_url),
        metadata_cap_id: coin_data
            .metadata_cap_id
            .map(|id| Address::from(id).to_string()),
    });

    let treasury = if sui_types::gas_coin::GAS::is_gas(core_coin_type) {
        Some(SUI_COIN_TREASURY)
    } else {
        match &coin_data.supply {
            Some(coin_registry::SupplyState::Fixed(supply)) => Some(CoinTreasury {
                id: coin_data
                    .treasury_cap_id
                    .map(|id| Address::from(id).to_string()),
                total_supply: Some(supply.value),
                supply_state: Some(SupplyState::Fixed.into()),
            }),
            _ => {
                // For unknown supply state, look up the treasury cap object
                let treasury_cap_id = coin_data.treasury_cap_id.or_else(|| {
                    // Fall back to legacy index lookup. This can happen if
                    // coin::register_supply has not yet been called
                    indexes
                        .get_coin_info(core_coin_type)
                        .ok()
                        .flatten()
                        .and_then(|info| info.treasury_object_id)
                });
                treasury_cap_id.and_then(|id| get_treasury_cap_info(service, id))
            }
        }
    };

    let regulated_metadata = match &coin_data.regulated {
        coin_registry::RegulatedState::Regulated { cap, .. } => {
            Some(RegulatedCoinMetadata {
                id: None, // No separate RegulatedCoinMetadata object in CoinRegistry
                coin_metadata_object: Some(Address::from(coin_data.id.id.bytes).to_string()),
                deny_cap_object: Some(Address::from(*cap).to_string()),
            })
        }
        coin_registry::RegulatedState::Unknown => None,
    };

    Ok(Some(GetCoinInfoResponse {
        coin_type: Some(coin_type.to_string()),
        metadata,
        treasury,
        regulated_metadata,
    }))
}

fn get_coin_info_from_index(
    service: &RpcService,
    indexes: &dyn sui_types::storage::RpcIndexes,
    coin_type: &StructTag,
    core_coin_type: &move_core_types::language_storage::StructTag,
) -> Result<Option<GetCoinInfoResponse>> {
    let coin_info = match indexes.get_coin_info(core_coin_type)? {
        Some(info) => info,
        None => return Ok(None),
    };
    let metadata = if let Some(coin_metadata_object_id) = coin_info.coin_metadata_object_id {
        service
            .reader
            .inner()
            .get_object(&coin_metadata_object_id)
            .map(sui_types::coin::CoinMetadata::try_from)
            .transpose()
            .map_err(|_| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!(
                        "Unable to read object {} for coin type {} as CoinMetadata",
                        coin_metadata_object_id, core_coin_type
                    ),
                )
            })?
            .map(|value| CoinMetadata {
                id: Some(Address::from(value.id.id.bytes).to_string()),
                decimals: Some(value.decimals.into()),
                name: Some(value.name),
                symbol: Some(value.symbol),
                description: Some(value.description),
                icon_url: value.icon_url,
                metadata_cap_id: None,
            })
    } else {
        None
    };

    let treasury = if let Some(treasury_object_id) = coin_info.treasury_object_id {
        get_treasury_cap_info(service, treasury_object_id)
    } else if sui_types::gas_coin::GAS::is_gas(core_coin_type) {
        Some(SUI_COIN_TREASURY)
    } else {
        None
    };

    let regulated_metadata = if let Some(regulated_coin_metadata_object_id) =
        coin_info.regulated_coin_metadata_object_id
    {
        service
            .reader
            .inner()
            .get_object(&regulated_coin_metadata_object_id)
            .map(sui_types::coin::RegulatedCoinMetadata::try_from)
            .transpose()
            .map_err(|_| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!(
                        "Unable to read object {} for coin type {} as CoinMetadata",
                        regulated_coin_metadata_object_id, core_coin_type
                    ),
                )
            })?
            .map(|value| RegulatedCoinMetadata {
                id: Some(Address::from(value.id.id.bytes).to_string()),
                coin_metadata_object: Some(
                    Address::from(value.coin_metadata_object.bytes).to_string(),
                ),
                deny_cap_object: Some(Address::from(value.deny_cap_object.bytes).to_string()),
            })
    } else {
        None
    };

    Ok(Some(GetCoinInfoResponse {
        coin_type: Some(coin_type.to_string()),
        metadata,
        treasury,
        regulated_metadata,
    }))
}

fn get_treasury_cap_info(
    service: &RpcService,
    treasury_object_id: SuiObjectID,
) -> Option<CoinTreasury> {
    let obj = service.reader.inner().get_object(&treasury_object_id)?;

    // Treasury caps owned by 0x0 indicate fixed supply
    let supply_state = if obj.owner == sui_types::object::Owner::AddressOwner(SuiAddress::ZERO) {
        SupplyState::Fixed
    } else {
        SupplyState::Unknown
    };

    sui_types::coin::TreasuryCap::try_from(obj)
        .ok()
        .map(|treasury| CoinTreasury {
            id: Some(Address::from(treasury.id.id.bytes).to_string()),
            total_supply: Some(treasury.total_supply.value),
            supply_state: Some(supply_state.into()),
        })
}
