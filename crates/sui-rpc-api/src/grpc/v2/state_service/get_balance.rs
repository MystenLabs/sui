// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ErrorReason, Result, RpcError, RpcService};
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{Balance, GetBalanceRequest, GetBalanceResponse};
use sui_sdk_types::Address;
use sui_sdk_types::StructTag;
use sui_types::base_types::SuiAddress;
use sui_types::storage::BalanceInfo;
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
        .parse::<Address>()
        .map_err(|e| {
            FieldViolation::new("owner")
                .with_description(format!("invalid owner: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
    let owner = SuiAddress::from(owner);

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

    let balance = render_balance(service, owner, core_coin_type, balance_info);

    Ok(GetBalanceResponse::default().with_balance(balance))
}

pub(super) fn render_balance(
    _service: &RpcService,
    _owner: SuiAddress,
    coin_type: move_core_types::language_storage::StructTag,
    balance_info: BalanceInfo,
) -> Balance {
    let mut balance = Balance::default()
        .with_coin_type(coin_type.to_canonical_string(true))
        .with_balance(
            balance_info
                .coin_balance
                .saturating_add(balance_info.address_balance),
        );

    if balance_info.coin_balance != 0 {
        balance.set_coin_balance(balance_info.coin_balance);
    }

    if balance_info.address_balance != 0 {
        balance.set_address_balance(balance_info.address_balance);
    }

    balance
}
