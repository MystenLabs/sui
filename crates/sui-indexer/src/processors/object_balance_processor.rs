// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_types::{CheckpointId, SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI};
use tracing::info;

use crate::errors::IndexerError;
use crate::models::object_balances::ObjectBalance;
use crate::store::IndexerStore;

pub const OBJECT_BALANCES_WATERMARK: &str = "object-balances";

pub struct ObjectBalanceProcessor<S> {
    pub store: S,
}

impl<S> ObjectBalanceProcessor<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer ObjectBalance async processor started...");
        let watermark = self.store.get_watermark(OBJECT_BALANCES_WATERMARK).await?;
        let mut cursor = watermark
            .map(|w| w.checkpoint.map(|c| c.saturating_add(1)).unwrap_or(0))
            .unwrap_or(0);
        loop {
            let latest_checkpoint = self.store.get_latest_checkpoint_sequence_number().await?;

            if latest_checkpoint < cursor {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }

            let checkpoints = self
                .store
                .get_checkpoints(Some(CheckpointId::SequenceNumber(cursor as u64)), 1)
                .await?;
            let transactions = checkpoints
                .iter()
                .flat_map(|c| c.transactions.iter())
                .collect::<Vec<_>>();

            let tx_digest_strs = transactions
                .iter()
                .map(|tx| tx.to_string())
                .collect::<Vec<String>>();
            let txs = self
                .store
                .multi_get_transactions_by_digests(&tx_digest_strs)
                .await?;
            let effects = txs
                .into_iter()
                .map(|tx| tx.transaction_effects_content)
                .map(|effects| serde_json::from_str(&effects))
                .collect::<Result<Vec<SuiTransactionBlockEffects>, _>>()
                .map_err(|e| IndexerError::SerdeError(e.to_string()))?;

            let changed_objects = effects
                .iter()
                .map(|fx| fx.all_changed_objects())
                .flat_map(|changed_objects| {
                    changed_objects
                        .into_iter()
                        .map(|(object_ref, _)| (object_ref.object_id(), object_ref.version()))
                })
                .collect::<Vec<_>>();
            let changed_objects = changed_objects
                .iter()
                .map(|(id, version)| self.store.get_sui_types_object(id, version))
                .collect::<Result<Vec<_>, _>>()?;

            let mut balances = Vec::new();

            for object in changed_objects {
                let sui_types::object::Data::Move(move_object) = &object.data else {
                        continue;
                    };

                for (type_tag, value) in
                    move_object.get_coin_balances2(self.store.module_cache())?
                {
                    balances.push(ObjectBalance {
                        id: object.id().to_string(),
                        version: object.version().value() as i64,
                        coin_type: type_tag.to_string(),
                        balance: value as i64,
                    })
                }
            }

            self.store
                .persist_object_balances(cursor, &balances)
                .await?;

            info!("Processed object balances for checkpoints: {}", cursor);
            cursor += 1;
        }
    }
}
