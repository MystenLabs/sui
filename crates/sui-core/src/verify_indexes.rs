// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, bail, Result};
use sui_storage::{indexes::CoinInfo, IndexStore};
use sui_types::{base_types::ObjectInfo, object::Owner};
use tracing::info;
use typed_store::traits::Map;

use crate::authority::AuthorityStore;

/// This is a very expensive function that verifies some of the secondary indexes. This is done by
/// iterating through the live object set and recalculating these secodary indexes.
pub fn verify_indexes(database: Arc<AuthorityStore>, indexes: Arc<IndexStore>) -> Result<()> {
    info!("Begin running index verification checks");

    let mut owner_index = BTreeMap::new();
    let mut coin_index = BTreeMap::new();

    for object in database
        .perpetual_tables
        .objects
        .unbounded_iter()
        .flat_map(|(key, object)| database.perpetual_tables.object(&key, object).ok())
        .flatten()
    {
        let Owner::AddressOwner(owner) = object.owner else {
            continue;
        };

        // Owner Index Calculation
        let owner_index_key = (owner, object.id());
        let object_info = ObjectInfo::new(&object.compute_object_reference(), &object);

        owner_index.insert(owner_index_key, object_info);

        // Coin Index Calculation
        if let Some(type_tag) = object.coin_type_maybe() {
            let info =
                CoinInfo::from_object(&object).expect("already checked that this is a coin type");
            let key = (owner, type_tag.to_string(), object.id());

            coin_index.insert(key, info);
        }
    }

    // Verify Owner Index
    for (key, info) in indexes.tables().owner_index().unbounded_iter() {
        let calculated_info = owner_index.remove(&key).ok_or_else(|| {
            anyhow!(
                "owner_index: found extra, unexpected entry {:?}",
                (&key, &info)
            )
        })?;

        if calculated_info != info {
            bail!("owner_index: entry {key:?} is different: expected {calculated_info:?} found {info:?}");
        }
    }

    if !owner_index.is_empty() {
        bail!("owner_index: is missing entries: {owner_index:?}");
    }

    // Verify Coin Index
    for (key, info) in indexes.tables().coin_index().unbounded_iter() {
        let calculated_info = coin_index.remove(&key).ok_or_else(|| {
            anyhow!(
                "coin_index: found extra, unexpected entry {:?}",
                (&key, &info)
            )
        })?;

        if calculated_info != info {
            bail!("coin_index: entry {key:?} is different: expected {calculated_info:?} found {info:?}");
        }
    }

    if !coin_index.is_empty() {
        bail!("coin_index: is missing entries: {coin_index:?}");
    }

    info!("Finisedh running index verification checks");

    Ok(())
}
