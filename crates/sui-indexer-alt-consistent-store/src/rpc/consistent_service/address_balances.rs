// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::str::FromStr;

use anyhow::Context;
use bincode::serde::Compat;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;
use sui_indexer_alt_framework::types::{TypeTag, base_types::SuiAddress};

use crate::{
    rpc::{
        error::{RpcError, StatusCode, db_error},
        pagination::Page,
    },
    schema::address_balances::Key,
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

pub(super) fn batch_get_address_balances(
    state: &State,
    checkpoint: u64,
    request: grpc::BatchGetAddressBalancesRequest,
) -> Result<grpc::BatchGetAddressBalancesResponse, RpcError<Error>> {
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

    let index = &state.store.schema().address_balances;
    let values = index
        .multi_get(checkpoint, &keys)
        .map_err(|e| db_error(e, "failed to batch get address balances"))?;

    let mut balances = Vec::with_capacity(values.len());
    for (key, value) in keys.into_iter().zip(values) {
        balances.push(stored_to_balance(
            &key,
            value.context("Failed to deserialize balance")?.unwrap_or(0),
            None,
        )?);
    }

    Ok(grpc::BatchGetAddressBalancesResponse { balances })
}

pub(super) fn get_address_balance(
    state: &State,
    checkpoint: u64,
    request: grpc::GetAddressBalanceRequest,
) -> Result<grpc::AddressBalance, RpcError<Error>> {
    let key = key(request)?;
    let index = &state.store.schema().address_balances;
    let balance = index
        .get(checkpoint, &key)
        .map_err(|e| db_error(e, "failed to get address balance"))?
        .unwrap_or(0);

    Ok(stored_to_balance(&key, balance, None)?)
}

pub(super) fn list_address_balances(
    state: &State,
    checkpoint: u64,
    request: grpc::ListAddressBalancesRequest,
) -> Result<grpc::ListAddressBalancesResponse, RpcError<Error>> {
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

    let index = &state.store.schema().address_balances;
    let resp = page.paginate_prefix(index, checkpoint, &Compat(owner))?;

    let mut balances = vec![];
    for (token, key, balance) in resp.results {
        balances.push(stored_to_balance(&key, balance, Some(token))?);
    }

    Ok(grpc::ListAddressBalancesResponse {
        has_previous_page: Some(resp.has_prev),
        has_next_page: Some(resp.has_next),
        balances,
    })
}

/// Convert a point lookup into a key for the underlying index.
fn key(request: grpc::GetAddressBalanceRequest) -> Result<Key, Error> {
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

    Ok(Key { owner, type_ })
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

fn stored_to_balance(
    key: &Key,
    stored: u128,
    page_token: Option<Vec<u8>>,
) -> anyhow::Result<grpc::AddressBalance> {
    let with_prefix = true;
    let coin_type = key.type_.to_canonical_string(with_prefix);
    let balance = u64::try_from(stored)
        .with_context(|| format!("Bad balance for type {coin_type}: {stored}"))?;

    Ok(grpc::AddressBalance {
        owner: Some(key.owner.to_string()),
        coin_type: Some(coin_type),
        balance: Some(balance),
        page_token: page_token.map(|token| token.into()),
    })
}
