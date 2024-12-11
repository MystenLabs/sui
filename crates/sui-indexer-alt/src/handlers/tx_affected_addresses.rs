// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use diesel_async::RunQueryDsl;
use itertools::Itertools;
use sui_indexer_alt_framework::{
    db,
    pipeline::{concurrent::Handler, Processor},
};
use sui_types::{full_checkpoint_content::CheckpointData, object::Owner};

use crate::{models::transactions::StoredTxAffectedAddress, schema::tx_affected_addresses};

pub(crate) struct TxAffectedAddresses;

impl Processor for TxAffectedAddresses {
    const NAME: &'static str = "tx_affected_addresses";

    type Value = StoredTxAffectedAddress;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let mut values = Vec::new();
        let first_tx = checkpoint_summary.network_total_transactions as usize - transactions.len();

        for (i, tx) in transactions.iter().enumerate() {
            let tx_sequence_number = (first_tx + i) as i64;
            let sender = tx.transaction.sender_address();
            let payer = tx.transaction.gas_owner();
            let recipients = tx.effects.all_changed_objects().into_iter().filter_map(
                |(_object_ref, owner, _write_kind)| match owner {
                    Owner::AddressOwner(address) => Some(address),
                    _ => None,
                },
            );

            let affected_addresses: Vec<StoredTxAffectedAddress> = recipients
                .chain(vec![sender, payer])
                .unique()
                .map(|a| StoredTxAffectedAddress {
                    tx_sequence_number,
                    affected: a.to_vec(),
                    sender: sender.to_vec(),
                })
                .collect();
            values.extend(affected_addresses);
        }

        Ok(values)
    }
}

#[async_trait::async_trait]
impl Handler for TxAffectedAddresses {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(tx_affected_addresses::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
