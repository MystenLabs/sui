// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::node::v2alpha::AccountObject;
use crate::proto::node::v2alpha::ListAccountObjectsRequest;
use crate::proto::node::v2alpha::ListAccountObjectsResponse;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_sdk_types::Address;
use sui_sdk_types::Version;
use sui_sdk_types::{ObjectId, StructTag};
use sui_types::sui_sdk_types_conversions::struct_tag_core_to_sdk;
use tap::Pipe;

impl RpcService {
    pub fn list_account_objects(
        &self,
        request: ListAccountObjectsRequest,
    ) -> Result<ListAccountObjectsResponse> {
        let indexes = self
            .reader
            .inner()
            .indexes()
            .ok_or_else(RpcError::not_found)?;

        let owner: Address = request
            .owner
            .as_ref()
            .ok_or_else(|| RpcError::new(tonic::Code::InvalidArgument, "missing owner"))?
            .try_into()
            .map_err(|e| {
                RpcError::new(tonic::Code::InvalidArgument, format!("invalid parent: {e}"))
            })?;

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
            .map(|info| {
                AccountOwnedObjectInfo {
                    owner: info.owner.into(),
                    object_id: info.object_id.into(),
                    version: info.version.into(),
                    type_: struct_tag_core_to_sdk(info.type_.into())?,
                }
                .pipe(Ok)
            })
            .collect::<Result<Vec<_>>>()?;

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

        Ok(ListAccountObjectsResponse {
            objects: object_info
                .into_iter()
                .map(AccountOwnedObjectInfo::into_proto)
                .collect(),
            next_page_token,
        })
    }
}

fn decode_page_token(page_token: &str) -> Result<ObjectId> {
    use base64::prelude::BASE64_STANDARD;
    use base64::Engine;

    let bytes = BASE64_STANDARD.decode(page_token).unwrap();
    Ok(ObjectId::new(bytes.try_into().unwrap()))
}

fn encode_page_token(page_token: ObjectId) -> String {
    use base64::prelude::BASE64_STANDARD;
    use base64::Engine;

    BASE64_STANDARD.encode(page_token.as_bytes())
}

pub struct AccountOwnedObjectInfo {
    pub owner: Address,
    pub object_id: ObjectId,
    pub version: Version,
    pub type_: StructTag,
}

impl AccountOwnedObjectInfo {
    fn into_proto(self) -> AccountObject {
        AccountObject {
            owner: Some(self.owner.into()),
            object_id: Some(self.object_id.into()),
            version: Some(self.version),
            object_type: Some(self.type_.into()),
        }
    }
}
