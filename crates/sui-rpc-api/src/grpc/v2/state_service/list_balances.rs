// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Bound;

use bytes::Bytes;
use move_core_types::language_storage::StructTag;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::ListBalancesRequest;
use sui_rpc::proto::sui::rpc::v2::ListBalancesResponse;
use sui_rpc::proto::sui::rpc::v2::QueryEnd;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_sdk_types::Address;
use sui_types::base_types::SuiAddress;

use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use crate::reader::indexes_disabled;

use super::checkpoint::PinSource;
use super::checkpoint::resolve_pinned;
use super::checkpoint::resolve_state_read;
use super::cursor::DecodedPageToken;
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
    service
        .reader
        .inner()
        .indexes()
        .ok_or_else(indexes_disabled)?;

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

    if let Some(options) = request.options.as_ref() {
        reject_legacy_paging_with_options(request.page_size, request.page_token.as_deref())?;
        let opts = parse_query_options(
            Some(options),
            request.at_checkpoint,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
            extract_key,
        )?;
        let pinned = resolve_pinned(
            service.reader.inner().as_ref(),
            opts.checkpoint,
            opts.pin_source,
            false,
        )?;

        let (lower, upper) = opts.bounds();
        let rows = pinned
            .indexes
            .balance_iter(&owner_sui, lower, upper, opts.descending)?
            .take(opts.limit)
            .map(|result| {
                result.map_err(|err| RpcError::new(tonic::Code::Internal, err.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let hit_limit = rows.len() == opts.limit;
        let mut response = ListBalancesResponse::default();
        response.balances = Vec::with_capacity(rows.len());
        response.cursors = Vec::with_capacity(rows.len());
        for (coin_type, balance_info) in rows {
            response.cursors.push(
                StateCursor {
                    checkpoint: pinned.checkpoint,
                    key: StateCursorKey::Balances {
                        owner: owner_sui,
                        coin_type: coin_type.clone(),
                    },
                }
                .encode(),
            );
            response.balances.push(super::get_balance::render_balance(
                service,
                owner_sui,
                coin_type,
                balance_info,
            ));
        }
        let end_reason = if hit_limit {
            QueryEndReason::ItemLimit
        } else {
            opts.exhausted_end_reason()
        };
        response.end = Some(QueryEnd::default().with_reason(end_reason));
        return Ok(response);
    }

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, MAX_PAGE_SIZE))
        .unwrap_or(DEFAULT_PAGE_SIZE);
    let page_token = request
        .page_token
        .as_deref()
        .map(|token| {
            decode_page_token::<LegacyPageToken, StructTag, _, _>(
                token,
                request.at_checkpoint,
                |t| t.owner == owner,
                extract_key,
            )
        })
        .transpose()?;

    let (checkpoint, pin, pin_source, lower) = match page_token {
        Some(DecodedPageToken::Legacy(token)) => (
            None,
            false,
            PinSource::AtCheckpoint,
            Bound::Included(token.coin_type),
        ),
        Some(DecodedPageToken::Pinned { checkpoint, key }) => (
            Some(checkpoint),
            true,
            if request.at_checkpoint.is_some() {
                PinSource::AtCheckpoint
            } else {
                PinSource::Cursor
            },
            Bound::Excluded(key),
        ),
        None => (
            request.at_checkpoint,
            request.at_checkpoint.is_some(),
            PinSource::AtCheckpoint,
            Bound::Unbounded,
        ),
    };

    let target = resolve_state_read(service, checkpoint, pin, pin_source, false)?;

    let mut rows = target
        .indexes()
        .balance_iter(&owner_sui, lower, Bound::Unbounded, false)?
        .take(page_size + 1)
        .map(|result| result.map_err(|err| RpcError::new(tonic::Code::Internal, err.to_string())))
        .collect::<Result<Vec<_>, _>>()?;

    let next_page_token = if rows.len() > page_size {
        let next_row = rows.pop().unwrap();
        if let Some(pinned) = target.pinned() {
            let (last_coin_type, _) = rows.last().expect("page_size >= 1");
            Some(
                StateCursor {
                    checkpoint: pinned.checkpoint,
                    key: StateCursorKey::Balances {
                        owner: owner_sui,
                        coin_type: last_coin_type.clone(),
                    },
                }
                .encode(),
            )
        } else {
            Some(encode_legacy_page_token(LegacyPageToken {
                owner,
                coin_type: next_row.0,
            }))
        }
    } else {
        None
    };

    let mut response = ListBalancesResponse::default();
    response.balances = rows
        .into_iter()
        .map(|(coin_type, balance_info)| {
            super::get_balance::render_balance(service, owner_sui, coin_type, balance_info)
        })
        .collect();
    response.next_page_token = next_page_token;
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
        let mut mixed = ListBalancesRequest::default();
        mixed.owner = Some(owner.to_string());
        mixed.page_size = Some(1);
        mixed.options = Some(QueryOptions::default());
        assert_eq!(
            list_balances(&service, mixed).unwrap_err().code(),
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
