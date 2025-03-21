// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use diesel::prelude::*;
use diesel::sql_types::{Array, Bytea, Bool};
use diesel::deserialize::{QueryableByName};
use diesel_async::RunQueryDsl;
use diesel::pg::Pg;
use sui_types::digests::ObjectDigest;
use std::sync::Arc;
use std::time::Instant;
use std::collections::HashMap;
use url::Url;

use sui_indexer_alt_schema::{
    objects::{StoredObjInfo, StoredObjVersion, StoredOwnerKind},
    schema::{obj_info, obj_versions},
};
use sui_types::accumulator::Accumulator;
use sui_core::authority::authority_store_tables::LiveObject;
use sui_sql_macro::sql;
use sui_types::object::Object;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, VersionNumber};
use sui_core::state_accumulator::{StateAccumulator, StateAccumulatorMetrics};
use sui_types::messages_checkpoint::ECMHLiveObjectSetDigest;
use fastcrypto::hash::MultisetHash;
use sui_pg_db as db;

#[derive(QueryableByName, Debug)]
struct ObjectDigestResult {
    #[diesel(sql_type = Bytea)]
    object_digest: Vec<u8>,
}

pub struct Config {
    pub database_url: Url,
    pub page_size: i64,
}

pub struct ObjectAccumulator {
    db: db::Db,
    config: Config,
}

impl ObjectAccumulator {
    pub async fn new(config: Config) -> Result<Self> {
        let db = db::Db::for_read(config.database_url.clone(), db::DbArgs::default()).await?;

        Ok(Self {
            db,
            config,
        })
    }

    pub async fn accumulate_objects(&mut self) -> Result<ECMHLiveObjectSetDigest> {
        use obj_info::dsl as i;

        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();

        let mut conn = self.db.connect().await?;

        let (candidates, newer) = diesel::alias!(obj_info as candidates, obj_info as newer);

        macro_rules! candidates {
            ($($field:ident),* $(,)?) => {
                candidates.fields(($(o::$field),*))
            };
        }

        macro_rules! newer {
            ($($field:ident),* $(,)?) => {
                newer.fields(($(o::$field),*))
            };
        }
        let start_time = Instant::now();
        let mut total_objects = 0;
        let mut acc = Accumulator::default();
        // let mut live_obj = std::collections::HashSet::new();

        println!("Starting accumulation of objects");

        // Track last processed object for keyset pagination
        let mut cursor: Option<Vec<u8>> = None;

        loop {
            // let mut query = candidates
            //     .select(candidates!(
            //         object_id,
            //         cp_sequence_number,
            //         owner_kind,
            //         owner_id,
            //         package,
            //         module,
            //         name,
            //         instantiation,
            //     ))
            //     .left_join(
            //         newer.on(candidates!(object_id)
            //             .eq(newer!(object_id))
            //             .and(candidates!(cp_sequence_number).lt(newer!(cp_sequence_number)))),
            //     )
            //     .filter(newer!(object_id).is_null())
            //     .filter(candidates!(owner_kind).eq(StoredOwnerKind::Address))
            //     // .filter(candidates!(owner_kind).is_not_null())
            //     .order_by(candidates!(cp_sequence_number).desc())
            //     .then_order_by(candidates!(object_id).desc())
            //     .limit(self.config.page_size)
            //     .into_boxed();

            let limit = self.config.page_size;
            tracing::info!("limit: {}", limit);

            let mut query = i::obj_info.select((
                i::object_id,
                // i::cp_sequence_number,
                i::owner_kind,
            ))
            .distinct_on(i::object_id)
            .order((i::object_id, i::cp_sequence_number.desc()))
            .filter(i::owner_kind.is_not_null())
            .limit(limit)
            .into_boxed();


            if let Some(ref object_id) = cursor {
                query = query.filter(i::object_id.gt(object_id.clone()));
                // query = query.filter(sql!(as Bool,
                //     "(candidates.cp_sequence_number, candidates.object_id) < ({BigInt}, {Bytea})",
                //     cp_sequence_number,
                //     object_id.clone(),
                // ));
            }

            tracing::info!("{}", diesel::debug_query(&query));

            let latest_obj_infos: Vec<(Vec<u8>, Option<StoredOwnerKind>)> = query.get_results(&mut conn).await?;

            // for obj in &latest_obj_infos {
            //     if obj.1.is_some() {
            //         if !live_obj.insert(obj.0.clone()) {

            //         }
            //     }
            // }

            tracing::info!("Found {} objects", latest_obj_infos.len());

            if latest_obj_infos.is_empty() {
                break;
            }

            // Get the object_ids from the batch
            let object_ids: Vec<Vec<u8>> = latest_obj_infos
                .iter()
                .map(|info| info.0.clone())
                .collect();

            // Fetch latest versions using LATERAL join
            let query = diesel::sql_query(
                r#"
                    SELECT
                        v.object_digest
                    FROM (
                        SELECT UNNEST($1) object_id
                    ) k
                    CROSS JOIN LATERAL (
                        SELECT
                            object_id,
                            object_version,
                            object_digest,
                            cp_sequence_number
                        FROM
                            obj_versions
                        WHERE
                            obj_versions.object_id = k.object_id
                        ORDER BY
                            object_version DESC
                        LIMIT
                            1
                    ) v
                "#,
            )
            .bind::<Array<Bytea>, _>(object_ids.clone());

            let digest_results: Vec<ObjectDigestResult> = query.get_results(&mut conn).await?;

            tracing::info!("Found {} digest results", digest_results.len());
            let digests: Vec<Vec<u8>> = digest_results.into_iter().map(|r| r.object_digest).collect();

            // Process the batch
            for digest in digests {
                acc.insert(digest);

                total_objects += 1;
                if total_objects % 1000000 == 0 {
                    let elapsed = start_time.elapsed();
                    println!(
                        "Processed {} objects in {:.2}s",
                        total_objects,
                        elapsed.as_secs_f64()
                    );
                }
            }

            // Update last_object_id once per batch using the last object_id we processed
            if let Some(last_info) = latest_obj_infos.last() {
                cursor = Some(last_info.0.clone());
            }
        }

        let elapsed = start_time.elapsed();
        println!(
            "Completed processing {} objects in {:.2}s",
            total_objects,
            elapsed.as_secs_f64()
        );

        Ok(acc.digest().into())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config {
        database_url: Url::parse(&std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set"))
            .expect("Invalid database URL"),
        page_size: 100000,
    };

    let mut accumulator = ObjectAccumulator::new(config).await?;
    let digest = accumulator.accumulate_objects().await?;
    println!("Final digest: {:?}", digest);

    Ok(())
}

