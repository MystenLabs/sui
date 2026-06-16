// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `ConsistentService` balance handlers.
//!
//! Reads off the [`balance`] CF. Our schema folds the coin- and
//! address-side accumulators into a single [`BalanceDelta`] row,
//! so we don't need the alt-consistent-store's two-CF merge; one
//! read per `(owner, coin_type)` returns both halves.
//!
//! [`balance`]: sui_rpc_store::schema::balance
//! [`BalanceDelta`]: sui_rpc_store::proto::BalanceDelta

use std::str::FromStr;

use sui_consistent_store::Schema as _;
use sui_consistent_store::SchemaAtSnapshot as _;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::proto::BalanceDelta;
use sui_rpc_store::schema::balance;
use sui_types::TypeTag;
use sui_types::base_types::SuiAddress;

use crate::consistent_service::State;
use crate::consistent_service::pagination::End;
use crate::consistent_service::pagination::Page;
use crate::consistent_service::state::Error as StateError;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("missing owner")]
    MissingOwner,
    #[error("missing coin_type")]
    MissingType,
    #[error("invalid owner: {0:?}")]
    InvalidOwner(String),
    #[error("invalid coin_type: {0:?}")]
    InvalidType(String),
    #[error("too many requests in batch: {got} (max: {max})")]
    TooManyRequests { got: usize, max: u32 },
    #[error(transparent)]
    Db(#[from] sui_consistent_store::error::Error),
    #[error("failed to open schema: {0}")]
    Open(#[from] sui_consistent_store::error::OpenError),
    #[error(transparent)]
    State(#[from] StateError),
}

impl From<Error> for tonic::Status {
    fn from(e: Error) -> Self {
        match e {
            Error::MissingOwner
            | Error::MissingType
            | Error::InvalidOwner(_)
            | Error::InvalidType(_)
            | Error::TooManyRequests { .. } => tonic::Status::invalid_argument(e.to_string()),
            Error::Db(_) | Error::Open(_) => tonic::Status::internal(e.to_string()),
            Error::State(s) => tonic::Status::from(s),
        }
    }
}

pub(super) fn get_balance(
    state: &State,
    checkpoint: u64,
    request: grpc::GetBalanceRequest,
) -> Result<grpc::Balance, Error> {
    let (owner, coin_type) = parse_key(&request)?;
    let snap = state.snapshot(checkpoint)?;
    let schema = RpcStoreSchema::open(&state.db)?.at(&snap);
    let balance = schema
        .get_balance(owner, coin_type.clone())?
        .unwrap_or_default();
    Ok(balance_proto(owner, &coin_type, &balance, None))
}

pub(super) fn batch_get_balances(
    state: &State,
    checkpoint: u64,
    request: grpc::BatchGetBalancesRequest,
) -> Result<grpc::BatchGetBalancesResponse, Error> {
    let pagination = &*state.pagination;
    if request.requests.len() > pagination.max_batch_size as usize {
        return Err(Error::TooManyRequests {
            got: request.requests.len(),
            max: pagination.max_batch_size,
        });
    }

    let snap = state.snapshot(checkpoint)?;
    let schema = RpcStoreSchema::open(&state.db)?.at(&snap);

    let mut balances = Vec::with_capacity(request.requests.len());
    for req in request.requests {
        let (owner, coin_type) = parse_key(&req)?;
        let bal = schema
            .get_balance(owner, coin_type.clone())?
            .unwrap_or_default();
        balances.push(balance_proto(owner, &coin_type, &bal, None));
    }
    Ok(grpc::BatchGetBalancesResponse { balances })
}

pub(super) fn list_balances(
    state: &State,
    checkpoint: u64,
    request: grpc::ListBalancesRequest,
) -> Result<grpc::ListBalancesResponse, Error> {
    let owner = parse_owner(request.owner())?;

    let snap = state.snapshot(checkpoint)?;
    let schema = RpcStoreSchema::open(&state.db)?.at(&snap);

    let page = Page::from_request(
        &state.pagination,
        request.after_token(),
        request.before_token(),
        request.page_size(),
        End::from_proto(request.end.unwrap_or_default()),
    );

    // The compaction filter eventually drops rows whose both
    // sides are zero, but until then we filter them out so a
    // pruned-but-not-yet-compacted balance doesn't surface.
    let resp = page.paginate_filtered(
        &schema.balance,
        &balance::OwnerPrefix(owner),
        |_token, _key: &balance::Key, value: &balance::Value| {
            // Skip rows whose merge collapsed both halves to
            // zero. The compaction filter will eventually drop
            // them but until then they look like a "live row
            // with a balance of zero" — which the caller would
            // see as an empty-but-nonzero `Balance` entry.
            // Mirrors the alt-consistent-store's
            // `*balance > 0` filter.
            let stored: &BalanceDelta = value;
            read_i128(&stored.coin) != 0 || read_i128(&stored.address) != 0
        },
    )?;

    let mut balances = Vec::with_capacity(resp.results.len());
    for (token, key, value) in resp.results {
        let stored = value.into_inner();
        let bal = balance::Balance {
            coin: read_i128(&stored.coin),
            address: read_i128(&stored.address),
        };
        balances.push(balance_proto(key.owner, &key.coin_type, &bal, Some(token)));
    }

    Ok(grpc::ListBalancesResponse {
        has_previous_page: Some(resp.has_prev),
        has_next_page: Some(resp.has_next),
        balances,
    })
}

fn parse_key(req: &grpc::GetBalanceRequest) -> Result<(SuiAddress, TypeTag), Error> {
    let owner = parse_owner(req.owner())?;
    let coin_type = if req.coin_type().is_empty() {
        return Err(Error::MissingType);
    } else {
        TypeTag::from_str(req.coin_type())
            .map_err(|_| Error::InvalidType(req.coin_type().to_owned()))?
    };
    Ok((owner, coin_type))
}

fn parse_owner(input: &str) -> Result<SuiAddress, Error> {
    if input.is_empty() {
        return Err(Error::MissingOwner);
    }
    let stripped = input
        .strip_prefix("0x")
        .ok_or_else(|| Error::InvalidOwner(input.to_owned()))?;
    if stripped.is_empty() || stripped.len() > 64 {
        return Err(Error::InvalidOwner(input.to_owned()));
    }
    let padded = if stripped.len() == 64 {
        input.to_owned()
    } else {
        format!("0x{stripped:0>64}")
    };
    SuiAddress::from_str(&padded).map_err(|_| Error::InvalidOwner(input.to_owned()))
}

fn balance_proto(
    owner: SuiAddress,
    coin_type: &TypeTag,
    balance: &balance::Balance,
    page_token: Option<Vec<u8>>,
) -> grpc::Balance {
    let with_prefix = true;
    grpc::Balance {
        owner: Some(owner.to_string()),
        coin_type: Some(coin_type.to_canonical_string(with_prefix)),
        total_balance: Some(clamp_u64(balance.total())),
        coin_balance: Some(clamp_u64(balance.coin)),
        address_balance: Some(clamp_u64(balance.address)),
        page_token: page_token.map(Into::into),
    }
}

fn clamp_u64(value: i128) -> u64 {
    if value < 0 {
        0
    } else {
        u64::try_from(value).unwrap_or(u64::MAX)
    }
}

fn read_i128(bytes: &[u8]) -> i128 {
    if bytes.len() != 16 {
        return 0;
    }
    let mut buf = [0u8; 16];
    buf.copy_from_slice(bytes);
    i128::from_le_bytes(buf)
}
