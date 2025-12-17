// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ErrorReason, Result, RpcError, RpcService};
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{Balance, GetBalanceRequest, GetBalanceResponse};
use sui_sdk_types::StructTag;
use sui_types::base_types::SuiAddress;
use sui_types::sui_sdk_types_conversions::struct_tag_sdk_to_core;

#[tracing::instrument(skip(service))]
pub fn get_balance(service: &RpcService, request: GetBalanceRequest) -> Result<GetBalanceResponse> {
    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?;

    let owner = request
        .owner
        .as_ref()
        .ok_or_else(|| {
            FieldViolation::new("owner")
                .with_description("missing owner")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<SuiAddress>()
        .map_err(|e| {
            FieldViolation::new("owner")
                .with_description(format!("invalid owner: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let coin_type = request
        .coin_type
        .as_ref()
        .ok_or_else(|| {
            FieldViolation::new("coin_type")
                .with_description("missing coin_type")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<StructTag>()
        .map_err(|e| {
            FieldViolation::new("coin_type")
                .with_description(format!("invalid coin_type: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let core_coin_type = struct_tag_sdk_to_core(coin_type.clone())?;

    let balance_info = indexes
        .get_balance(&owner, &core_coin_type)?
        .unwrap_or_default(); // Use default (zero) if no balance found

    let mut balance = Balance::default();
    balance.coin_type = Some(coin_type.to_string());
    balance.balance = Some(balance_info.balance);

    let mut response = GetBalanceResponse::default();
    response.balance = Some(balance);
    Ok(response)
}
