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

#[cfg(test)]
mod tests {
    use sui_rpc::proto::sui::rpc::v2::BatchGetBalancesRequest;
    use sui_rpc::proto::sui::rpc::v2::GetBalanceRequest;

    use super::*;
    use crate::grpc::v2::state_service::test_support::TestStateStore;
    use crate::grpc::v2::state_service::test_support::struct_tag;
    use crate::grpc::v2::state_service::test_support::test_address;
    use crate::ledger_history::query_options::assert_above_latest_available_error;
    use crate::ledger_history::query_options::assert_below_serving_floor_error;
    use crate::reader::NotServedHereCause;
    use crate::reader::assert_not_served_here;

    fn req(owner: SuiAddress, coin_type: &str, at_checkpoint: Option<u64>) -> GetBalanceRequest {
        let mut r = GetBalanceRequest::default();
        r.owner = Some(owner.to_string());
        r.coin_type = Some(coin_type.to_owned());
        r.at_checkpoint = at_checkpoint;
        r
    }

    #[test]
    fn get_balance_at_checkpoint_reads_snapshot_not_current_state() {
        let (_dir, store, service) = TestStateStore::open(5);
        let owner = test_address(1);
        let sui = struct_tag("0x2::sui::SUI");

        store.add_balance(owner, sui.clone(), 100, 25);
        store.take_snapshot(1);
        store.add_balance(owner, sui.clone(), 50, 10);
        store.take_snapshot(2);
        // Current state advances past latest snapshot.
        store.add_balance(owner, sui, 1_000, 0);

        let at_1 = get_balance(&service, req(owner, "0x2::sui::SUI", Some(1)))
            .unwrap()
            .balance
            .unwrap();
        assert_eq!(at_1.balance, Some(125));
        assert_eq!(at_1.coin_balance, Some(100));
        assert_eq!(at_1.address_balance, Some(25));

        let at_2 = get_balance(&service, req(owner, "0x2::sui::SUI", Some(2)))
            .unwrap()
            .balance
            .unwrap();
        assert_eq!(at_2.balance, Some(185));

        let current = get_balance(&service, req(owner, "0x2::sui::SUI", None))
            .unwrap()
            .balance
            .unwrap();
        assert_eq!(current.balance, Some(1_185));
    }

    #[test]
    fn batch_get_balances_mixed_checkpoints_zero_fill_and_refusals() {
        let (_dir, store, service) = TestStateStore::open(2);
        let alice = test_address(1);
        let bob = test_address(2);
        let sui = struct_tag("0x2::sui::SUI");
        let usdc = struct_tag("0xa::usdc::USDC");

        store.add_balance(alice, sui.clone(), 10, 0);
        store.take_snapshot(1);
        store.add_balance(alice, sui.clone(), 20, 5);
        store.take_snapshot(2);
        store.add_balance(alice, sui.clone(), 30, 0);
        store.take_snapshot(3); // evicts snapshot 1; retained range is 2..=3
        store.add_balance(alice, sui, 40, 0); // current state = 105

        let mut batch = BatchGetBalancesRequest::default();
        batch.requests = vec![
            req(alice, "0x2::sui::SUI", Some(2)),
            req(bob, "0x2::sui::SUI", Some(2)),
            req(alice, "0x2::sui::SUI", Some(3)),
            req(alice, "0x2::sui::SUI", None),
            req(alice, &usdc.to_canonical_string(true), Some(3)),
        ];
        let resp = batch_get_balances(&service, batch).unwrap();
        let totals: Vec<u64> = resp.balances.iter().map(|b| b.balance.unwrap()).collect();
        assert_eq!(totals, vec![35, 0, 65, 105, 0]);

        // Accepts 200+ items.
        let mut two_hundred = BatchGetBalancesRequest::default();
        two_hundred.requests = vec![req(alice, "0x2::sui::SUI", Some(3)); 250];
        assert_eq!(
            batch_get_balances(&service, two_hundred)
                .unwrap()
                .balances
                .len(),
            250
        );

        // Rejects batches above MAX_BATCH_REQUESTS.
        let mut too_many = BatchGetBalancesRequest::default();
        too_many.requests = vec![req(alice, "0x2::sui::SUI", None); MAX_BATCH_REQUESTS + 1];
        assert_eq!(
            batch_get_balances(&service, too_many).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );

        // Any per-item refusal fails the whole batch.
        let mut below = BatchGetBalancesRequest::default();
        below.requests = vec![
            req(alice, "0x2::sui::SUI", Some(3)),
            req(alice, "0x2::sui::SUI", Some(1)),
        ];
        assert_below_serving_floor_error(batch_get_balances(&service, below).unwrap_err(), 2);

        let mut above = BatchGetBalancesRequest::default();
        above.requests = vec![
            req(alice, "0x2::sui::SUI", Some(3)),
            req(alice, "0x2::sui::SUI", Some(4)),
        ];
        assert_above_latest_available_error(
            batch_get_balances(&service, above).unwrap_err(),
            "at_checkpoint",
            4,
            4,
        );

        // Malformed item fails with INVALID_ARGUMENT even if another item is out of range.
        let mut malformed = BatchGetBalancesRequest::default();
        malformed.requests = vec![
            req(alice, "0x2::sui::SUI", Some(99)),
            GetBalanceRequest::default(),
        ];
        assert_eq!(
            batch_get_balances(&service, malformed).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );

        // Window 0 refuses pinned item with NOT_SERVED_HERE (disabled_by_config).
        let (_dir0, _store0, service0) = TestStateStore::open(0);
        let mut disabled = BatchGetBalancesRequest::default();
        disabled.requests = vec![req(alice, "0x2::sui::SUI", Some(1))];
        assert_not_served_here(
            batch_get_balances(&service0, disabled).unwrap_err(),
            NotServedHereCause::DisabledByConfig,
        );
    }
}
