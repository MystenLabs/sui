// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_rpc::proto::sui::rpc::v2::coin_treasury::SupplyState as RpcSupplyState;
use sui_rpc::proto::sui::rpc::v2::CoinTreasury;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoResponse;
use sui_sdk_types::{Address, StructTag};
use sui_types::base_types::{ObjectID as SuiObjectID, SuiAddress};
use sui_types::coin::RegulatedCoinMetadata;
use sui_types::coin_registry::{Currency, RegulatedState as CurrencyRegulatedState, SupplyState};
use sui_types::object::Owner::AddressOwner;
use sui_types::object::Owner::ConsensusAddressOwner;
use sui_types::object::Owner::Immutable;
use sui_types::sui_sdk_types_conversions::struct_tag_sdk_to_core;

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
    let currency_id = Currency::derive_object_id(core_coin_type.clone().into()).map_err(|e| {
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
    let metadata = Some((&currency).into());

    let treasury = if sui_types::gas_coin::GAS::is_gas(core_coin_type) {
        Some(SUI_COIN_TREASURY)
    } else {
        match &currency.supply {
            Some(SupplyState::Fixed(supply)) => {
                let mut treasury = CoinTreasury::default();
                treasury.id = currency
                    .treasury_cap_id
                    .map(|id| Address::from(id).to_string());
                treasury.total_supply = Some(*supply);
                treasury.supply_state = Some(RpcSupplyState::Fixed.into());
                Some(treasury)
            }
            Some(SupplyState::BurnOnly(supply)) => {
                let mut treasury = CoinTreasury::default();
                treasury.id = currency
                    .treasury_cap_id
                    .map(|id| Address::from(id).to_string());
                treasury.total_supply = Some(*supply);
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

    // If registry has a definitive state, use it
    let regulated_metadata = if !matches!(&currency.regulated, CurrencyRegulatedState::Unknown) {
        Some((&currency.regulated).into())
    } else {
        // Registry has Unknown state, need to check for legacy RegulatedCoinMetadata
        let indexes = match service.reader.inner().indexes() {
            Some(indexes) => indexes,
            None => {
                // No indexes available, keep as Unknown
                let mut response = GetCoinInfoResponse::default();
                response.coin_type = Some(coin_type.to_string());
                response.metadata = metadata;
                response.treasury = treasury;
                response.regulated_metadata = Some(CurrencyRegulatedState::Unknown.into());
                return Ok(Some(response));
            }
        };

        let coin_info = indexes.get_coin_info(core_coin_type).map_err(|e| {
            RpcError::new(
                tonic::Code::Internal,
                format!("Failed to get coin info for {}: {}", core_coin_type, e),
            )
        })?;

        let regulated_id = coin_info.and_then(|info| info.regulated_coin_metadata_object_id);

        match regulated_id {
            None => {
                // No RegulatedCoinMetadata exists, coin is unregulated
                Some(CurrencyRegulatedState::Unregulated.into())
            }
            Some(id) => object_store
                .get_object(&id)
                .map(RegulatedCoinMetadata::try_from)
                .transpose()
                .map_err(|_| {
                    RpcError::new(
                        tonic::Code::Internal,
                        format!(
                            "Unable to read object {} for coin type {} as RegulatedCoinMetadata",
                            id, core_coin_type
                        ),
                    )
                })?
                .map(Into::into)
                .or(Some(CurrencyRegulatedState::Unregulated.into())),
        }
    };

    let mut response = GetCoinInfoResponse::default();
    response.coin_type = Some(coin_type.to_string());
    response.metadata = metadata;
    response.treasury = treasury;
    response.regulated_metadata = regulated_metadata;
    Ok(Some(response))
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
            .map(Into::into)
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
            .map(RegulatedCoinMetadata::try_from)
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
            .map(Into::into)
    } else {
        Some(CurrencyRegulatedState::Unregulated.into())
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

    // Treasury caps that are immutable, owned by 0x0, or consensus-owned by 0x0 indicate fixed supply
    let supply_state = match &obj.owner {
        Immutable => RpcSupplyState::Fixed,
        AddressOwner(addr) if *addr == SuiAddress::ZERO => RpcSupplyState::Fixed,
        ConsensusAddressOwner { owner, .. } if *owner == SuiAddress::ZERO => RpcSupplyState::Fixed,
        _ => RpcSupplyState::Unknown,
    };

    sui_types::coin::TreasuryCap::try_from(obj)
        .ok()
        .map(|treasury| {
            let mut coin_treasury: CoinTreasury = treasury.into();
            coin_treasury.supply_state = Some(supply_state.into());
            coin_treasury
        })
}
