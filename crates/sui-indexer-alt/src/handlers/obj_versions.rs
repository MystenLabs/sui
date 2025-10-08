// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::{concurrent::Handler, Processor},
    postgres::{Connection, Db},
    types::{effects::TransactionEffectsAPI, full_checkpoint_content::CheckpointData},
};
use sui_indexer_alt_schema::{objects::StoredObjVersion, schema::obj_versions};

pub(crate) struct ObjVersions;

#[async_trait]
impl Processor for ObjVersions {
    const NAME: &'static str = "obj_versions";
    type Value = StoredObjVersion;

    async fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let cp_sequence_number = checkpoint_summary.sequence_number as i64;
        Ok(transactions
            .iter()
            .flat_map(|tx| {
                let lamport = tx.effects.lamport_version();

                tx.effects
                    .object_changes()
                    .into_iter()
                    .map(move |c| StoredObjVersion {
                        object_id: c.id.to_vec(),
                        // If the object was created or modified, it has an output version,
                        // otherwise it was deleted/wrapped and its version is the transaction's
                        // lamport version.
                        object_version: c.output_version.unwrap_or(lamport).value() as i64,
                        object_digest: c.output_digest.map(|d| d.inner().into()),
                        cp_sequence_number,
                    })
            })
            .collect())
    }
}

#[async_trait]
impl Handler for ObjVersions {
    type Store = Db;

    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(obj_versions::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
