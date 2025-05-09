// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Weak;
use std::{collections::BTreeMap, sync::Arc};

use crate::jsonrpc_index::{CoinIndexKey2, CoinInfo, IndexStore};
use anyhow::{anyhow, bail, Result};
use sui_types::{base_types::ObjectInfo, object::Owner};
use tracing::info;
use typed_store::traits::Map;

use crate::authority::AuthorityState;
use crate::{
    authority::authority_store_tables::LiveObject, object_state_hasher::ObjectStateHashStore,
};

/// This is a very expensive function that verifies some of the secondary indexes. This is done by
/// iterating through the live object set and recalculating these secodary indexes.
pub fn verify_indexes(store: &dyn ObjectStateHashStore, indexes: Arc<IndexStore>) -> Result<()> {
    info!("Begin running index verification checks");

    let mut owner_index = BTreeMap::new();
    let mut coin_index = BTreeMap::new();

    tracing::info!("Reading live objects set");
    for object in store.iter_live_object_set(false) {
        let LiveObject::Normal(object) = object else {
            continue;
        };
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
            let key = CoinIndexKey2::new(owner, type_tag.to_string(), info.balance, object.id());

            coin_index.insert(key, info);
        }
    }

    tracing::info!("Live objects set is prepared, about to verify indexes");

    // Verify Owner Index
    for item in indexes.tables().owner_index().safe_iter() {
        let (key, info) = item?;
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
    tracing::info!("Owner index is good");

    // Verify Coin Index
    for item in indexes.tables().coin_index().safe_iter() {
        let (key, info) = item?;
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
    tracing::info!("Coin index is good");

    if !coin_index.is_empty() {
        bail!("coin_index: is missing entries: {coin_index:?}");
    }

    info!("Finished running index verification checks");

    Ok(())
}

// temporary code to repair the coin index. This should be removed in the next release
pub async fn fix_indexes(authority_state: Weak<AuthorityState>) -> Result<()> {
    let is_violation = |coin_index_key: &CoinIndexKey2, state: &Arc<AuthorityState>| -> bool {
        if let Some(object) = state
            .get_object_store()
            .get_object(&coin_index_key.object_id)
        {
            if matches!(object.owner, Owner::AddressOwner(real_owner_id) | Owner::ObjectOwner(real_owner_id) if coin_index_key.owner == real_owner_id)
            {
                return false;
            }
        }
        true
    };

    tracing::info!("Starting fixing coin index");
    // populate candidate list without locking. Some entries are benign
    let authority_state_clone = authority_state.clone();
    let candidates = tokio::task::spawn_blocking(move || {
        if let Some(authority) = authority_state_clone.upgrade() {
            let mut batch = vec![];
            if let Some(indexes) = &authority.indexes {
                for entry in indexes.tables().coin_index().safe_iter() {
                    let (coin_index_key, _) = entry?;
                    if is_violation(&coin_index_key, &authority) {
                        batch.push(coin_index_key);
                    }
                }
            }
            return Ok::<Vec<_>, anyhow::Error>(batch);
        }
        Ok(vec![])
    })
    .await??;

    if let Some(authority) = authority_state.upgrade() {
        if let Some(indexes) = &authority.indexes {
            for chunk in candidates.chunks(100) {
                let _locks = indexes
                    .caches
                    .locks
                    .acquire_locks(chunk.iter().map(|key| key.owner));
                let mut batch = vec![];
                for key in chunk {
                    if is_violation(key, &authority) {
                        batch.push(key);
                    }
                }
                let mut wb = indexes.tables().coin_index().batch();
                wb.delete_batch(indexes.tables().coin_index(), batch)?;
                wb.write()?;
            }
        }
    }
    tracing::info!("Finished fix for the coin index");
    Ok(())
}
