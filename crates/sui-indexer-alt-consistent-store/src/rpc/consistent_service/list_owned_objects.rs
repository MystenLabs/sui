// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::str::FromStr;

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;
use sui_indexer_alt_framework::types::base_types::SuiAddress;

use crate::rpc::error::{RpcError, StatusCode};
use crate::schema::{self, Schema};
use crate::store::Store;

#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    #[error("Missing 'owner'")]
    MissingOwner,

    #[error("Missing 'address' for kind '{}'", .0.as_str_name())]
    MissingAddress(grpc::owner::OwnerKind),

    #[error("Unexpected 'address' for kind '{}'", .0.as_str_name())]
    UnexpectedAddress(grpc::owner::OwnerKind),

    #[error("Invalid 'address': {0:?}")]
    InvalidAddress(String),
}

impl StatusCode for Error {
    fn code(&self) -> tonic::Code {
        match self {
            Error::MissingOwner
            | Error::MissingAddress(_)
            | Error::UnexpectedAddress(_)
            | Error::InvalidAddress(_) => tonic::Code::InvalidArgument,
        }
    }
}

pub(super) fn list_owned_objects(
    _store: &Store<Schema>,
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

    // Type filters not supported yet.
    if !request.object_type().is_empty() {
        return Err(RpcError::Unimplemented);
    }

    // Address filters not supported yet.
    Err(RpcError::Unimplemented)
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
