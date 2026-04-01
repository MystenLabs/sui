// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared helpers for tiered object-cache stores (`ReadThroughStore`, `WriteThroughStore`).

use anyhow::{Error, Result};
use sui_types::object::Object;

use crate::{ObjectKey, ObjectStore, ObjectStoreWriter};

/// Read objects from `primary`, falling back to `secondary` for misses and
/// writing fetched results back into `primary`.
pub(super) fn get_objects_with_backfill<P, S>(
    primary: &P,
    secondary: &S,
    keys: &[ObjectKey],
) -> Result<Vec<Option<(Object, u64)>>, Error>
where
    P: ObjectStore + ObjectStoreWriter,
    S: ObjectStore,
{
    let mut objects = primary.get_objects(keys)?;
    let mut missing_keys = Vec::new();
    let mut missing_indexes = Vec::new();

    for (idx, object) in objects.iter().enumerate() {
        if object.is_none() {
            missing_keys.push(keys[idx].clone());
            missing_indexes.push(idx);
        }
    }

    if missing_keys.is_empty() {
        return Ok(objects);
    }

    let fetched_objects = secondary.get_objects(&missing_keys)?;
    debug_assert_eq!(missing_indexes.len(), fetched_objects.len());

    for ((idx, key), fetched_object) in missing_indexes
        .iter()
        .zip(missing_keys.iter())
        .zip(fetched_objects.into_iter())
    {
        if let Some((object, actual_version)) = fetched_object {
            primary.write_object(key, object.clone(), actual_version)?;
            objects[*idx] = Some((object, actual_version));
        }
    }

    Ok(objects)
}
