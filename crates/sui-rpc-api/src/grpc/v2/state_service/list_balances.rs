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
