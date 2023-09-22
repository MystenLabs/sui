// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{
    extract::{Path, State},
    routing::get,
    Router,
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    object::Object,
    storage::ObjectStore2,
};

use crate::{AppError, Bcs};

pub const GET_OBJECT_PATH: &str = "/objects/:object_id";

pub async fn get_object<S>(
    Path(object_id): Path<ObjectID>,
    State(store): State<S>,
) -> Result<Bcs<Object>, AppError>
where
    S: ObjectStore2,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let object = store
        .get_object(&object_id)?
        .ok_or_else(|| anyhow::anyhow!("object not found"))?;

    Ok(Bcs(object))
}

pub const GET_OBJECT_WITH_VERSION_PATH: &str = "/objects/:object_id/version/:version";

pub async fn get_object_with_version<S>(
    Path((object_id, version)): Path<(ObjectID, SequenceNumber)>,
    State(store): State<S>,
) -> Result<Bcs<Object>, AppError>
where
    S: ObjectStore2,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let object = store
        .get_object_by_key(&object_id, version)?
        .ok_or_else(|| anyhow::anyhow!("object not found"))?;

    Ok(Bcs(object))
}

pub(super) fn router<S>(store: S) -> Router
where
    S: ObjectStore2 + Clone + Send + Sync + 'static,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    Router::new()
        .route(GET_OBJECT_PATH, get(get_object::<S>))
        .route(
            GET_OBJECT_WITH_VERSION_PATH,
            get(get_object_with_version::<S>),
        )
        .with_state(store)
}
