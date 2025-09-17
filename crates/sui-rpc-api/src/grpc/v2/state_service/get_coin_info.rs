// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_rpc::proto::sui::rpc::v2::coin_metadata::MetadataCapState as ProtoMetadataCapState;
use sui_rpc::proto::sui::rpc::v2::coin_treasury::SupplyState as RpcSupplyState;
use sui_rpc::proto::sui::rpc::v2::regulated_coin_metadata::CoinRegulatedState as ProtoCoinRegulatedState;
use sui_rpc::proto::sui::rpc::v2::CoinMetadata;
use sui_rpc::proto::sui::rpc::v2::CoinTreasury;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoResponse;
use sui_rpc::proto::sui::rpc::v2::RegulatedCoinMetadata;
use sui_sdk_types::{Address, StructTag};
use sui_types::base_types::{ObjectID as SuiObjectID, SuiAddress};
use sui_types::coin_registry::{
    self, Currency, CurrencyKey, CurrencyRegulatedState, MetadataCapState, SupplyState,
};
use sui_types::derived_object;
use sui_types::sui_sdk_types_conversions::struct_tag_sdk_to_core;
use sui_types::{TypeTag, SUI_COIN_REGISTRY_OBJECT_ID};

const SUI_COIN_TREASURY: CoinTreasury = {
    let mut treasury = CoinTreasury::const_default();
    treasury.total_supply = Some(sui_types::gas_coin::TOTAL_SUPPLY_MIST);
    treasury.supply_state = Some(RpcSupplyState::Fixed as i32);
    treasury
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

    match get_coin_info_from_registry(service, &coin_type, &core_coin_type) {
        Ok(Some(response)) => Ok(response),
        Ok(None) => get_coin_info_from_index(service, indexes, &coin_type, &core_coin_type)?
            .ok_or_else(|| CoinNotFoundError(coin_type).into()),
        Err(e) => Err(e),
    }
}

fn get_coin_info_from_registry(
    service: &RpcService,
    coin_type: &StructTag,
    core_coin_type: &move_core_types::language_storage::StructTag,
) -> Result<Option<GetCoinInfoResponse>> {
    let currency_key_type = move_core_types::language_storage::StructTag {
        address: move_core_types::account_address::AccountAddress::from_hex_literal("0x2").unwrap(),
        module: move_core_types::identifier::Identifier::new(
            coin_registry::COIN_REGISTRY_MODULE_NAME.as_str(),
        )
        .unwrap(),
        name: move_core_types::identifier::Identifier::new(
            coin_registry::CURRENCY_KEY_STRUCT_NAME.as_str(),
        )
        .unwrap(),
        type_params: vec![TypeTag::Struct(Box::new(core_coin_type.clone()))],
    };

    let currency_key_bytes = bcs::to_bytes(&CurrencyKey::new()).map_err(|e| {
        RpcError::new(
            tonic::Code::Internal,
            format!("Failed to serialize CurrencyKey: {}", e),
        )
    })?;

    let currency_id = derived_object::derive_object_id(
        SUI_COIN_REGISTRY_OBJECT_ID,
        &TypeTag::Struct(Box::new(currency_key_type.clone())),
        &currency_key_bytes,
    )
    .map_err(|e| {
        RpcError::new(
            tonic::Code::Internal,
            format!(
                "Failed to derive Currency ID for coin type {}: {}",
                core_coin_type, e
            ),
        )
    })?;

    let object_store = service.reader.inner();
    let currency_obj = match object_store.get_object(&currency_id) {
        Some(obj) => obj,
        None => {
            return Ok(None); // Coin not registered in CoinRegistry
        }
    };

    let move_obj = currency_obj.data.try_as_move().ok_or_else(|| {
        RpcError::new(
            tonic::Code::Internal,
            format!(
                "Currency for coin type {} is not a Move object",
                core_coin_type
            ),
        )
    })?;

    let currency = bcs::from_bytes::<Currency>(move_obj.contents()).map_err(|e| {
        RpcError::new(
            tonic::Code::Internal,
            format!(
                "Failed to deserialize Currency for coin type {}: {}",
                core_coin_type, e
            ),
        )
    })?;
    let metadata = {
        let mut metadata = CoinMetadata::default();
        metadata.id = Some(Address::from(currency.id.id.bytes).to_string());
        metadata.decimals = Some(currency.decimals.into());
        metadata.name = Some(currency.name);
        metadata.symbol = Some(currency.symbol);
        metadata.description = Some(currency.description);
        metadata.icon_url = Some(currency.icon_url);
        match &currency.metadata_cap_id {
            MetadataCapState::Claimed(id) => {
                metadata.metadata_cap_state = Some(ProtoMetadataCapState::Claimed as i32);
                metadata.metadata_cap_id = Some(Address::from(*id).to_string());
            }
            MetadataCapState::Unclaimed => {
                metadata.metadata_cap_state = Some(ProtoMetadataCapState::Unclaimed as i32);
            }
            MetadataCapState::Deleted => {
                metadata.metadata_cap_state = Some(ProtoMetadataCapState::Deleted as i32);
            }
        }
        Some(metadata)
    };

    let treasury = if sui_types::gas_coin::GAS::is_gas(core_coin_type) {
        Some(SUI_COIN_TREASURY)
    } else {
        match &currency.supply {
            Some(SupplyState::Fixed(supply)) => {
                let mut treasury = CoinTreasury::default();
                treasury.id = currency
                    .treasury_cap_id
                    .map(|id| Address::from(id).to_string());
                treasury.total_supply = Some(supply.value);
                treasury.supply_state = Some(RpcSupplyState::Fixed.into());
                Some(treasury)
            }
            Some(SupplyState::BurnOnly(supply)) => {
                let mut treasury = CoinTreasury::default();
                treasury.id = currency
                    .treasury_cap_id
                    .map(|id| Address::from(id).to_string());
                treasury.total_supply = Some(supply.value);
                treasury.supply_state = Some(RpcSupplyState::BurnOnly.into());
                Some(treasury)
            }
            _ => {
                // For unknown supply state, look up the treasury cap object
                currency
                    .treasury_cap_id
                    .and_then(|id| get_treasury_cap_info(service, id))
            }
        }
    };

    let regulated_metadata = match &currency.regulated {
        CurrencyRegulatedState::Regulated {
            cap,
            allow_global_pause,
            variant,
        } => {
            let mut regulated = RegulatedCoinMetadata::default();
            regulated.id = None;
            regulated.coin_metadata_object = None;
            regulated.deny_cap_object = Some(Address::from(*cap).to_string());
            regulated.allow_global_pause = *allow_global_pause;
            regulated.variant = Some(*variant as u32);
            regulated.coin_regulated_state = Some(ProtoCoinRegulatedState::Regulated as i32);
            Some(regulated)
        }
        CurrencyRegulatedState::Unregulated => {
            let mut regulated = RegulatedCoinMetadata::default();
            regulated.coin_regulated_state = Some(ProtoCoinRegulatedState::Unregulated as i32);
            Some(regulated)
        }
        CurrencyRegulatedState::Unknown => {
            let mut regulated = RegulatedCoinMetadata::default();
            regulated.coin_regulated_state = Some(ProtoCoinRegulatedState::Unknown as i32);
            Some(regulated)
        }
    };

    {
        let mut response = GetCoinInfoResponse::default();
        response.coin_type = Some(coin_type.to_string());
        response.metadata = metadata;
        response.treasury = treasury;
        response.regulated_metadata = regulated_metadata;
        Ok(Some(response))
    }
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
            .map(|value| {
                let mut metadata = CoinMetadata::default();
                metadata.id = Some(Address::from(value.id.id.bytes).to_string());
                metadata.decimals = Some(value.decimals.into());
                metadata.name = Some(value.name);
                metadata.symbol = Some(value.symbol);
                metadata.description = Some(value.description);
                metadata.icon_url = value.icon_url;
                metadata
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
                        "Unable to read object {} for coin type {} as RegulatedCoinMetadata",
                        regulated_coin_metadata_object_id, core_coin_type
                    ),
                )
            })?
            .map(|value| {
                let mut message = RegulatedCoinMetadata::default();
                message.id = Some(Address::from(value.id.id.bytes).to_string());
                message.coin_metadata_object =
                    Some(Address::from(value.coin_metadata_object.bytes).to_string());
                message.deny_cap_object =
                    Some(Address::from(value.deny_cap_object.bytes).to_string());
                message.coin_regulated_state = Some(ProtoCoinRegulatedState::Regulated as i32);
                message
            })
    } else {
        let mut message = RegulatedCoinMetadata::default();
        message.coin_regulated_state = Some(ProtoCoinRegulatedState::Unknown as i32);
        Some(message)
    };

    {
        let mut response = GetCoinInfoResponse::default();
        response.coin_type = Some(coin_type.to_string());
        response.metadata = metadata;
        response.treasury = treasury;
        response.regulated_metadata = regulated_metadata;
        Ok(Some(response))
    }
}

fn get_treasury_cap_info(
    service: &RpcService,
    treasury_object_id: SuiObjectID,
) -> Option<CoinTreasury> {
    let obj = service.reader.inner().get_object(&treasury_object_id)?;

    // Treasury caps owned by 0x0 indicate fixed supply
    let supply_state = if obj.owner == sui_types::object::Owner::AddressOwner(SuiAddress::ZERO) {
        RpcSupplyState::Fixed
    } else {
        RpcSupplyState::Unknown
    };

    sui_types::coin::TreasuryCap::try_from(obj)
        .ok()
        .map(|treasury| {
            let mut coin_treasury = CoinTreasury::default();
            coin_treasury.id = Some(Address::from(treasury.id.id.bytes).to_string());
            coin_treasury.total_supply = Some(treasury.total_supply.value);
            coin_treasury.supply_state = Some(supply_state.into());
            coin_treasury
        })
}
