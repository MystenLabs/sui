// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use move_core_types::language_storage::StructTag as CoreStructTag;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Balance;
use sui_rpc::proto::sui::rpc::v2::BatchGetBalancesRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetBalancesResponse;
use sui_rpc::proto::sui::rpc::v2::GetBalanceRequest;
use sui_rpc::proto::sui::rpc::v2::GetBalanceResponse;
use sui_sdk_types::Address;
use sui_sdk_types::StructTag;
use sui_types::base_types::SuiAddress;
use sui_types::storage::BalanceInfo;
use sui_types::storage::RpcIndexes;
use sui_types::sui_sdk_types_conversions::struct_tag_sdk_to_core;

use crate::ErrorReason;
use crate::Result;
use crate::RpcService;
use crate::reader::indexes_disabled;

use super::checkpoint::PinSource;
use super::checkpoint::resolve_state_read;

/// Maximum number of balance lookups accepted in a single `BatchGetBalances` call.
pub const MAX_BATCH_REQUESTS: usize = 1000;

struct ParsedBalanceRequest {
    owner: SuiAddress,
    coin_type: CoreStructTag,
    at_checkpoint: Option<u64>,
}

fn parse_balance_request(
    request: &GetBalanceRequest,
    field_prefix: &str,
) -> Result<ParsedBalanceRequest> {
    let owner_field = format!("{field_prefix}owner");
    let owner = request
        .owner
        .as_ref()
        .ok_or_else(|| {
            FieldViolation::new(&owner_field)
                .with_description("missing owner")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<Address>()
        .map_err(|e| {
            FieldViolation::new(&owner_field)
                .with_description(format!("invalid owner: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
    let owner = SuiAddress::from(owner);

    let coin_type_field = format!("{field_prefix}coin_type");
    let coin_type = request
        .coin_type
        .as_ref()
        .ok_or_else(|| {
            FieldViolation::new(&coin_type_field)
                .with_description("missing coin_type")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<StructTag>()
        .map_err(|e| {
            FieldViolation::new(&coin_type_field)
                .with_description(format!("invalid coin_type: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let core_coin_type = struct_tag_sdk_to_core(coin_type)?;

    Ok(ParsedBalanceRequest {
        owner,
        coin_type: core_coin_type,
        at_checkpoint: request.at_checkpoint,
    })
}

#[tracing::instrument(skip(service))]
pub fn get_balance(service: &RpcService, request: GetBalanceRequest) -> Result<GetBalanceResponse> {
    service
        .reader
        .inner()
        .indexes()
        .ok_or_else(indexes_disabled)?;

    let parsed = parse_balance_request(&request, "")?;

    let target = resolve_state_read(
        service,
        parsed.at_checkpoint,
        parsed.at_checkpoint.is_some(),
        PinSource::AtCheckpoint,
        false,
    )?;

    let balance_info = target
        .indexes()
        .get_balance(&parsed.owner, &parsed.coin_type)?
        .unwrap_or_default();

    let balance = render_balance(service, parsed.owner, parsed.coin_type, balance_info);

    Ok(GetBalanceResponse::default().with_balance(balance))
}

#[tracing::instrument(skip(service))]
pub fn batch_get_balances(
    service: &RpcService,
    request: BatchGetBalancesRequest,
) -> Result<BatchGetBalancesResponse> {
    let reader = service.reader.inner();
    let current_indexes = reader.indexes().ok_or_else(indexes_disabled)?;

    if request.requests.len() > MAX_BATCH_REQUESTS {
        return Err(FieldViolation::new("requests")
            .with_description(format!(
                "number of batch requests exceeds limit of {MAX_BATCH_REQUESTS}"
            ))
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }

    let parsed: Vec<ParsedBalanceRequest> = request
        .requests
        .iter()
        .enumerate()
        .map(|(i, item)| parse_balance_request(item, &format!("requests[{i}].")))
        .collect::<Result<_>>()?;

    // Acquire each distinct checkpoint snapshot once so all items sharing a checkpoint read the
    // exact same consistent view and the snapshot cannot be evicted mid-batch.
    let mut snapshots: HashMap<u64, Arc<dyn RpcIndexes>> = HashMap::new();
    for item in &parsed {
        if let Some(cp) = item.at_checkpoint
            && !snapshots.contains_key(&cp)
        {
            let pinned = super::checkpoint::resolve_pinned(
                reader.as_ref(),
                Some(cp),
                PinSource::AtCheckpoint,
                false,
            )?;
            snapshots.insert(cp, pinned.indexes);
        }
    }

    let mut balances = Vec::with_capacity(parsed.len());
    for item in parsed {
        let indexes = match item.at_checkpoint {
            Some(cp) => snapshots[&cp].as_ref(),
            None => current_indexes,
        };
        let balance_info = indexes
            .get_balance(&item.owner, &item.coin_type)?
            .unwrap_or_default();
        balances.push(render_balance(
            service,
            item.owner,
            item.coin_type,
            balance_info,
        ));
    }

    let mut response = BatchGetBalancesResponse::default();
    response.balances = balances;
    Ok(response)
}

pub(super) fn render_balance(
    _service: &RpcService,
    _owner: SuiAddress,
    coin_type: CoreStructTag,
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
