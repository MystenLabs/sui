// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Bound;

use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use crate::reader::indexes_disabled;
use bytes::Bytes;
use move_core_types::language_storage::StructTag;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::ListBalancesRequest;
use sui_rpc::proto::sui::rpc::v2::ListBalancesResponse;
use sui_rpc::proto::sui::rpc::v2::QueryEnd;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_sdk_types::Address;
use sui_types::base_types::SuiAddress;
use sui_types::storage::RpcIndexes;

use super::checkpoint::resolve_pinned;
use super::cursor::StateCursor;
use super::cursor::StateCursorKey;
use super::cursor::decode_page_token;
use super::cursor::parse_query_options;
use super::cursor::reject_legacy_paging_with_options;
use super::cursor::wrong_cursor_scope;
const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE: usize = 1000;

#[tracing::instrument(skip(service))]
pub fn list_balances(
    service: &RpcService,
    request: ListBalancesRequest,
) -> Result<ListBalancesResponse> {
    let reader = service.reader.inner();
    let current_indexes = reader.indexes().ok_or_else(indexes_disabled)?;

    let owner: Address = request
        .owner
        .as_ref()
        .ok_or_else(|| {
            FieldViolation::new("owner")
                .with_description("missing owner")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse()
        .map_err(|e| {
            FieldViolation::new("owner")
                .with_description(format!("invalid owner: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
    let owner_sui = SuiAddress::from(owner);

    let extract_key = |field: &str, key: StateCursorKey| match key {
        StateCursorKey::Balances {
            owner: cur_owner,
            coin_type,
        } if cur_owner == owner_sui => Ok(coin_type),
        _ => Err(wrong_cursor_scope(
            field,
            "cursor does not match this ListBalances owner",
        )),
    };

    let (pin, lower, upper, descending, limit, exhausted_end_reason) =
        if let Some(options) = request.options.as_ref() {
            reject_legacy_paging_with_options(request.page_size, request.page_token.as_deref())?;
            let opts = parse_query_options(
                Some(options),
                request.at_checkpoint,
                DEFAULT_PAGE_SIZE,
                MAX_PAGE_SIZE,
                extract_key,
            )?;
            let (lower, upper) = opts.bounds();
            (
                Some(opts.pin),
                lower,
                upper,
                opts.descending,
                opts.limit,
                Some(opts.exhausted_end_reason()),
            )
        } else {
            let page_size = request
                .page_size
                .map(|s| (s as usize).clamp(1, MAX_PAGE_SIZE))
                .unwrap_or(DEFAULT_PAGE_SIZE);
            let (pin, lower) = decode_page_token::<LegacyPageToken, StructTag, _, _, _>(
                request.page_token.as_deref(),
                request.at_checkpoint,
                |t| t.owner == owner,
                |t| t.coin_type,
                extract_key,
            )?;
            (pin, lower, Bound::Unbounded, false, page_size, None)
        };

    let pinned = pin
        .map(|pin| resolve_pinned(reader.as_ref(), pin, false))
        .transpose()?;
    let indexes: &dyn RpcIndexes = pinned
        .as_ref()
        .map_or(current_indexes, |p| p.indexes.as_ref());

    let take_count = if exhausted_end_reason.is_some() {
        limit
    } else {
        limit + 1
    };
    let mut rows = indexes
        .balance_iter(&owner_sui, lower, upper, descending)?
        .take(take_count)
        .map(|result| result.map_err(|err| RpcError::new(tonic::Code::Internal, err.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    let next_row =
        (exhausted_end_reason.is_none() && rows.len() > limit).then(|| rows.pop().unwrap());
    let hit_limit = rows.len() == limit;

    let mut response = ListBalancesResponse::default();
    response.balances = Vec::with_capacity(rows.len());
    if exhausted_end_reason.is_some() {
        response.cursors = Vec::with_capacity(rows.len());
    }
    let mut last_coin_type = None;
    for (coin_type, balance_info) in rows {
        response
            .balances
            .push(super::get_balance::render_balance(&coin_type, balance_info));
        if let Some(pinned) = &pinned {
            if exhausted_end_reason.is_some() {
                response.cursors.push(
                    StateCursor {
                        checkpoint: pinned.checkpoint,
                        key: StateCursorKey::Balances {
                            owner: owner_sui,
                            coin_type,
                        },
                    }
                    .encode(),
                );
            } else {
                last_coin_type = Some(coin_type);
            }
        }
    }

    if let Some(exhausted_reason) = exhausted_end_reason {
        let end_reason = if hit_limit {
            QueryEndReason::ItemLimit
        } else {
            exhausted_reason
        };
        response.end = Some(QueryEnd::default().with_reason(end_reason));
    } else if let Some((next_coin_type, _)) = next_row {
        response.next_page_token = Some(match &pinned {
            Some(pinned) => StateCursor {
                checkpoint: pinned.checkpoint,
                key: StateCursorKey::Balances {
                    owner: owner_sui,
                    coin_type: last_coin_type.expect("limit >= 1"),
                },
            }
            .encode(),
            None => encode_legacy_page_token(LegacyPageToken {
                owner,
                coin_type: next_coin_type,
            }),
        });
    }

    Ok(response)
}

fn encode_legacy_page_token(page_token: LegacyPageToken) -> Bytes {
    bcs::to_bytes(&page_token).unwrap().into()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct LegacyPageToken {
    owner: Address,
    coin_type: StructTag,
}

#[cfg(test)]
mod tests {
    use sui_rpc::proto::sui::rpc::v2::Ordering;
    use sui_rpc::proto::sui::rpc::v2::QueryOptions;

    use super::*;
    use crate::grpc::v2::state_service::test_support::TestStateStore;
    use crate::grpc::v2::state_service::test_support::struct_tag;
    use crate::grpc::v2::state_service::test_support::test_address;

    #[test]
    fn cursor_paging_directions_bounds_and_checkpoint_pinning() {
        let (_dir, store, service) = TestStateStore::open(5);
        let owner = test_address(1);
        for (name, amt) in [("A", 10), ("B", 20), ("C", 30), ("D", 40)] {
            store.add_balance(owner, struct_tag(&format!("0x2::c::{name}")), amt, 0);
        }
        store.take_snapshot(10);
        // Mutate live state after snapshot 10 so page 1 must read snapshot 10 rather than live state.
        store.add_balance(owner, struct_tag("0x2::c::A"), 1_000, 0);

        // Page 1 with `options` and no `at_checkpoint` pins to newest snapshot (10).
        let mut opts = QueryOptions::default();
        opts.limit = Some(2);
        let mut req1 = ListBalancesRequest::default();
        req1.owner = Some(owner.to_string());
        req1.options = Some(opts);
        let page1 = list_balances(&service, req1).unwrap();
        assert!(page1.next_page_token.is_none());
        assert_eq!(page1.balances.len(), 2);
        assert_eq!(page1.cursors.len(), 2);
        assert_eq!(page1.end.unwrap().reason(), QueryEndReason::ItemLimit);
        assert_eq!(page1.balances[0].balance(), 10);
        // Mutate state in checkpoint 11 (add coin E and bump C); resuming from page 1 cursor
        // must still read checkpoint 10!
        store.add_balance(owner, struct_tag("0x2::c::C"), 900, 0);
        store.add_balance(owner, struct_tag("0x2::c::E"), 50, 0);
        store.take_snapshot(11);

        let mut opts2 = QueryOptions::default();
        opts2.limit = Some(10);
        opts2.after = Some(page1.cursors[1].clone());
        let mut req2 = ListBalancesRequest::default();
        req2.owner = Some(owner.to_string());
        req2.options = Some(opts2);
        let page2 = list_balances(&service, req2).unwrap();
        assert_eq!(page2.balances.len(), 2);
        assert_eq!(page2.end.unwrap().reason(), QueryEndReason::CheckpointBound);
        let all_asc: Vec<_> = page1
            .balances
            .iter()
            .chain(&page2.balances)
            .map(|b| (b.coin_type().to_owned(), b.balance()))
            .collect();
        assert_eq!(all_asc.len(), 4);
        // C is still 30 (from checkpoint 10), and E is absent.
        assert!(all_asc.iter().any(|(t, v)| t.ends_with("::C") && *v == 30));
        assert!(!all_asc.iter().any(|(t, _)| t.ends_with("::E")));

        // Descending order is the exact reverse of ascending canonical order.
        let mut desc_opts = QueryOptions::default();
        desc_opts.ordering = Some(Ordering::Descending as i32);
        desc_opts.limit = Some(10);
        let mut req_desc = ListBalancesRequest::default();
        req_desc.owner = Some(owner.to_string());
        req_desc.at_checkpoint = Some(10);
        req_desc.options = Some(desc_opts);
        let desc = list_balances(&service, req_desc).unwrap();
        assert_eq!(desc.end.unwrap().reason(), QueryEndReason::CheckpointBound);
        let desc_items: Vec<_> = desc
            .balances
            .iter()
            .map(|b| (b.coin_type().to_owned(), b.balance()))
            .collect();
        let mut expected_desc = all_asc.clone();
        expected_desc.reverse();
        assert_eq!(desc_items, expected_desc);

        // Exclusive (after, before) bounds end with CURSOR_BOUND in both directions.
        let mut bounded_asc = QueryOptions::default();
        bounded_asc.after = Some(page1.cursors[0].clone());
        bounded_asc.before = Some(page2.cursors[1].clone());
        let mut req_ba = ListBalancesRequest::default();
        req_ba.owner = Some(owner.to_string());
        req_ba.options = Some(bounded_asc);
        let ba = list_balances(&service, req_ba).unwrap();
        assert_eq!(ba.balances.len(), 2);
        assert_eq!(ba.end.unwrap().reason(), QueryEndReason::CursorBound);

        let mut bounded_desc = QueryOptions::default();
        bounded_desc.ordering = Some(Ordering::Descending as i32);
        bounded_desc.after = Some(page1.cursors[0].clone());
        bounded_desc.before = Some(page2.cursors[1].clone());
        let mut req_bd = ListBalancesRequest::default();
        req_bd.owner = Some(owner.to_string());
        req_bd.options = Some(bounded_desc);
        let bd = list_balances(&service, req_bd).unwrap();
        assert_eq!(bd.balances.len(), 2);
        assert_eq!(bd.end.unwrap().reason(), QueryEndReason::CursorBound);
        assert_eq!(bd.balances[0].coin_type(), ba.balances[1].coin_type());
        assert_eq!(bd.balances[1].coin_type(), ba.balances[0].coin_type());

        // Mixing options with page_size or page_token fails with INVALID_ARGUMENT.
        let mut mixed_size = ListBalancesRequest::default();
        mixed_size.owner = Some(owner.to_string());
        mixed_size.page_size = Some(1);
        mixed_size.options = Some(QueryOptions::default());
        assert_eq!(
            list_balances(&service, mixed_size).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );

        let mut mixed_token = ListBalancesRequest::default();
        mixed_token.owner = Some(owner.to_string());
        mixed_token.page_token = Some(page1.cursors[0].clone());
        mixed_token.options = Some(QueryOptions::default());
        assert_eq!(
            list_balances(&service, mixed_token).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );
        // Cursor for another owner fails with INVALID_ARGUMENT.
        let mut wrong_owner_opts = QueryOptions::default();
        wrong_owner_opts.after = Some(page1.cursors[0].clone());
        let mut wrong_owner = ListBalancesRequest::default();
        wrong_owner.owner = Some(test_address(2).to_string());
        wrong_owner.options = Some(wrong_owner_opts);
        assert_eq!(
            list_balances(&service, wrong_owner).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );
    }

    #[test]
    fn legacy_and_pinned_page_tokens_behave_as_specified() {
        let (_dir, store, service) = TestStateStore::open(5);
        let owner = test_address(1);
        for name in ["A", "B", "C"] {
            store.add_balance(owner, struct_tag(&format!("0x2::c::{name}")), 10, 0);
        }
        store.take_snapshot(5);

        // Unpinned legacy request emits bare BCS token and reads current state.
        let mut r1 = ListBalancesRequest::default();
        r1.owner = Some(owner.to_string());
        r1.page_size = Some(1);
        let p1 = list_balances(&service, r1).unwrap();
        assert!(p1.end.is_none());
        assert!(p1.cursors.is_empty());
        let legacy_tok = p1.next_page_token.unwrap();
        // Confirm it is bare BCS.
        assert!(bcs::from_bytes::<LegacyPageToken>(&legacy_tok).is_ok());

        // Current state adds D; resuming with legacy token sees D.
        store.add_balance(owner, struct_tag("0x2::c::D"), 99, 0);
        let mut r2 = ListBalancesRequest::default();
        r2.owner = Some(owner.to_string());
        r2.page_token = Some(legacy_tok);
        let p2 = list_balances(&service, r2).unwrap();
        assert_eq!(p2.balances.len(), 3);

        // Pinned legacy request (`at_checkpoint = 5`) emits CursorToken and stays at 5.
        let mut pr1 = ListBalancesRequest::default();
        pr1.owner = Some(owner.to_string());
        pr1.page_size = Some(1);
        pr1.at_checkpoint = Some(5);
        let pp1 = list_balances(&service, pr1).unwrap();
        let pinned_tok = pp1.next_page_token.unwrap();
        assert!(bcs::from_bytes::<LegacyPageToken>(&pinned_tok).is_err());

        let mut pr2 = ListBalancesRequest::default();
        pr2.owner = Some(owner.to_string());
        pr2.page_token = Some(pinned_tok.clone());
        let pp2 = list_balances(&service, pr2).unwrap();
        assert_eq!(pp2.balances.len(), 2); // B and C only, not D

        // Mismatched `at_checkpoint` vs pinned token -> INVALID_ARGUMENT.
        let mut mismatch = ListBalancesRequest::default();
        mismatch.owner = Some(owner.to_string());
        mismatch.at_checkpoint = Some(6);
        mismatch.page_token = Some(pinned_tok);
        assert_eq!(
            list_balances(&service, mismatch).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );
    }
}
