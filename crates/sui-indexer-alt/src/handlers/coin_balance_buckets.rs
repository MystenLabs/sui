// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Result, anyhow, bail, ensure};
use diesel::prelude::QueryableByName;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    FieldCount,
    pipeline::{Processor, concurrent::Handler},
    postgres::{Connection, Db},
    types::{
        TypeTag,
        base_types::{ObjectID, SuiAddress},
        full_checkpoint_content::CheckpointData,
        object::{Object, Owner},
    },
};
use sui_indexer_alt_schema::{
    objects::{
        StoredCoinBalanceBucket, StoredCoinBalanceBucketDeletionReference, StoredCoinOwnerKind,
    },
    schema::{coin_balance_buckets, coin_balance_buckets_deletion_reference},
};

use super::checkpoint_input_objects;
use async_trait::async_trait;

/// This handler is used to track the balance buckets of address-owned coins.
/// The balance bucket is calculated using log10 of the coin balance.
/// Whenever a coin object's presence, owner or balance bucket changes,
/// we will insert a new row into the `coin_balance_buckets` table.
/// A Delete record will be inserted when a coin object is no longer present or no longer owned by an address.
pub(crate) struct CoinBalanceBuckets;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProcessedCoinBalanceBucket {
    pub object_id: ObjectID,
    pub cp_sequence_number: u64,
    pub change: CoinBalanceBucketChangeKind,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CoinBalanceBucketChangeKind {
    Upsert {
        owner_kind: StoredCoinOwnerKind,
        owner_id: SuiAddress,
        coin_type: TypeTag,
        balance_bucket: i16,
        /// Indicates whether the coin was created/unwrapped in this checkpoint.
        created: bool,
    },
    Delete,
}

#[async_trait]
impl Processor for CoinBalanceBuckets {
    const NAME: &'static str = "coin_balance_buckets";
    type Value = ProcessedCoinBalanceBucket;

    async fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let checkpoint_input_objects = checkpoint_input_objects(checkpoint)?;
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
                            change: CoinBalanceBucketChangeKind::Upsert {
                                owner_kind: new_owner.0,
                                owner_id: new_owner.1,
                                coin_type,
                                balance_bucket: output_balance_bucket,
                                created: input_owner.is_none(),
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

#[async_trait]
impl Handler for CoinBalanceBuckets {
    type Store = Db;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        let stored = values
            .iter()
            .map(|v| v.try_into())
            .collect::<Result<Vec<StoredCoinBalanceBucket>>>()?;

        let mut references = Vec::new();
        for value in values {
            match &value.change {
                CoinBalanceBucketChangeKind::Upsert { created, .. } => {
                    if !created {
                        references.push(StoredCoinBalanceBucketDeletionReference {
                            object_id: value.object_id.to_vec(),
                            cp_sequence_number: value.cp_sequence_number as i64,
                        });
                    }
                }
                // Store record of current version to delete previous version, and another to delete
                // itself. When pruning, the deletion record will not be pruned in the
                // `value.cp_sequence_number` checkpoint, but the next one.
                CoinBalanceBucketChangeKind::Delete => {
                    references.push(StoredCoinBalanceBucketDeletionReference {
                        object_id: value.object_id.to_vec(),
                        cp_sequence_number: value.cp_sequence_number as i64,
                    });
                    references.push(StoredCoinBalanceBucketDeletionReference {
                        object_id: value.object_id.to_vec(),
                        cp_sequence_number: value.cp_sequence_number as i64 + 1,
                    });
                }
            }
        }

        let count = diesel::insert_into(coin_balance_buckets::table)
            .values(&stored)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?;
        let deleted_refs = if !references.is_empty() {
            diesel::insert_into(coin_balance_buckets_deletion_reference::table)
                .values(&references)
                .on_conflict_do_nothing()
                .execute(conn)
                .await?
        } else {
            0
        };

        Ok(count + deleted_refs)
    }

    // TODO: Add tests for this function.
    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection<'a>,
    ) -> anyhow::Result<usize> {
        // This query first deletes from coin_balance_buckets_deletion_reference and computes
        // predecessors, then deletes from coin_balance_buckets using the precomputed predecessor
        // information. The inline compute avoids HashAggregate operations and the ensuing
        // materialization overhead.
        //
        // This works best on under 1.5 million object changes, roughly 15k checkpoints. Performance
        // degrades sharply beyond this, since the planner switches to hash joins and full table
        // scans. A HashAggregate approach interestingly becomes more performant in this scenario.
        //
        // If the first call to prune succeeds, subsequent calls will find no records to delete from
        // coin_balance_buckets_deletion_reference, and consequently no records to delete from the
        // main table. Pruning is thus idempotent after the initial run.
        //
        // TODO: use sui_sql_macro's query!
        let query = format!(
            "
            -- Delete reference records and return immediate predecessor refs to the main table.
            WITH deletion_refs AS (
                DELETE FROM
                    coin_balance_buckets_deletion_reference dr
                WHERE
                    {} <= cp_sequence_number AND cp_sequence_number < {}
                RETURNING
                    object_id, (
                    SELECT
                        cb.cp_sequence_number
                    FROM
                        coin_balance_buckets cb
                    WHERE
                        dr.object_id = cb.object_id
                    AND cb.cp_sequence_number < dr.cp_sequence_number
                    ORDER BY
                        cb.cp_sequence_number DESC
                    LIMIT
                        1
                    ) AS cp_sequence_number
            ),
            deleted_coins AS (
                DELETE FROM
                    coin_balance_buckets cb
                USING
                    deletion_refs dr
                WHERE
                    cb.object_id = dr.object_id
                AND cb.cp_sequence_number = dr.cp_sequence_number
                RETURNING
                    cb.object_id
            )
            SELECT
                (SELECT COUNT(*) FROM deleted_coins) AS deleted_coins,
                (SELECT COUNT(*) FROM deletion_refs) AS deleted_refs
            ",
            from, to_exclusive
        );

        #[derive(QueryableByName)]
        struct CountResult {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            deleted_coins: i64,
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            deleted_refs: i64,
        }

        let CountResult {
            deleted_coins,
            deleted_refs,
        } = diesel::sql_query(query)
            .get_result::<CountResult>(conn)
            .await?;

        ensure!(
            deleted_coins == deleted_refs,
            "Deleted coins count ({deleted_coins}) does not match deleted refs count ({deleted_refs})",
        );

        Ok((deleted_coins + deleted_refs) as usize)
    }
}

impl FieldCount for ProcessedCoinBalanceBucket {
    const FIELD_COUNT: usize = StoredCoinBalanceBucket::FIELD_COUNT;
}

impl TryInto<StoredCoinBalanceBucket> for &ProcessedCoinBalanceBucket {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<StoredCoinBalanceBucket> {
        match &self.change {
            CoinBalanceBucketChangeKind::Upsert {
                owner_kind,
                owner_id,
                coin_type,
                balance_bucket,
                created: _,
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
/// either through fast-path ownership or consensus ownership.
pub(crate) fn get_coin_owner(object: &Object) -> Option<(StoredCoinOwnerKind, SuiAddress)> {
    match object.owner() {
        Owner::AddressOwner(owner_id) => Some((StoredCoinOwnerKind::Fastpath, *owner_id)),
        Owner::ConsensusAddressOwner { owner, .. } => {
            Some((StoredCoinOwnerKind::Consensus, *owner))
        }
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use diesel::QueryDsl;
    use sui_indexer_alt_framework::{
        Indexer,
        types::{
            base_types::{MoveObjectType, ObjectID, SequenceNumber, SuiAddress, dbg_addr},
            digests::TransactionDigest,
            gas_coin::GAS,
            object::{MoveObject, Object},
            test_checkpoint_data_builder::TestCheckpointDataBuilder,
        },
    };
    use sui_indexer_alt_schema::MIGRATIONS;
    use sui_protocol_config::ProtocolConfig;

    // Get all balance buckets from the database, sorted by object_id and cp_sequence_number.
    async fn get_all_balance_buckets(conn: &mut Connection<'_>) -> Vec<StoredCoinBalanceBucket> {
        coin_balance_buckets::table
            .order_by((
                coin_balance_buckets::object_id,
                coin_balance_buckets::cp_sequence_number,
            ))
            .load(conn)
            .await
            .unwrap()
    }

    #[test]
    fn test_get_coin_balance_bucket() {
        let id = ObjectID::random();

        // Test coin with 0 balance
        let zero_coin = Object::with_id_owner_gas_for_testing(id, SuiAddress::ZERO, 0);
        assert_eq!(get_coin_balance_bucket(&zero_coin).unwrap(), 0);

        // Test coin with balance 1 (10^0)
        let one_coin = Object::with_id_owner_gas_for_testing(id, SuiAddress::ZERO, 1);
        assert_eq!(get_coin_balance_bucket(&one_coin).unwrap(), 0);

        // Test coin with balance 100 (10^2)
        let hundred_coin = Object::with_id_owner_gas_for_testing(id, SuiAddress::ZERO, 100);
        assert_eq!(get_coin_balance_bucket(&hundred_coin).unwrap(), 2);

        // Test coin with balance 1000000 (10^6)
        let million_coin = Object::with_id_owner_gas_for_testing(id, SuiAddress::ZERO, 1000000);
        assert_eq!(get_coin_balance_bucket(&million_coin).unwrap(), 6);

        // The type of this object is a staked SUI, not a coin.
        let invalid_coin = unsafe {
            Object::new_move(
                MoveObject::new_from_execution(
                    MoveObjectType::staked_sui(),
                    false,
                    SequenceNumber::new(),
                    bcs::to_bytes(&Object::new_gas_for_testing()).unwrap(),
                    &ProtocolConfig::get_for_max_version_UNSAFE(),
                    /* system_mutation */ false,
                )
                .unwrap(),
                Owner::AddressOwner(SuiAddress::ZERO),
                TransactionDigest::ZERO,
            )
        };
        assert!(get_coin_balance_bucket(&invalid_coin).is_err());
    }

    #[test]
    fn test_get_coin_owner() {
        let id = ObjectID::random();
        let addr1 = SuiAddress::random_for_testing_only();
        let addr_owned = Object::with_id_owner_for_testing(id, addr1);
        assert_eq!(
            get_coin_owner(&addr_owned),
            Some((StoredCoinOwnerKind::Fastpath, addr1))
        );

        // Test object owner (should return None)
        let obj_owned = Object::with_object_owner_for_testing(id, addr1.into());
        assert_eq!(get_coin_owner(&obj_owned), None);

        // Test shared owner (should return None)
        let shared = Object::shared_for_testing();
        assert_eq!(get_coin_owner(&shared), None);

        // Test immutable owner (should return None)
        let immutable = Object::immutable_with_id_for_testing(id);
        assert_eq!(get_coin_owner(&immutable), None);

        let consensus_v2 = Object::with_id_owner_version_for_testing(
            id,
            SequenceNumber::new(),
            Owner::ConsensusAddressOwner {
                start_version: SequenceNumber::new(),
                owner: addr1,
            },
        );
        assert_eq!(
            get_coin_owner(&consensus_v2),
            Some((StoredCoinOwnerKind::Consensus, addr1))
        );
    }

    #[tokio::test]
    async fn test_process_coin_balance_buckets_new_sui_coin() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_sui_object(0, 0)
            .create_sui_object(1, 100)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        assert_eq!(values.len(), 2);
        assert!(values.iter().any(|v| matches!(
            v.change,
            CoinBalanceBucketChangeKind::Upsert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 0,
                created: true,
                ..
            }
        )));
        assert!(values.iter().any(|v| matches!(
            v.change,
            CoinBalanceBucketChangeKind::Upsert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 2,
                created: true,
                ..
            }
        )));
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 2);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 2);
        let rows_pruned = CoinBalanceBuckets.prune(0, 1, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 0);
    }

    #[tokio::test]
    async fn test_process_coin_balance_buckets_new_other_coin() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);
        let coin_type = TypeTag::from_str("0x0::a::b").unwrap();
        builder = builder
            .start_transaction(0)
            .create_coin_object(0, 0, 10, coin_type.clone())
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(
            &values[0].change,
            &CoinBalanceBucketChangeKind::Upsert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 1,
                coin_type: coin_type.clone(),
                owner_id: TestCheckpointDataBuilder::derive_address(0),
                created: true,
            }
        );
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 1);
        let rows_pruned = CoinBalanceBuckets.prune(0, 1, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 0);
    }

    #[tokio::test]
    async fn test_process_coin_balance_buckets_balance_change() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_sui_object(0, 10010)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        assert_eq!(values.len(), 1);
        // Checkpoint 0 creates coin object 0.
        assert_eq!(
            values[0].change,
            CoinBalanceBucketChangeKind::Upsert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 4,
                coin_type: GAS::type_tag(),
                owner_id: TestCheckpointDataBuilder::derive_address(0),
                created: true,
            }
        );
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 1);

        // Transfer 10 MIST, balance goes from 10010 to 10000.
        // The balance bucket for the original coin does not change.
        // We should only see the creation of the new coin in the processed results.
        builder = builder
            .start_transaction(0)
            .transfer_coin_balance(0, 1, 1, 10)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        assert_eq!(values.len(), 1);
        // Checkpoint 1 creates coin object 1.
        assert_eq!(
            values[0].change,
            CoinBalanceBucketChangeKind::Upsert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 1,
                coin_type: GAS::type_tag(),
                owner_id: TestCheckpointDataBuilder::derive_address(1),
                created: true
            }
        );
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 2);

        // Nothing to prune because the two coins in the table have not been updated since creation.
        let rows_pruned = CoinBalanceBuckets.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 0);

        // Transfer 1 MIST, balance goes from 10000 to 9999.
        // The balance bucket changes, we should see a change, both for the old owner and the new owner.
        builder = builder
            .start_transaction(0)
            .transfer_coin_balance(0, 2, 1, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        assert_eq!(values.len(), 2);
        // Checkpoint 2 creates coin object 2, and mutates coin object 0.
        assert!(values.iter().any(|v| v.change
            == CoinBalanceBucketChangeKind::Upsert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 3,
                coin_type: GAS::type_tag(),
                owner_id: TestCheckpointDataBuilder::derive_address(0),
                created: false,
            }));
        assert!(values.iter().any(|v| v.change
            == CoinBalanceBucketChangeKind::Upsert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 0,
                coin_type: GAS::type_tag(),
                owner_id: TestCheckpointDataBuilder::derive_address(1),
                created: true,
            }));
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        // 2 inserts to main table, only 1 from the transfer - creations don't emit rows on ref
        // table.
        assert_eq!(rows_inserted, 3);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 4);

        let rows_pruned = CoinBalanceBuckets.prune(2, 3, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 2);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 3);
        assert_eq!(
            all_balance_buckets[0],
            StoredCoinBalanceBucket {
                object_id: TestCheckpointDataBuilder::derive_object_id(0).to_vec(),
                cp_sequence_number: 2,
                owner_kind: Some(StoredCoinOwnerKind::Fastpath),
                owner_id: Some(TestCheckpointDataBuilder::derive_address(0).to_vec()),
                coin_type: Some(bcs::to_bytes(&GAS::type_tag()).unwrap()),
                coin_balance_bucket: Some(3),
            }
        );
        assert_eq!(
            all_balance_buckets[1],
            StoredCoinBalanceBucket {
                object_id: TestCheckpointDataBuilder::derive_object_id(1).to_vec(),
                cp_sequence_number: 1,
                owner_kind: Some(StoredCoinOwnerKind::Fastpath),
                owner_id: Some(TestCheckpointDataBuilder::derive_address(1).to_vec()),
                coin_type: Some(bcs::to_bytes(&GAS::type_tag()).unwrap()),
                coin_balance_bucket: Some(1),
            }
        );
        assert_eq!(
            all_balance_buckets[2],
            StoredCoinBalanceBucket {
                object_id: TestCheckpointDataBuilder::derive_object_id(2).to_vec(),
                cp_sequence_number: 2,
                owner_kind: Some(StoredCoinOwnerKind::Fastpath),
                owner_id: Some(TestCheckpointDataBuilder::derive_address(1).to_vec()),
                coin_type: Some(bcs::to_bytes(&GAS::type_tag()).unwrap()),
                coin_balance_bucket: Some(0),
            }
        );
    }

    #[tokio::test]
    async fn test_process_coin_balance_buckets_coin_deleted() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].change, CoinBalanceBucketChangeKind::Delete);
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        // 1 insertion to main table, 2 to ref table because of delete.
        assert_eq!(rows_inserted, 3);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 2);
        assert_eq!(
            all_balance_buckets[1],
            StoredCoinBalanceBucket {
                object_id: TestCheckpointDataBuilder::derive_object_id(0).to_vec(),
                cp_sequence_number: 1,
                owner_kind: None,
                owner_id: None,
                coin_type: None,
                coin_balance_bucket: None,
            }
        );

        let rows_pruned = CoinBalanceBuckets.prune(0, 2, &mut conn).await.unwrap();
        let sentinel_rows_pruned = CoinBalanceBuckets.prune(2, 3, &mut conn).await.unwrap();
        assert_eq!(rows_pruned + sentinel_rows_pruned, 4);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 0);
    }

    #[tokio::test]
    async fn test_process_coin_balance_buckets_owner_change() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_sui_object(0, 100)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(
            values[0].change,
            CoinBalanceBucketChangeKind::Upsert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 2,
                coin_type: GAS::type_tag(),
                owner_id: TestCheckpointDataBuilder::derive_address(1),
                created: false,
            }
        );
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 2);

        let rows_pruned = CoinBalanceBuckets.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 2);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 1);
        assert_eq!(
            all_balance_buckets[0],
            StoredCoinBalanceBucket {
                object_id: TestCheckpointDataBuilder::derive_object_id(0).to_vec(),
                cp_sequence_number: 1,
                owner_kind: Some(StoredCoinOwnerKind::Fastpath),
                owner_id: Some(TestCheckpointDataBuilder::derive_address(1).to_vec()),
                coin_type: Some(bcs::to_bytes(&GAS::type_tag()).unwrap()),
                coin_balance_bucket: Some(2),
            }
        );
    }

    #[tokio::test]
    async fn test_process_coin_balance_buckets_object_owned() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);

        // So this is considered as a delete.
        builder = builder
            .start_transaction(0)
            .change_object_owner(0, Owner::ObjectOwner(dbg_addr(1)))
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].change, CoinBalanceBucketChangeKind::Delete);
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 3);

        let rows_pruned = CoinBalanceBuckets.prune(0, 2, &mut conn).await.unwrap();
        let sentinel_rows_pruned = CoinBalanceBuckets.prune(2, 3, &mut conn).await.unwrap();
        assert_eq!(rows_pruned + sentinel_rows_pruned, 4);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 0);
    }

    #[tokio::test]
    async fn test_process_coin_balance_buckets_wrap_and_prune_after_unwrap() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);

        // Create a coin in checkpoint 0
        builder = builder
            .start_transaction(0)
            .create_sui_object(0, 100)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);

        // Wrap the coin in checkpoint 1
        builder = builder
            .start_transaction(0)
            .wrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].change, CoinBalanceBucketChangeKind::Delete);
        // 1 insertion to main table, 2 to ref table because of wrap.
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 3);

        // Unwrap the coin in checkpoint 2
        builder = builder
            .start_transaction(0)
            .unwrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = CoinBalanceBuckets
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(
            values[0].change,
            CoinBalanceBucketChangeKind::Upsert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 2,
                coin_type: GAS::type_tag(),
                owner_id: TestCheckpointDataBuilder::derive_address(0),
                created: true,
            }
        );
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);

        // Prune after unwrap
        let rows_pruned = CoinBalanceBuckets.prune(0, 3, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 4);

        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 1);
        assert_eq!(all_balance_buckets[0].cp_sequence_number, 2);
        assert!(all_balance_buckets[0].owner_kind.is_some());
    }

    /// Three coins are created in checkpoint 0. All are transferred in checkpoint 1, and
    /// transferred back in checkpoint 2. Prune `[2, 3)` first, then `[1, 2)`, finally `[0, 1)`.
    #[tokio::test]
    async fn test_process_coin_balance_buckets_out_of_order_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);

        // Create three coins in checkpoint 0
        builder = builder
            .start_transaction(0)
            .create_sui_object(0, 100)
            .create_sui_object(1, 1000)
            .create_sui_object(2, 10000)
            .finish_transaction();
        let checkpoint0 = builder.build_checkpoint();
        let result = CoinBalanceBuckets
            .process(&Arc::new(checkpoint0))
            .await
            .unwrap();
        assert_eq!(result.len(), 3);
        let rows_inserted = CoinBalanceBuckets::commit(&result, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 3);

        // Transfer all coins to address 1 in checkpoint 1
        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .transfer_object(1, 1)
            .transfer_object(2, 1)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        let result = CoinBalanceBuckets
            .process(&Arc::new(checkpoint1))
            .await
            .unwrap();
        assert_eq!(result.len(), 3);
        let rows_inserted = CoinBalanceBuckets::commit(&result, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 6);

        // Transfer all coins back to address 0 in checkpoint 2
        builder = builder
            .start_transaction(1)
            .transfer_object(0, 0)
            .transfer_object(1, 0)
            .transfer_object(2, 0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        let result = CoinBalanceBuckets
            .process(&Arc::new(checkpoint2))
            .await
            .unwrap();
        assert_eq!(result.len(), 3);
        let rows_inserted = CoinBalanceBuckets::commit(&result, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 6);

        // Each of the 3 coins will have two deletion references, one at cp_sequence_number 1, another at 2.
        let all_deletion_references = coin_balance_buckets_deletion_reference::table
            .load::<StoredCoinBalanceBucketDeletionReference>(&mut conn)
            .await
            .unwrap();
        assert_eq!(all_deletion_references.len(), 6);
        for reference in &all_deletion_references {
            assert!(reference.cp_sequence_number == 1 || reference.cp_sequence_number == 2);
        }

        // Prune [2, 3) first (reverse order)
        let rows_pruned = CoinBalanceBuckets.prune(2, 3, &mut conn).await.unwrap();
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        let all_deletion_references = coin_balance_buckets_deletion_reference::table
            .load::<StoredCoinBalanceBucketDeletionReference>(&mut conn)
            .await
            .unwrap();

        // Each coin should have two entries, with cp_sequence_number being either 0 or 2.
        for bucket in &all_balance_buckets {
            assert!(bucket.cp_sequence_number == 0 || bucket.cp_sequence_number == 2);
        }
        assert_eq!(rows_pruned, 6);
        assert_eq!(all_balance_buckets.len(), 6);
        // References at cp_sequence_number 2 should be pruned.
        for reference in &all_deletion_references {
            assert!(reference.cp_sequence_number != 2);
        }

        // Prune [1, 2) next
        let rows_pruned = CoinBalanceBuckets.prune(1, 2, &mut conn).await.unwrap();
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        let all_deletion_references = coin_balance_buckets_deletion_reference::table
            .load::<StoredCoinBalanceBucketDeletionReference>(&mut conn)
            .await
            .unwrap();

        // Each coin should have a single entry with cp_sequence_number 2.
        for bucket in &all_balance_buckets {
            assert_eq!(bucket.cp_sequence_number, 2);
        }
        assert_eq!(rows_pruned, 6);
        assert_eq!(all_balance_buckets.len(), 3);
        // References at cp_sequence_number 1 should be pruned.
        for reference in &all_deletion_references {
            assert_eq!(reference.cp_sequence_number, 0);
        }
    }

    /// Test concurrent pruning operations to ensure thread safety and data consistency.
    /// This test creates the same scenario as test_process_coin_balance_buckets_out_of_order_pruning but runs
    /// multiple pruning operations concurrently.
    #[tokio::test]
    async fn test_process_coin_balance_buckets_concurrent_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);

        // Create the same scenario as the out-of-order test
        builder = builder
            .start_transaction(0)
            .create_sui_object(0, 100)
            .create_sui_object(1, 1000)
            .create_sui_object(2, 10000)
            .finish_transaction();
        let checkpoint0 = builder.build_checkpoint();
        let result = CoinBalanceBuckets
            .process(&Arc::new(checkpoint0))
            .await
            .unwrap();
        CoinBalanceBuckets::commit(&result, &mut conn)
            .await
            .unwrap();

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .transfer_object(1, 1)
            .transfer_object(2, 1)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        let result = CoinBalanceBuckets
            .process(&Arc::new(checkpoint1))
            .await
            .unwrap();
        CoinBalanceBuckets::commit(&result, &mut conn)
            .await
            .unwrap();

        builder = builder
            .start_transaction(1)
            .transfer_object(0, 0)
            .transfer_object(1, 0)
            .transfer_object(2, 0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        let result = CoinBalanceBuckets
            .process(&Arc::new(checkpoint2))
            .await
            .unwrap();
        CoinBalanceBuckets::commit(&result, &mut conn)
            .await
            .unwrap();

        // Verify initial state
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 9); // 3 coins × 3 checkpoints
        let all_deletion_references = coin_balance_buckets_deletion_reference::table
            .load::<StoredCoinBalanceBucketDeletionReference>(&mut conn)
            .await
            .unwrap();
        assert_eq!(all_deletion_references.len(), 6);

        // Run concurrent pruning operations
        let mut handles = Vec::new();

        // Clone the store so each spawned task can own its own connection
        let store = indexer.store().clone();

        // Spawn pruning [2, 3)
        let store1 = store.clone();
        handles.push(tokio::spawn(async move {
            let mut conn = store1.connect().await.unwrap();
            CoinBalanceBuckets.prune(2, 3, &mut conn).await
        }));

        // Spawn pruning [1, 2)
        let store2 = store.clone();
        handles.push(tokio::spawn(async move {
            let mut conn = store2.connect().await.unwrap();
            CoinBalanceBuckets.prune(1, 2, &mut conn).await
        }));

        // Spawn pruning [0, 1)
        let store3 = store.clone();
        handles.push(tokio::spawn(async move {
            let mut conn = store3.connect().await.unwrap();
            CoinBalanceBuckets.prune(0, 1, &mut conn).await
        }));

        // Wait for all pruning operations to complete
        let results: Vec<Result<usize, anyhow::Error>> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // Verify all pruning operations succeeded
        for result in &results {
            assert!(result.is_ok(), "Pruning operation failed: {:?}", result);
        }

        // Verify final state is consistent
        let final_balance_buckets = get_all_balance_buckets(&mut conn).await;
        let final_deletion_references = coin_balance_buckets_deletion_reference::table
            .load::<StoredCoinBalanceBucketDeletionReference>(&mut conn)
            .await
            .unwrap();

        // After all pruning, we should have only the latest versions (cp_sequence_number = 2)
        assert_eq!(final_balance_buckets.len(), 3);
        for bucket in &final_balance_buckets {
            assert_eq!(bucket.cp_sequence_number, 2);
        }

        // All deletion references should be cleaned up
        assert_eq!(final_deletion_references.len(), 0);

        // Verify the total number of pruned rows matches expectations
        let total_pruned: usize = results.into_iter().map(|r| r.unwrap()).sum();
        assert_eq!(total_pruned, 12);
        for bucket in &final_balance_buckets {
            assert_eq!(bucket.cp_sequence_number, 2);
        }
    }
}
