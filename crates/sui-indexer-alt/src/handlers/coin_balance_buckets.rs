// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, bail, Result};
use diesel::sql_query;
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

use crate::consistent_pruning::{PruningInfo, PruningLookupTable};

/// This handler is used to track the balance buckets of address-owned coins.
/// The balance bucket is calculated using log10 of the coin balance.
/// Whenever a coin object's presence, owner or balance bucket changes,
/// we will insert a new row into the `coin_balance_buckets` table.
/// A Delete record will be inserted when a coin object is no longer present or no longer owned by an address.
#[derive(Default)]
pub(crate) struct CoinBalanceBuckets {
    pruning_lookup_table: Arc<PruningLookupTable>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProcessedCoinBalanceBucket {
    pub object_id: ObjectID,
    pub cp_sequence_number: u64,
    pub change: CoinBalanceBucketChangeKind,
}

#[derive(Debug, PartialEq, Eq)]
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

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let checkpoint_input_objects = checkpoint.checkpoint_input_objects();
        let latest_live_output_objects: BTreeMap<_, _> = checkpoint
            .latest_live_output_objects()
            .into_iter()
            .map(|o| (o.id(), o))
            .collect();
        let mut values: BTreeMap<ObjectID, Self::Value> = BTreeMap::new();
        let mut prune_info = PruningInfo::new();
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
            prune_info.add_deleted_object(*object_id);
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
                    prune_info.add_deleted_object(*object_id);
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
                    // If input_owner is None, it means that the coin was not tracked in the table
                    // prior to the checkpoint, and is now created/unwrapped. In this case, we don't
                    // need to prune anything, since there was no old data to prune.
                    if input_owner.is_some() {
                        prune_info.add_mutated_object(*object_id);
                    }
                }
                _ => {}
            }
        }
        self.pruning_lookup_table
            .insert(cp_sequence_number, prune_info);

        Ok(values.into_values().collect())
    }
}

#[async_trait::async_trait]
impl Handler for CoinBalanceBuckets {
    const PRUNING_REQUIRES_PROCESSED_VALUES: bool = true;

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

    // TODO: Add tests for this function.
    async fn prune(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut db::Connection<'_>,
    ) -> anyhow::Result<usize> {
        use sui_indexer_alt_schema::schema::coin_balance_buckets::dsl;

        let to_prune = self
            .pruning_lookup_table
            .get_prune_info(from, to_exclusive)?;

        if to_prune.is_empty() {
            self.pruning_lookup_table.gc_prune_info(from, to_exclusive);
            return Ok(0);
        }

        // For each (object_id, cp_sequence_number_exclusive), delete all entries with
        // cp_sequence_number less than cp_sequence_number_exclusive that match the object_id.

        let values = to_prune
            .iter()
            .map(|(object_id, seq_number)| {
                let object_id_hex = hex::encode(object_id);
                format!("('\\x{}'::BYTEA, {}::BIGINT)", object_id_hex, seq_number)
            })
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "
            WITH to_prune_data (object_id, cp_sequence_number_exclusive) AS (
                VALUES {}
            )
            DELETE FROM coin_balance_buckets
            USING to_prune_data
            WHERE coin_balance_buckets.{:?} = to_prune_data.object_id
              AND coin_balance_buckets.{:?} < to_prune_data.cp_sequence_number_exclusive
            ",
            values,
            dsl::object_id,
            dsl::cp_sequence_number,
        );
        let rows_deleted = sql_query(query).execute(conn).await?;
        self.pruning_lookup_table.gc_prune_info(from, to_exclusive);
        Ok(rows_deleted)
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use diesel::QueryDsl;
    use sui_indexer_alt_framework::Indexer;
    use sui_indexer_alt_schema::MIGRATIONS;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::base_types::{dbg_addr, MoveObjectType, ObjectID, SequenceNumber, SuiAddress};
    use sui_types::digests::TransactionDigest;
    use sui_types::gas_coin::GAS;
    use sui_types::object::{Authenticator, MoveObject, Object};
    use sui_types::test_checkpoint_data_builder::TestCheckpointDataBuilder;

    // Get all balance buckets from the database, sorted by object_id and cp_sequence_number.
    async fn get_all_balance_buckets(
        conn: &mut db::Connection<'_>,
    ) -> Vec<StoredCoinBalanceBucket> {
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
            Owner::ConsensusV2 {
                authenticator: Box::new(Authenticator::SingleOwner(addr1)),
                start_version: SequenceNumber::new(),
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
        let mut conn = indexer.db().connect().await.unwrap();
        let handler = CoinBalanceBuckets::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_sui_object(0, 0)
            .create_sui_object(1, 100)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = handler.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(values.len(), 2);
        assert!(values.iter().any(|v| matches!(
            v.change,
            CoinBalanceBucketChangeKind::Insert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 0,
                ..
            }
        )));
        assert!(values.iter().any(|v| matches!(
            v.change,
            CoinBalanceBucketChangeKind::Insert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 2,
                ..
            }
        )));
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 2);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 2);
        let rows_pruned = handler.prune(0, 1, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 0);
    }

    #[tokio::test]
    async fn test_process_coin_balance_buckets_new_other_coin() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();
        let handler = CoinBalanceBuckets::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        let coin_type = TypeTag::from_str("0x0::a::b").unwrap();
        builder = builder
            .start_transaction(0)
            .create_coin_object(0, 0, 10, coin_type.clone())
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = handler.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(
            &values[0].change,
            &CoinBalanceBucketChangeKind::Insert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 1,
                coin_type: coin_type.clone(),
                owner_id: TestCheckpointDataBuilder::derive_address(0),
            }
        );
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 1);
        let rows_pruned = handler.prune(0, 1, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 0);
    }

    #[tokio::test]
    async fn test_process_coin_balance_buckets_balance_change() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();
        let handler = CoinBalanceBuckets::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_sui_object(0, 10010)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = handler.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(values.len(), 1);
        // Checkpoint 0 creates coin object 0.
        assert_eq!(
            values[0].change,
            CoinBalanceBucketChangeKind::Insert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 4,
                coin_type: GAS::type_tag(),
                owner_id: TestCheckpointDataBuilder::derive_address(0),
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
        let values = handler.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(values.len(), 1);
        // Checkpoint 1 creates coin object 1.
        assert_eq!(
            values[0].change,
            CoinBalanceBucketChangeKind::Insert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 1,
                coin_type: GAS::type_tag(),
                owner_id: TestCheckpointDataBuilder::derive_address(1),
            }
        );
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 2);

        // Nothing to prune because the two coins in the table have not been updated since creation.
        let rows_pruned = handler.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 0);

        // Transfer 1 MIST, balance goes from 10000 to 9999.
        // The balance bucket changes, we should see a change, both for the old owner and the new owner.
        builder = builder
            .start_transaction(0)
            .transfer_coin_balance(0, 2, 1, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = handler.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(values.len(), 2);
        // Checkpoint 2 creates coin object 2, and mutates coin object 0.
        assert!(values.iter().any(|v| v.change
            == CoinBalanceBucketChangeKind::Insert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 3,
                coin_type: GAS::type_tag(),
                owner_id: TestCheckpointDataBuilder::derive_address(0),
            }));
        assert!(values.iter().any(|v| v.change
            == CoinBalanceBucketChangeKind::Insert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 0,
                coin_type: GAS::type_tag(),
                owner_id: TestCheckpointDataBuilder::derive_address(1),
            }));
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 2);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 4);

        let rows_pruned = handler.prune(2, 3, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 1);
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
        let mut conn = indexer.db().connect().await.unwrap();
        let handler = CoinBalanceBuckets::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = handler.process(&Arc::new(checkpoint)).unwrap();
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = handler.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].change, CoinBalanceBucketChangeKind::Delete);
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);
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

        let rows_pruned = handler.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 2);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 0);
    }

    #[tokio::test]
    async fn test_process_coin_balance_buckets_owner_change() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();
        let handler = CoinBalanceBuckets::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_sui_object(0, 100)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = handler.process(&Arc::new(checkpoint)).unwrap();
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = handler.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(
            values[0].change,
            CoinBalanceBucketChangeKind::Insert {
                owner_kind: StoredCoinOwnerKind::Fastpath,
                balance_bucket: 2,
                coin_type: GAS::type_tag(),
                owner_id: TestCheckpointDataBuilder::derive_address(1),
            }
        );
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);

        let rows_pruned = handler.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 1);
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
        let mut conn = indexer.db().connect().await.unwrap();
        let handler = CoinBalanceBuckets::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = handler.process(&Arc::new(checkpoint)).unwrap();
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
        let values = handler.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].change, CoinBalanceBucketChangeKind::Delete);
        let rows_inserted = CoinBalanceBuckets::commit(&values, &mut conn)
            .await
            .unwrap();
        assert_eq!(rows_inserted, 1);

        let rows_pruned = handler.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 2);
        let all_balance_buckets = get_all_balance_buckets(&mut conn).await;
        assert_eq!(all_balance_buckets.len(), 0);
    }
}
