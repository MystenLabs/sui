// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;

use crate::rpc::{
    error::{RpcError, StatusCode},
    pagination::Page,
    type_filter::{self, TypeFilter},
};

use super::State;

#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    #[error("Bad 'object_type' filter, expected: {0}")]
    BadTypeFilter(#[from] type_filter::Error),

    #[error("Missing 'object_type' filter")]
    MissingType,
}

impl StatusCode for Error {
    fn code(&self) -> tonic::Code {
        match self {
            Error::BadTypeFilter(_) | Error::MissingType => tonic::Code::InvalidArgument,
        }
    }
}

pub(super) fn list_objects_by_type(
    state: &State,
    checkpoint: u64,
    request: grpc::ListObjectsByTypeRequest,
) -> Result<grpc::ListObjectsResponse, RpcError<Error>> {
    let type_: TypeFilter = (!request.object_type().is_empty())
        .then(|| request.object_type().parse())
        .transpose()
        .map_err(Error::from)?
        .ok_or(Error::MissingType)?;

    let page = Page::from_request(
        &state.config.pagination,
        request.after_token(),
        request.before_token(),
        request.page_size(),
        request.end(),
    );

    let index = &state.store.schema().object_by_type;
    let resp = page.paginate_prefix(index, checkpoint, &type_)?;

    Ok(grpc::ListObjectsResponse {
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
