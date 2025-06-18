// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::rpc::v2beta2::Balance;
use crate::proto::rpc::v2beta2::ListBalancesRequest;
use crate::proto::rpc::v2beta2::ListBalancesResponse;
use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use bytes::Bytes;
use sui_sdk_types::Address;
use sui_types::storage::BalanceInfo;
use tap::Pipe;

#[tracing::instrument(skip(service))]
pub fn list_balances(
    service: &RpcService,
    request: ListBalancesRequest,
) -> Result<ListBalancesResponse> {
    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?;

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

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, 1000))
        .unwrap_or(50);
    let page_token = request
        .page_token
        .map(|token| decode_page_token(&token))
        .transpose()?;

    if let Some(token) = &page_token {
        if token.owner != owner {
            return Err(FieldViolation::new("page_token")
                .with_description("page token owner does not match request owner")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
    }

    let mut balances = indexes
        .balance_iter(
            &owner.into(),
            page_token.map(|t| (owner.into(), t.coin_type)),
        )?
        .take(page_size + 1)
        .map(|result| result.map_err(|err| RpcError::new(tonic::Code::Internal, err.to_string())))
        .collect::<Result<Vec<_>, _>>()?;

    let next_page_token = if balances.len() > page_size {
        // SAFETY: We've already verified that balances is greater than limit, which is
        // guaranteed to be >= 1.
        balances
            .pop()
            .unwrap()
            .pipe(|(coin_type, _info)| encode_page_token(PageToken { owner, coin_type }))
            .pipe(Some)
    } else {
        None
    };

    Ok(ListBalancesResponse {
        balances: balances
            .into_iter()
            .map(balance_info_to_proto)
            .collect::<Result<Vec<_>>>()?,
        next_page_token,
    })
}

fn decode_page_token(page_token: &[u8]) -> Result<PageToken> {
    bcs::from_bytes(page_token).map_err(|e| {
        FieldViolation::new("page_token")
            .with_description(format!("invalid page token encoding: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

fn encode_page_token(page_token: PageToken) -> Bytes {
    bcs::to_bytes(&page_token).unwrap().into()
}

fn balance_info_to_proto(
    (coin_type, info): (move_core_types::language_storage::StructTag, BalanceInfo),
) -> Result<Balance> {
    Ok(Balance {
        coin_type: Some(
            sui_types::sui_sdk_types_conversions::struct_tag_core_to_sdk(coin_type)?.to_string(),
        ),
        balance: Some(info.balance),
    })
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PageToken {
    owner: Address,
    coin_type: move_core_types::language_storage::StructTag,
}
