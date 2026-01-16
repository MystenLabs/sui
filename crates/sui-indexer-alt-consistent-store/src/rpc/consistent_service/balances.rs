// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::cmp::Ordering;
use std::str::FromStr;

use anyhow::Context;
use bincode::serde::Compat;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;
use sui_indexer_alt_framework::types::TypeTag;
use sui_indexer_alt_framework::types::base_types::SuiAddress;

use crate::rpc::consistent_service::State;
use crate::rpc::error::RpcError;
use crate::rpc::error::StatusCode;
use crate::rpc::error::db_error;
use crate::rpc::pagination::Page;
use crate::schema::address_balances::Key as AddressKey;
use crate::schema::balances::Key as CoinKey;

#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    #[error("Invalid 'owner': {0:?}")]
    InvalidOwner(String),

    #[error("Invalid 'coin_type': {0:?}")]
    InvalidType(String),

    #[error("Missing 'owner'")]
    MissingOwner,

    #[error("Missing 'coin_type'")]
    MissingType,

    #[error("Too many requests in batch: {0} (max: {1})")]
    TooManyRequests(usize, u32),
}

impl StatusCode for Error {
    fn code(&self) -> tonic::Code {
        match self {
            Error::InvalidOwner(_)
            | Error::InvalidType(_)
            | Error::MissingOwner
            | Error::MissingType
            | Error::TooManyRequests(_, _) => tonic::Code::InvalidArgument,
        }
    }
}

pub(super) fn batch_get_balances(
    state: &State,
    checkpoint: u64,
    request: grpc::BatchGetBalancesRequest,
) -> Result<grpc::BatchGetBalancesResponse, RpcError<Error>> {
    let config = &state.rpc_config.pagination;
    let keys = if request.requests.len() > config.max_batch_size as usize {
        return Err(Error::TooManyRequests(
            request.requests.len(),
            state.rpc_config.pagination.max_batch_size,
        )
        .into());
    } else {
        request
            .requests
            .into_iter()
            .map(key)
            .collect::<Result<Vec<_>, _>>()?
    };

    let (cb_keys, ab_keys) = keys.into_iter().unzip::<_, _, Vec<_>, Vec<_>>();

    let index = &state.store.schema().balances;
    let cbs = index
        .multi_get(checkpoint, &cb_keys)
        .map_err(|e| db_error(e, "failed to batch get balances"))?;

    let index = &state.store.schema().address_balances;
    let abs = index
        .multi_get(checkpoint, &ab_keys)
        .map_err(|e| db_error(e, "failed to batch get address balances"))?;

    let mut balances = Vec::with_capacity(cb_keys.len());
    for ((cb, ab), key) in cbs.into_iter().zip(abs).zip(cb_keys) {
        let with_prefix = true;
        let coin_type = key.type_.to_canonical_string(with_prefix);
        let coin_balance = try_balance(
            cb.context("Failed to deserialize balance")?.unwrap_or(0),
            &coin_type,
        )?;
        let address_balance = try_balance(
            ab.context("Failed to deserialize balance")?.unwrap_or(0),
            &coin_type,
        )?;

        balances.push(grpc::Balance {
            owner: Some(key.owner.to_string()),
            coin_type: Some(coin_type),
            total_balance: Some(address_balance + coin_balance),
            address_balance: Some(address_balance),
            coin_balance: Some(coin_balance),
            page_token: None,
        });
    }

    Ok(grpc::BatchGetBalancesResponse { balances })
}

pub(super) fn get_balance(
    state: &State,
    checkpoint: u64,
    request: grpc::GetBalanceRequest,
) -> Result<grpc::Balance, RpcError<Error>> {
    let (cb_key, ab_key) = key(request)?;

    let index = &state.store.schema().balances;
    let coin_balance = index
        .get(checkpoint, &cb_key)
        .map_err(|e| db_error(e, "failed to get coin balance"))?
        .unwrap_or(0);

    let index = &state.store.schema().address_balances;
    let address_balance = index
        .get(checkpoint, &ab_key)
        .map_err(|e| db_error(e, "failed to get address balance"))?
        .unwrap_or(0);

    let with_prefix = true;
    let coin_type = cb_key.type_.to_canonical_string(with_prefix);
    let coin_balance = try_balance(coin_balance, &coin_type)?;
    let address_balance = try_balance(address_balance, &coin_type)?;

    Ok(grpc::Balance {
        owner: Some(cb_key.owner.to_string()),
        coin_type: Some(coin_type),
        total_balance: Some(address_balance + coin_balance),
        address_balance: Some(address_balance),
        coin_balance: Some(coin_balance),
        page_token: None,
    })
}

pub(super) fn list_balances(
    state: &State,
    checkpoint: u64,
    request: grpc::ListBalancesRequest,
) -> Result<grpc::ListBalancesResponse, RpcError<Error>> {
    let owner = if request.owner().is_empty() {
        return Err(Error::MissingOwner.into());
    } else {
        owner(request.owner())?
    };

    let page = Page::from_request(
        &state.rpc_config.pagination,
        request.after_token(),
        request.before_token(),
        request.page_size(),
        request.end(),
    );

    let index = &state.store.schema().balances;
    // Zero balances may end up in the databases through accumulation. They will eventually be
    // cleaned up by compaction, but until then, they need to be filtered out of results (similar
    // to how RocksDB filters out tombstones).
    let cb_resp = page.paginate_filtered(index, checkpoint, &Compat(owner), |_, _, balance| {
        *balance > 0
    })?;

    let index = &state.store.schema().address_balances;
    let ab_resp = page.paginate_prefix(index, checkpoint, &Compat(owner))?;

    let merged_results = merge_balances(cb_resp.results, ab_resp.results);
    let limit = page.limit();
    let has_overflow = merged_results.len() > limit;
    let (has_prev, has_next) = if page.is_from_front() {
        // Forward: overflow affects has_next
        (
            cb_resp.has_prev || ab_resp.has_prev,
            has_overflow || cb_resp.has_next || ab_resp.has_next,
        )
    } else {
        // Backward: overflow affects has_prev
        (
            has_overflow || cb_resp.has_prev || ab_resp.has_prev,
            cb_resp.has_next || ab_resp.has_next,
        )
    };

    let truncated: Vec<_> = if page.is_from_front() {
        merged_results.into_iter().take(limit).collect()
    } else {
        let skip = merged_results.len().saturating_sub(limit);
        merged_results.into_iter().skip(skip).collect()
    };

    let mut balances = vec![];
    for (token, (owner, type_tag), coin_balance, address_balance) in truncated {
        let coin_type = type_tag.to_canonical_string(/* with_prefix */ true);
        let coin_balance = try_balance(coin_balance, &coin_type)?;
        let address_balance = try_balance(address_balance, &coin_type)?;

        balances.push(grpc::Balance {
            owner: Some(owner.to_string()),
            coin_type: Some(coin_type),
            total_balance: Some(address_balance + coin_balance),
            address_balance: Some(address_balance),
            coin_balance: Some(coin_balance),
            page_token: Some(token.into()),
        });
    }

    Ok(grpc::ListBalancesResponse {
        has_previous_page: Some(has_prev),
        has_next_page: Some(has_next),
        balances,
    })
}

/// Merge coin and address balances for the same owner on coin type. The inputs are expected to be
/// in ascending order.
#[allow(clippy::type_complexity)]
fn merge_balances(
    coin_balances: Vec<(Vec<u8>, CoinKey, i128)>,
    address_balances: Vec<(Vec<u8>, AddressKey, u128)>,
) -> Vec<(Vec<u8>, (SuiAddress, TypeTag), i128, u128)> {
    let mut merged = Vec::with_capacity(coin_balances.len() + address_balances.len());
    let mut coins = coin_balances.into_iter().peekable();
    let mut addresses = address_balances.into_iter().peekable();
    // Continue to merge until both iterators are exhausted.
    loop {
        let pick_from = match (coins.peek(), addresses.peek()) {
            (None, None) => break,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some((_, ck, _)), Some((_, ak, _))) => ck.type_.cmp(&ak.type_),
        };

        let next = match pick_from {
            Ordering::Less => {
                let (token, key, balance) = coins.next().unwrap();
                (token, (key.owner, key.type_.clone()), balance, 0)
            }
            Ordering::Greater => {
                let (token, key, balance) = addresses.next().unwrap();
                (token, (key.owner, key.type_.clone()), 0, balance)
            }
            Ordering::Equal => {
                let (ctoken, ckey, cbalance) = coins.next().unwrap();
                let (_, _, abalance) = addresses.next().unwrap();
                (ctoken, (ckey.owner, ckey.type_.clone()), cbalance, abalance)
            }
        };

        merged.push(next);
    }
    merged
}

/// Convert a point lookup into keys for the underlying indexes.
fn key(request: grpc::GetBalanceRequest) -> Result<(CoinKey, AddressKey), Error> {
    let owner = if request.owner().is_empty() {
        return Err(Error::MissingOwner);
    } else {
        owner(request.owner())?
    };

    let type_ = if request.coin_type().is_empty() {
        return Err(Error::MissingType);
    } else {
        TypeTag::from_str(request.coin_type())
            .map_err(|_| Error::InvalidType(request.coin_type().to_owned()))?
    };

    Ok((
        CoinKey {
            owner,
            type_: type_.clone(),
        },
        AddressKey { owner, type_ },
    ))
}

/// Parse the owner's `SuiAddress` from a string. Addresses must start with `0x` followed by
/// between 1 and 64 hexadecimal characters.
///
/// TODO: Switch to using `sui_sdk_types::Address`, once the indexing framework is ported to the
/// new SDK.
fn owner(input: &str) -> Result<SuiAddress, Error> {
    let Some(s) = input.strip_prefix("0x") else {
        return Err(Error::InvalidOwner(input.to_owned()));
    };

    let s = if s.is_empty() || s.len() > 64 {
        return Err(Error::InvalidOwner(s.to_owned()));
    } else if s.len() != 64 {
        Cow::Owned(format!("0x{s:0>64}"))
    } else {
        Cow::Borrowed(input)
    };

    SuiAddress::from_str(s.as_ref()).map_err(|_| Error::InvalidOwner(s.to_string()))
}

fn try_balance<T>(stored: T, coin_type: &str) -> anyhow::Result<u64>
where
    T: TryInto<u64> + std::fmt::Display + Copy,
    <T as TryInto<u64>>::Error: std::error::Error + Send + Sync + 'static,
{
    stored
        .try_into()
        .with_context(|| format!("Bad balance for type {coin_type}: {stored}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coin_key(owner: SuiAddress, type_str: &str) -> CoinKey {
        CoinKey {
            owner,
            type_: TypeTag::from_str(type_str).unwrap(),
        }
    }

    fn address_key(owner: SuiAddress, type_str: &str) -> AddressKey {
        AddressKey {
            owner,
            type_: TypeTag::from_str(type_str).unwrap(),
        }
    }

    #[test]
    /// Coin balance [A, C], address balance [B, D] are interleaved and merged to [A, B, C, D]
    fn test_merge_balances_no_overlap() {
        let owner = SuiAddress::ZERO;

        let coin_balances = vec![
            (vec![1], coin_key(owner, "0x1::a::A"), 100),
            (vec![3], coin_key(owner, "0x1::c::C"), 300),
        ];
        let address_balances = vec![
            (vec![2], address_key(owner, "0x1::b::B"), 200),
            (vec![4], address_key(owner, "0x1::d::D"), 400),
        ];

        let result = merge_balances(coin_balances, address_balances);

        assert_eq!(result.len(), 4);
        // A (coin only)
        assert_eq!(result[0].1.1.to_string(), "0x1::a::A");
        assert_eq!(result[0].2, 100); // coin_balance
        assert_eq!(result[0].3, 0); // address_balance
        // B (address only)
        assert_eq!(result[1].1.1.to_string(), "0x1::b::B");
        assert_eq!(result[1].2, 0);
        assert_eq!(result[1].3, 200);
        // C (coin only)
        assert_eq!(result[2].1.1.to_string(), "0x1::c::C");
        assert_eq!(result[2].2, 300);
        assert_eq!(result[2].3, 0);
        // D (address only)
        assert_eq!(result[3].1.1.to_string(), "0x1::d::D");
        assert_eq!(result[3].2, 0);
        assert_eq!(result[3].3, 400);
    }

    /// Coin balance [A, C], address balance [B, C] with overlap on C → merged [A, B, C (merged)]
    #[test]
    fn test_merge_balances_partial_overlap() {
        let owner = SuiAddress::ZERO;

        let coin_balances = vec![
            (vec![1], coin_key(owner, "0x1::a::A"), 100),
            (vec![3], coin_key(owner, "0x1::c::C"), 300),
        ];
        let address_balances = vec![
            (vec![2], address_key(owner, "0x1::b::B"), 200),
            (vec![99], address_key(owner, "0x1::c::C"), 50), // same type as coin C
        ];

        let result = merge_balances(coin_balances, address_balances);

        assert_eq!(result.len(), 3);
        // A (coin only)
        assert_eq!(result[0].1.1.to_string(), "0x1::a::A");
        assert_eq!(result[0].2, 100);
        assert_eq!(result[0].3, 0);
        // B (address only)
        assert_eq!(result[1].1.1.to_string(), "0x1::b::B");
        assert_eq!(result[1].2, 0);
        assert_eq!(result[1].3, 200);
        // C (merged: coin=300, address=50)
        assert_eq!(result[2].1.1.to_string(), "0x1::c::C");
        assert_eq!(result[2].2, 300);
        assert_eq!(result[2].3, 50);
        // Token should be from coin (vec![3]), not address (vec![99])
        assert_eq!(result[2].0, vec![3]);
    }
}
