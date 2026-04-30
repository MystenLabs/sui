// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
use async_trait::async_trait;
use diesel::sql_types::BigInt;
use diesel::sql_types::Bytea;
use diesel_async::RunQueryDsl;
use futures::stream;
use sui_futures::stream::Break;
use sui_futures::stream::ConcurrencyConfig;
use sui_futures::stream::TrySpawnStreamExt;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::postgres::Connection;
use sui_indexer_alt_framework::postgres::handler::Handler;
use sui_indexer_alt_framework::types::effects::TransactionEffectsAPI;
use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;
use sui_indexer_alt_schema::objects::StoredObjVersion;
use sui_indexer_alt_schema::schema::obj_versions;
use sui_sql_macro::query;
use tokio::sync::mpsc;

const PRUNE_CHANNEL_CAPACITY: usize = 100;

pub(crate) struct ObjVersions {
    client: IngestionClient,
}

impl ObjVersions {
    pub(crate) fn new(client: IngestionClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Processor for ObjVersions {
    const NAME: &'static str = "obj_versions";
    type Value = StoredObjVersion;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let Checkpoint {
            transactions,
            summary,
            ..
        } = checkpoint.as_ref();

        let cp_sequence_number = summary.sequence_number as i64;
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
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(obj_versions::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection<'a>,
    ) -> anyhow::Result<usize> {
        let (tx, mut rx) = mpsc::channel(PRUNE_CHANNEL_CAPACITY);

        let client = self.client.clone();
        let fetcher: tokio::task::JoinHandle<Result<(), Break<anyhow::Error>>> =
            tokio::spawn(async move {
                stream::iter(from..to_exclusive)
                    .try_for_each_send_spawned(
                        ConcurrencyConfig::adaptive(33, 1, 2000),
                        move |cp| {
                            let client = client.clone();
                            async move {
                                let checkpoint =
                                    client.checkpoint(cp).await.map_err(anyhow::Error::new)?;
                                Ok(Arc::new(checkpoint))
                            }
                        },
                        tx,
                        |_| {},
                    )
                    .await
            });

        let mut ids: Vec<Vec<u8>> = vec![];
        let mut versions: Vec<i64> = vec![];
        while let Some(cp) = rx.recv().await {
            for tx in &cp.checkpoint.transactions {
                let lamport = tx.effects.lamport_version();
                for change in tx.effects.object_changes() {
                    if let Some(v) = change.input_version {
                        ids.push(change.id.to_vec());
                        versions.push(v.value() as i64);
                    }

                    if change.output_version.is_none() {
                        ids.push(change.id.to_vec());
                        versions.push(lamport.value() as i64);
                    }
                }
            }
        }

        fetcher
            .await
            .context("prune fetcher task panicked")?
            .map_err(|e| match e {
                Break::Break => anyhow!("prune fetcher cancelled"),
                Break::Err(err) => anyhow::Error::from(err).context("prune fetch failed"),
            })?;

        if ids.is_empty() {
            return Ok(0);
        }

        let query = query!(
            r#"
            DELETE FROM
                obj_versions ov
            USING (
                SELECT
                    UNNEST({Array<Bytea>}) AS object_id,
                    UNNEST({Array<BigInt>}) AS object_version
            ) deleted
            WHERE
                ov.object_id = deleted.object_id
            AND ov.object_version = deleted.object_version
            "#,
            ids,
            versions,
        );

        Ok(query.execute(conn).await?)
    }
}
