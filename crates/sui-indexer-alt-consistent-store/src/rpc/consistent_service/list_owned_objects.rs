// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::str::FromStr;

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;
use sui_indexer_alt_framework::types::base_types::SuiAddress;

use crate::rpc::error::{RpcError, StatusCode};
use crate::rpc::pagination::Page;
use crate::rpc::type_filter::{self, TypeFilter};
use crate::schema;

use super::State;

#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    #[error("Bad 'object_type' filter, expected: {0}")]
    BadTypeFilter(#[from] type_filter::Error),

    #[error("Invalid 'address': {0:?}")]
    InvalidAddress(String),

    #[error("Missing 'address' for kind '{}'", .0.as_str_name())]
    MissingAddress(grpc::owner::OwnerKind),

    #[error("Missing 'owner'")]
    MissingOwner,

    #[error("Unexpected 'address' for kind '{}'", .0.as_str_name())]
    UnexpectedAddress(grpc::owner::OwnerKind),
}

impl StatusCode for Error {
    fn code(&self) -> tonic::Code {
        match self {
            Error::BadTypeFilter(_)
            | Error::InvalidAddress(_)
            | Error::MissingAddress(_)
            | Error::MissingOwner
            | Error::UnexpectedAddress(_) => tonic::Code::InvalidArgument,
        }
    }
}

pub(super) fn list_owned_objects(
    state: &State,
    checkpoint: u64,
    request: grpc::ListOwnedObjectsRequest,
) -> Result<grpc::ListOwnedObjectsResponse, RpcError<Error>> {
    let owner = request.owner.as_ref().ok_or(Error::MissingOwner)?;

    use grpc::owner::OwnerKind as GK;
    use schema::object_by_owner::OwnerKind as SK;
    let kind = match (owner.kind(), owner.address()) {
        (GK::Unknown, _) => {
            return Err(Error::MissingOwner.into());
        }

        (kind @ (GK::Address | GK::Object), "") => {
            return Err(Error::MissingAddress(kind).into());
        }

        (GK::Shared, "") => SK::Shared,
        (GK::Immutable, "") => SK::Immutable,
        (GK::Address, address) => SK::AddressOwner(addr(address)?),
        (GK::Object, address) => SK::ObjectOwner(addr(address)?),

        (kind @ (GK::Shared | GK::Immutable), _) => {
            return Err(Error::UnexpectedAddress(kind).into());
        }
    };

    // Only support address filters for now.
    if matches!(kind, SK::Immutable | SK::Shared) {
        return Err(RpcError::Unimplemented);
    }

    let type_: Option<TypeFilter> = (!request.object_type().is_empty())
        .then(|| request.object_type().parse())
        .transpose()
        .map_err(Error::from)?;

    let page = Page::from_request(
        &state.config.pagination,
        request.after_token(),
        request.before_token(),
        request.page_size(),
        request.end(),
    );

    let index = &state.store.schema().object_by_owner;
    let resp = if let Some(type_) = type_ {
        page.paginate_prefix(index, checkpoint, &(kind, Some(type_)))?
    } else {
        page.paginate_prefix(index, checkpoint, &kind)?
    };

    Ok(grpc::ListOwnedObjectsResponse {
        has_previous_page: Some(resp.has_prev),
        has_next_page: Some(resp.has_next),
        objects: resp
            .results
            .into_iter()
            .map(|(token, key, (version, digest))| grpc::Object {
                object_id: Some(key.object_id.to_canonical_string(/* with_prefix */ true)),
                version: Some(version.value()),
                digest: Some(digest.base58_encode()),
                page_token: Some(token.into()),
            })
            .collect(),
    })
}

/// Parse a `SuiAddress` from a string. Addresses must start with `0x` followed by between 1 and 64
/// hexadecimal characters.
///
/// TODO: Switch to using `sui_sdk_types::Address`, once the indexing framework is ported to the
/// new SDK.
fn addr(input: &str) -> Result<SuiAddress, Error> {
    let Some(s) = input.strip_prefix("0x") else {
        return Err(Error::InvalidAddress(input.to_owned()));
    };

    let s = if s.is_empty() || s.len() > 64 {
        return Err(Error::InvalidAddress(s.to_owned()));
    } else if s.len() != 64 {
        Cow::Owned(format!("0x{s:0>64}"))
    } else {
        Cow::Borrowed(input)
    };

    SuiAddress::from_str(s.as_ref()).map_err(|_| Error::InvalidAddress(s.to_string()))
}
