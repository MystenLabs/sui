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
use sui_sdk_types::Version;
use sui_sdk_types::{ObjectId, StructTag};
use sui_types::sui_sdk_types_conversions::struct_tag_core_to_sdk;
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
        .map_err(|e| RpcError::new(tonic::Code::InvalidArgument, format!("invalid parent: {e}")))?;

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, 1000))
        .unwrap_or(50);
    let page_token = request
        .page_token
        .map(|token| decode_page_token(&token))
        .transpose()?;

    let mut object_info = indexes
        .account_owned_objects_info_iter(owner.into(), page_token.map(Into::into))?
        .take(page_size + 1)
        .map(|result| {
            result
                .map_err(|err| RpcError::new(tonic::Code::Internal, err.to_string()))
                .and_then(|info| {
                    OwnedOwnedObjectInfo {
                        owner: info.owner.into(),
                        object_id: info.object_id.into(),
                        version: info.version.into(),
                        type_: struct_tag_core_to_sdk(info.type_.into())?,
                    }
                    .pipe(Ok)
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let next_page_token = if object_info.len() > page_size {
        // SAFETY: We've already verified that object_info is greater than limit, which is
        // gaurenteed to be >= 1.
        object_info
            .pop()
            .unwrap()
            .object_id
            .pipe(encode_page_token)
            .pipe(Some)
    } else {
        None
    };

    Ok(ListOwnedObjectsResponse {
        objects: object_info
            .into_iter()
            .map(OwnedOwnedObjectInfo::into_proto)
            .collect(),
        next_page_token,
    })
}

fn decode_page_token(page_token: &[u8]) -> Result<ObjectId> {
    Ok(ObjectId::new(page_token.try_into().unwrap()))
}

fn encode_page_token(page_token: ObjectId) -> Bytes {
    page_token.as_bytes().to_vec().into()
}

pub struct OwnedOwnedObjectInfo {
    pub owner: Address,
    pub object_id: ObjectId,
    pub version: Version,
    pub type_: StructTag,
}

impl OwnedOwnedObjectInfo {
    fn into_proto(self) -> OwnedObject {
        OwnedObject {
            owner: Some(self.owner.to_string()),
            object_id: Some(self.object_id.to_string()),
            version: Some(self.version),
            object_type: Some(self.type_.to_string()),
        }
    }
}
