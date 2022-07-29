// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rocksdb::{DBWithThreadMode, MultiThreaded, Options};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use sui_types::base_types::{AuthorityName, ExecutionDigests};
use sui_types::error::SuiResult;
use typed_store::rocks::DBMap;
use typed_store::{reopen, Map};

const TX_EFFECTS_GOSSIP_ORDER: &str = "tx_effects_gossip_order";
const TALLY_SCORES: &str = "tally_scores";

pub type GossipSequenceNumber = u64;

pub struct TallyRecord {
    next_seq: AtomicU64,
    /// Stores the names of validators that sent us execution digests through gossip.
    /// The key of this table is a tuple of the execution digest and the name of the validator.
    /// The value is a sequence number obtained from an atomic counter that increase each time
    /// we receive an item from gossip. It can be used to determine the order we received from
    /// each validator for each transaction.
    /// At the end of each checkpoint, we go through each digest executed in the checkpoint,
    /// update the score of validators based on the order we received from them. We also remove
    /// all the digests processed.
    /// In case there are leaks (i.e. digests added back after processing), at the end of epoch,
    /// we clear this table.
    tx_effects_gossip_order: DBMap<(ExecutionDigests, AuthorityName), GossipSequenceNumber>,

    /// Tracks the current accumulated total score of each validator.
    tally_scores: DBMap<AuthorityName, u64>,
}

impl TallyRecord {
    pub fn new(db: &Arc<DBWithThreadMode<MultiThreaded>>) -> Self {
        let (tx_effects_gossip_order, tally_scores) = reopen! (
            db,
            TX_EFFECTS_GOSSIP_ORDER;<(ExecutionDigests, AuthorityName), GossipSequenceNumber>,
            TALLY_SCORES;<AuthorityName, u64>
        );
        Self {
            next_seq: AtomicU64::new(0),
            tx_effects_gossip_order,
            tally_scores,
        }
    }

    pub fn get_tables_options<'a>(
        options: &'a Options,
        point_lookup: &'a Options,
    ) -> Vec<(&'static str, &'a Options)> {
        vec![
            (TX_EFFECTS_GOSSIP_ORDER, options),
            (TALLY_SCORES, point_lookup),
        ]
    }

    pub fn is_table(table_name: &str) -> bool {
        table_name == TX_EFFECTS_GOSSIP_ORDER || table_name == TALLY_SCORES
    }

    pub fn dump(&self, table_name: &str) -> anyhow::Result<BTreeMap<String, String>> {
        match table_name {
            TX_EFFECTS_GOSSIP_ORDER => {
                self.tx_effects_gossip_order.try_catch_up_with_primary()?;
                Ok(self
                    .tx_effects_gossip_order
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>())
            }
            TALLY_SCORES => {
                self.tally_scores.try_catch_up_with_primary()?;
                Ok(self
                    .tally_scores
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>())
            }
            _ => {
                unreachable!()
            }
        }
    }

    pub fn add_record(&self, digests: ExecutionDigests, validator: AuthorityName) -> SuiResult {
        let seq = self.next_seq();
        self.tx_effects_gossip_order
            .insert(&(digests, validator), &seq)?;
        Ok(())
    }

    /// Given a list of effects digests, for each of these digests, we look at our record and see
    /// for each digest what is the order we received it from different validators through gossip.
    /// We then give them score based on the order.
    pub fn update_score<'a>(
        &self,
        digests: impl Iterator<Item = &'a ExecutionDigests>,
    ) -> SuiResult {
        let mut to_be_deleted = Vec::new();
        for digest in digests {
            let records: BTreeMap<_, _> = self
                .tx_effects_gossip_order
                .iter()
                .skip_to(&(*digest, AuthorityName::ZERO))?
                .take_while(|((d, _name), _seq)| d == digest)
                .map(|((_, name), seq)| (seq, name))
                .collect();
            to_be_deleted.extend(records.iter().map(|(_, name)| (*digest, *name)));
            let gossip_size = records.len();
            for (order, (_, name)) in records.into_iter().enumerate() {
                self.give_score(name, order, gossip_size)?;
            }
        }
        self.tx_effects_gossip_order.multi_remove(to_be_deleted)?;
        Ok(())
    }

    fn next_seq(&self) -> GossipSequenceNumber {
        self.next_seq.fetch_add(1, Ordering::Relaxed)
    }

    fn give_score(&self, name: AuthorityName, order: usize, gossip_size: usize) -> SuiResult {
        let cur_score = self.tally_scores.get(&name)?.unwrap_or_default();
        let to_add = Self::compute_score_for_order(order, gossip_size);
        self.tally_scores.insert(&name, &(cur_score + to_add))?;
        Ok(())
    }

    fn compute_score_for_order(order: usize, gossip_size: usize) -> u64 {
        (gossip_size - order) as u64
    }
}
