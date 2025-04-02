// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::rpc::v2alpha::CoinMetadata;
use crate::proto::rpc::v2alpha::CoinTreasury;
use crate::proto::rpc::v2alpha::GetCoinInfoRequest;
use crate::proto::rpc::v2alpha::GetCoinInfoResponse;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_sdk_types::{ObjectId, StructTag};
use sui_types::sui_sdk_types_conversions::struct_tag_sdk_to_core;

const SUI_COIN_TREASURY: CoinTreasury = CoinTreasury {
    id: None,
    total_supply: Some(sui_types::gas_coin::TOTAL_SUPPLY_MIST),
};

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

    let sui_types::storage::CoinInfo {
        coin_metadata_object_id,
        treasury_object_id,
    } = indexes
        .get_coin_info(&core_coin_type)?
        .ok_or_else(|| CoinNotFoundError(coin_type.clone()))?;

    let metadata = if let Some(coin_metadata_object_id) = coin_metadata_object_id {
        service.reader
            .inner()
            .get_object(&coin_metadata_object_id)
            .map(sui_types::coin::CoinMetadata::try_from)
            .transpose()
            .map_err(|_| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("Unable to read object {coin_metadata_object_id} for coin type {core_coin_type} as CoinMetadata"),
                )
            })?
            .map(|value| CoinMetadata {
                id: Some(ObjectId::from(value.id.id.bytes).to_string()),
                decimals: Some(value.decimals.into()),
                name: Some(value.name),
                symbol: Some(value.symbol),
                description: Some(value.description),
                icon_url: value.icon_url,
            })
    } else {
        None
    };

    let treasury = if let Some(treasury_object_id) = treasury_object_id {
        service.reader
            .inner()
            .get_object(&treasury_object_id)
            .map(sui_types::coin::TreasuryCap::try_from)
            .transpose()
            .map_err(|_| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("Unable to read object {treasury_object_id} for coin type {core_coin_type} as TreasuryCap"),
                )
            })?
            .map(|treasury| CoinTreasury {
                id: Some(ObjectId::from(treasury.id.id.bytes).to_string()),
                total_supply: Some(treasury.total_supply.value),
            })
    } else if sui_types::gas_coin::GAS::is_gas(&core_coin_type) {
        Some(SUI_COIN_TREASURY)
    } else {
        None
    };

    Ok(GetCoinInfoResponse {
        coin_type: Some(coin_type.to_string()),
        metadata,
        treasury,
    })
}

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
