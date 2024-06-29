// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::reader::StateReader;
use crate::Page;
use crate::{accept::AcceptFormat, response::ResponseContent, Result};
use axum::extract::Query;
use axum::extract::{Path, State};
use itertools::Itertools;
use sui_sdk2::types::{Address, Object, ObjectId};
use sui_types::storage::ObjectKey;
use tap::Pipe;

pub const LIST_ACCOUNT_OWNED_OBJECTS_PATH: &str = "/accounts/:account/objects";

pub async fn list_account_owned_objects(
    Path(address): Path<Address>,
    Query(parameters): Query<ListAccountOwnedObjectsQueryParameters>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<Page<Object, ObjectId>> {
    let limit = parameters.limit();
    let start = parameters.start();

    let mut object_keys = state
        .inner()
        .account_owned_objects_info_iter(address.into(), start)?
        .map(|info| ObjectKey(info.object_id, info.version))
        .take(limit + 1)
        .collect::<Vec<_>>();

    let cursor = if object_keys.len() > limit {
        // SAFETY: We've already verified that object_keys is greater than limit, which is
        // gaurenteed to be >= 1.
        object_keys.pop().unwrap().0.pipe(ObjectId::from).pipe(Some)
    } else {
        None
    };

    let objects = state
        .inner()
        .multi_get_objects_by_key(&object_keys)?
        .into_iter()
        .flatten()
        .map(Into::into)
        .collect_vec();

    match accept {
        AcceptFormat::Json => ResponseContent::Json(objects),
        AcceptFormat::Bcs => ResponseContent::Bcs(objects),
    }
    .pipe(|entries| Page { entries, cursor })
    .pipe(Ok)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ListAccountOwnedObjectsQueryParameters {
    pub limit: Option<u32>,
    pub start: Option<ObjectId>,
}

impl ListAccountOwnedObjectsQueryParameters {
    pub fn limit(&self) -> usize {
        self.limit
            .map(|l| (l as usize).clamp(1, crate::MAX_PAGE_SIZE))
            .unwrap_or(crate::DEFAULT_PAGE_SIZE)
    }

    pub fn start(&self) -> Option<sui_types::base_types::ObjectID> {
        self.start.map(Into::into)
    }
}
