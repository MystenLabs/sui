// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    proto::rpc::v2alpha::{Balance, GetBalanceRequest, GetBalanceResponse},
    Result, RpcError, RpcService,
};
use sui_sdk_types::StructTag;
use sui_types::base_types::SuiAddress;
use sui_types::sui_sdk_types_conversions::struct_tag_sdk_to_core;

const DEFAULT_COIN_TYPE: &str = "0x2::sui::SUI";

#[tracing::instrument(skip(service))]
pub fn get_balance(service: &RpcService, request: GetBalanceRequest) -> Result<GetBalanceResponse> {
    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?;

    // Parse owner address
    let owner_str = request.owner().trim();
    if owner_str.is_empty() {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            "owner address is required",
        ));
    }

    let owner = owner_str.parse::<SuiAddress>().map_err(|e| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            format!("invalid owner address: {e}"),
        )
    })?;

    // Parse coin type (default to SUI if not provided)
    let coin_type_str = request.coin_type.as_deref().unwrap_or(DEFAULT_COIN_TYPE);
    let coin_type = coin_type_str.parse::<StructTag>().map_err(|e| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            format!("invalid coin_type: {e}"),
        )
    })?;

    let core_coin_type = struct_tag_sdk_to_core(coin_type.clone())?;

    // Get balance from indexes
    let balance_info = indexes
        .get_balance(&owner, &core_coin_type)?
        .unwrap_or_default(); // Use default (zero) if no balance found

    Ok(GetBalanceResponse {
        balance: Some(Balance {
            coin_type: Some(coin_type.to_string()),
            total_balance: Some(balance_info.balance),
        }),
    })
}
