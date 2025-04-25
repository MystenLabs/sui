// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::rpc::v2alpha::ListOwnedObjectsRequest;
use crate::proto::rpc::v2alpha::ListOwnedObjectsResponse;
use crate::proto::rpc::v2alpha::OwnedObject;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use bytes::Bytes;
use sui_sdk_types::Address;
use sui_types::storage::OwnedObjectInfo;
use tap::Pipe;

#[tracing::instrument(skip(service))]
pub fn list_owned_objects(
    service: &RpcService,
    request: ListOwnedObjectsRequest,
) -> Result<ListOwnedObjectsResponse> {
    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?;

    let owner: Address = request
        .owner
        .as_ref()
        .ok_or_else(|| RpcError::new(tonic::Code::InvalidArgument, "missing owner"))?
        .parse()
        .map_err(|e| RpcError::new(tonic::Code::InvalidArgument, format!("invalid owner: {e}")))?;
    let object_type = request
        .object_type
        .map(|s| s.parse())
        .transpose()
        .map_err(|e| {
            RpcError::new(
                tonic::Code::InvalidArgument,
                format!("invalid object_type: {e}"),
            )
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
        if token.owner != owner || token.object_type != object_type {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                "invalid page_token",
            ));
        }
    }

    let mut object_info = indexes
        .owned_objects_iter(
            owner.into(),
            object_type.clone(),
            page_token.map(|t| t.inner),
        )?
        .take(page_size + 1)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| RpcError::new(tonic::Code::Internal, err.to_string()))?;
    let next_page_token = if object_info.len() > page_size {
        // SAFETY: We've already verified that object_info is greater than limit, which is
        // gaurenteed to be >= 1.
        object_info
            .pop()
            .unwrap()
            .pipe(|cursor| {
                encode_page_token(PageToken {
                    owner,
                    object_type,
                    inner: cursor,
                })
            })
            .pipe(Some)
    } else {
        None
    };

    Ok(ListOwnedObjectsResponse {
        objects: object_info.into_iter().map(owned_object_to_proto).collect(),
        next_page_token,
    })
}

fn decode_page_token(page_token: &[u8]) -> Result<PageToken> {
    bcs::from_bytes(page_token).map_err(Into::into)
}

fn encode_page_token(page_token: PageToken) -> Bytes {
    bcs::to_bytes(&page_token).unwrap().into()
}

fn owned_object_to_proto(info: OwnedObjectInfo) -> OwnedObject {
    OwnedObject {
        object_id: Some(info.object_id.to_string()),
        version: Some(info.version.value()),
        digest: Some(info.digest.to_string()),
        owner: Some(sui_sdk_types::Owner::Address(info.owner.into()).into()),
        object_type: Some(info.object_type.to_canonical_string(true)),
        balance: info.balance,
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PageToken {
    owner: Address,
    object_type: Option<move_core_types::language_storage::StructTag>,
    inner: OwnedObjectInfo,
}
