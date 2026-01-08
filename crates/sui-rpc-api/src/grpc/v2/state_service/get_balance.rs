// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ErrorReason, Result, RpcError, RpcService};
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{Balance, GetBalanceRequest, GetBalanceResponse};
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

    let balance = render_balance(service, owner, core_coin_type, balance_info);

    Ok(GetBalanceResponse::default().with_balance(balance))
}

pub(super) fn render_balance(
    service: &RpcService,
    owner: SuiAddress,
    coin_type: move_core_types::language_storage::StructTag,
    balance_info: BalanceInfo,
) -> Balance {
    let mut balance = Balance::default()
        .with_coin_type(coin_type.to_canonical_string(true))
        .with_balance(balance_info.balance);

    if balance_info.balance == 0 {
        return balance;
    }

    // When looking up an Address's balance for a particular coin type, there is a possibility that
    // the value we read is "newer" (further ahead in time) than the summed balance we've read from
    // the indexes.
    //
    // This inconsistency should in practice be hard to see (as its a race) but it can lead to
    // slightly inconsistent responses:
    //
    // - If the Address balance is greater than it was at the checkpoint corrisponding to our index
    // read, then we'll clamp the value returned to what was in the indexes and its possible that
    // we under report the balance stored in `Coin<T>`s by saying that the address has no coin
    // balance.
    //
    // - If the Address balance is less than it was at the checkpoint corrisponding to our index
    // read, then we can possibly over-inflate the balance stored in `Coin<T>`s.
    if let Some(address_balance) = service.reader.lookup_address_balance(owner, coin_type) {
        balance.set_address_balance(address_balance);

        match address_balance.cmp(&balance_info.balance) {
            std::cmp::Ordering::Less => {
                // If the AddressBalance is less than the total balance we read from the indexes,
                // then the difference is attributed to coins
                balance.set_coin_balance(balance_info.balance.saturating_sub(address_balance));
            }
            std::cmp::Ordering::Equal => {}
            std::cmp::Ordering::Greater => {
                // There is a potential race where the Address balance we read is newer than what
                // we read from the indexes, and if its higher then lets just cap it based on what
                // we have in the indexes.
                balance.set_address_balance(balance_info.balance);
            }
        }
    } else {
        // If there is no AddressBalance for this coin type, then all the balance is attributed to
        // coins
        balance.set_coin_balance(balance_info.balance);
    }

    balance
}
