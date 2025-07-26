// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::str::FromStr;

use anyhow::Context;
use bincode::serde::Compat;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;
use sui_indexer_alt_framework::types::{base_types::SuiAddress, TypeTag};

use crate::{
    rpc::{
        error::{RpcError, StatusCode},
        pagination::Page,
    },
    schema::balances::Key,
};

use super::State;

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
}

impl StatusCode for Error {
    fn code(&self) -> tonic::Code {
        match self {
            Error::InvalidOwner(_)
            | Error::InvalidType(_)
            | Error::MissingOwner
            | Error::MissingType => tonic::Code::InvalidArgument,
        }
    }
}

pub(super) fn get_balance(
    state: &State,
    checkpoint: u64,
    request: grpc::GetBalanceRequest,
) -> Result<grpc::Balance, RpcError<Error>> {
    let owner = if request.owner().is_empty() {
        return Err(Error::MissingOwner.into());
    } else {
        owner(request.owner())?
    };

    let coin_type = if request.coin_type().is_empty() {
        return Err(Error::MissingType.into());
    } else {
        TypeTag::from_str(request.coin_type())
            .map_err(|_| Error::InvalidType(request.coin_type().to_owned()))?
    };

    let key = Key {
        owner,
        type_: coin_type,
    };

    let index = &state.store.schema().balances;
    let balance = index
        .get(checkpoint, &key)
        .context("Failed to fetch data")?
        .unwrap_or(0);

    let with_prefix = true;
    let coin_type = key.type_.to_canonical_string(with_prefix);
    let balance = u64::try_from(balance)
        .with_context(|| format!("Bad balance for type {coin_type}: {balance}"))?;

    Ok(grpc::Balance {
        coin_type: Some(coin_type),
        balance: Some(balance),
        page_token: None,
    })
}

pub(super) fn list_balances(
    state: &State,
    checkpoint: u64,
    request: grpc::ListBalancesRequest,
) -> Result<grpc::ListBalancesResponse, RpcError<Error>> {
    if request.owner().is_empty() {
        return Err(Error::MissingOwner.into());
    }

    let owner = owner(request.owner())?;

    let page = Page::from_request(
        &state.config.pagination,
        request.after_token(),
        request.before_token(),
        request.page_size(),
        request.end(),
    );

    let index = &state.store.schema().balances;
    // Zero balances may end up in the databases through accumulation. They will eventually be
    // cleaned up by compaction, but until then, they need to be filtered out of results (similar
    // to how RocksDB filters out tombstones).
    let resp = page.paginate_filtered(index, checkpoint, &Compat(owner), |_, _, balance| {
        *balance > 0
    })?;

    let mut balances = vec![];
    for (token, key, balance) in resp.results {
        let coin_type = key.type_.to_canonical_string(/* with_prefix */ true);
        let balance = u64::try_from(balance)
            .with_context(|| format!("Bad balance for type {coin_type}: {balance}"))?;

        balances.push(grpc::Balance {
            coin_type: Some(coin_type),
            balance: Some(balance),
            page_token: Some(token.into()),
        });
    }

    Ok(grpc::ListBalancesResponse {
        has_previous_page: Some(resp.has_prev),
        has_next_page: Some(resp.has_next),
        balances,
    })
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
