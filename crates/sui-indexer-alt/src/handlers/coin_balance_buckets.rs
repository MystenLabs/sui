// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, bail, Result};
use diesel_async::RunQueryDsl;
use sui_field_count::FieldCount;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_indexer_alt_schema::{
    objects::{StoredCoinBalanceBucket, StoredCoinOwnerKind},
    schema::coin_balance_buckets,
};
use sui_pg_db as db;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    full_checkpoint_content::CheckpointData,
    object::{Object, Owner},
    TypeTag,
};

pub(crate) struct CoinBalanceBuckets;

pub(crate) struct ProcessedCoinBalanceBucket {
    pub object_id: ObjectID,
    pub cp_sequence_number: u64,
    pub change: CoinBalanceBucketChangeKind,
}

pub(crate) enum CoinBalanceBucketChangeKind {
    Insert {
        owner_kind: StoredCoinOwnerKind,
        owner_id: SuiAddress,
        coin_type: TypeTag,
        balance_bucket: i16,
    },
    Delete,
}

impl Processor for CoinBalanceBuckets {
    const NAME: &'static str = "coin_balance_buckets";
    type Value = ProcessedCoinBalanceBucket;

    // TODO: We need to add tests for this function.
    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let checkpoint_input_objects = checkpoint.checkpoint_input_objects();
        let latest_live_output_objects: BTreeMap<_, _> = checkpoint
            .latest_live_output_objects()
            .into_iter()
            .map(|o| (o.id(), o))
            .collect();
        let mut values: BTreeMap<ObjectID, Self::Value> = BTreeMap::new();
        for (object_id, input_object) in checkpoint_input_objects.iter() {
            // This loop processes all coins that were owned by a single address prior to the checkpoint,
            // but is now deleted or wrapped after the checkpoint.
            if !input_object.is_coin() {
                continue;
            }
            if get_coin_owner(input_object).is_none() {
                continue;
            }
            if latest_live_output_objects.contains_key(object_id) {
                continue;
            }
            values.insert(
                *object_id,
                ProcessedCoinBalanceBucket {
                    object_id: *object_id,
                    cp_sequence_number,
                    change: CoinBalanceBucketChangeKind::Delete,
                },
            );
        }
        for (object_id, output_object) in latest_live_output_objects.iter() {
            let Some(coin_type) = output_object.coin_type_maybe() else {
                continue;
            };

            let (input_bucket, input_owner) = match checkpoint_input_objects.get(object_id) {
                Some(input_object) => {
                    let bucket = get_coin_balance_bucket(input_object)?;
                    let owner = get_coin_owner(input_object);
                    (Some(bucket), owner)
                }
                None => (None, None),
            };

            let output_balance_bucket = get_coin_balance_bucket(output_object)?;
            let output_owner = get_coin_owner(output_object);

            match (input_owner, output_owner) {
                (Some(_), None) => {
                    // In this case, the coin was owned by a single address prior to the checkpoint,
                    // but is now either shared or immutable after the checkpoint. We treat this the same
                    // as if the coin was deleted, from the perspective of the balance bucket.
                    values.insert(
                        *object_id,
                        ProcessedCoinBalanceBucket {
                            object_id: *object_id,
                            cp_sequence_number,
                            change: CoinBalanceBucketChangeKind::Delete,
                        },
                    );
                }
                (_, Some(new_owner))
                    if input_owner != output_owner
                        || input_bucket != Some(output_balance_bucket) =>
                {
                    // In this case, the coin is still owned by a single address after the checkpoint,
                    // but either the owner or the balance bucket has changed. This also includes the case
                    // where the coin did not exist prior to the checkpoint, and is now created/unwrapped.
                    values.insert(
                        *object_id,
                        ProcessedCoinBalanceBucket {
                            object_id: *object_id,
                            cp_sequence_number,
                            change: CoinBalanceBucketChangeKind::Insert {
                                owner_kind: new_owner.0,
                                owner_id: new_owner.1,
                                coin_type,
                                balance_bucket: output_balance_bucket,
                            },
                        },
                    );
                }
                _ => {}
            }
        }

        Ok(values.into_values().collect())
    }
}

#[async_trait::async_trait]
impl Handler for CoinBalanceBuckets {
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        let values = values
            .iter()
            .map(|v| v.try_into())
            .collect::<Result<Vec<StoredCoinBalanceBucket>>>()?;
        Ok(diesel::insert_into(coin_balance_buckets::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}

impl FieldCount for ProcessedCoinBalanceBucket {
    const FIELD_COUNT: usize = StoredCoinBalanceBucket::FIELD_COUNT;
}

impl TryInto<StoredCoinBalanceBucket> for &ProcessedCoinBalanceBucket {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<StoredCoinBalanceBucket> {
        match &self.change {
            CoinBalanceBucketChangeKind::Insert {
                owner_kind,
                owner_id,
                coin_type,
                balance_bucket,
            } => {
                let serialized_coin_type = bcs::to_bytes(&coin_type)
                    .map_err(|_| anyhow!("Failed to serialize type for {}", self.object_id))?;
                Ok(StoredCoinBalanceBucket {
                    object_id: self.object_id.to_vec(),
                    cp_sequence_number: self.cp_sequence_number as i64,
                    owner_kind: Some(*owner_kind),
                    owner_id: Some(owner_id.to_vec()),
                    coin_type: Some(serialized_coin_type),
                    coin_balance_bucket: Some(*balance_bucket),
                })
            }
            CoinBalanceBucketChangeKind::Delete => Ok(StoredCoinBalanceBucket {
                object_id: self.object_id.to_vec(),
                cp_sequence_number: self.cp_sequence_number as i64,
                owner_kind: None,
                owner_id: None,
                coin_type: None,
                coin_balance_bucket: None,
            }),
        }
    }
}

/// Get the owner kind and address of a coin, if it is owned by a single address,
/// either through fast-path ownership or ConsensusV2 ownership.
pub(crate) fn get_coin_owner(object: &Object) -> Option<(StoredCoinOwnerKind, SuiAddress)> {
    match object.owner() {
        Owner::AddressOwner(owner_id) => Some((StoredCoinOwnerKind::Fastpath, *owner_id)),
        Owner::ConsensusV2 { authenticator, .. } => Some((
            StoredCoinOwnerKind::Consensus,
            *authenticator.as_single_owner(),
        )),
        Owner::Immutable | Owner::ObjectOwner(_) | Owner::Shared { .. } => None,
    }
}

pub(crate) fn get_coin_balance_bucket(coin: &Object) -> anyhow::Result<i16> {
    let Some(coin) = coin.as_coin_maybe() else {
        // TODO: We should make this an invariant violation.
        bail!("Failed to deserialize Coin for {}", coin.id());
    };
    let balance = coin.balance.value();
    if balance == 0 {
        return Ok(0);
    }
    let bucket = balance.ilog10() as i16;
    Ok(bucket)
}
