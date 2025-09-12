// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_rpc::proto::sui::rpc::v2::CoinMetadata;
use sui_rpc::proto::sui::rpc::v2::CoinTreasury;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoResponse;
use sui_rpc::proto::sui::rpc::v2::RegulatedCoinMetadata;
use sui_sdk_types::{Address, StructTag};
use sui_types::sui_sdk_types_conversions::struct_tag_sdk_to_core;

const SUI_COIN_TREASURY: CoinTreasury = {
    let mut treasury = CoinTreasury::const_default();
    treasury.total_supply = Some(sui_types::gas_coin::TOTAL_SUPPLY_MIST);
    treasury
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
        regulated_coin_metadata_object_id,
    } = indexes
        .get_coin_info(&core_coin_type)?
        .ok_or_else(|| CoinNotFoundError(coin_type.clone()))?;

    let metadata = if let Some(coin_metadata_object_id) = coin_metadata_object_id {
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

    let treasury = if let Some(treasury_object_id) = treasury_object_id {
        service
            .reader
            .inner()
            .get_object(&treasury_object_id)
            .map(sui_types::coin::TreasuryCap::try_from)
            .transpose()
            .map_err(|_| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!(
                        "Unable to read object {} for coin type {} as TreasuryCap",
                        treasury_object_id, core_coin_type
                    ),
                )
            })?
            .map(|treasury| {
                let mut message = CoinTreasury::default();
                message.id = Some(Address::from(treasury.id.id.bytes).to_string());
                message.total_supply = Some(treasury.total_supply.value);
                message
            })
    } else if sui_types::gas_coin::GAS::is_gas(&core_coin_type) {
        Some(SUI_COIN_TREASURY)
    } else {
        None
    };

    let regulated_metadata =
        if let Some(regulated_coin_metadata_object_id) = regulated_coin_metadata_object_id {
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
                .map(|value| {
                    let mut message = RegulatedCoinMetadata::default();
                    message.id = Some(Address::from(value.id.id.bytes).to_string());
                    message.coin_metadata_object =
                        Some(Address::from(value.coin_metadata_object.bytes).to_string());
                    message.deny_cap_object =
                        Some(Address::from(value.deny_cap_object.bytes).to_string());
                    message
                })
        } else {
            None
        };

    let mut response = GetCoinInfoResponse::default();
    response.coin_type = Some(coin_type.to_string());
    response.metadata = metadata;
    response.treasury = treasury;
    response.regulated_metadata = regulated_metadata;
    Ok(response)
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
