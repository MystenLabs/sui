// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::DeferralKey;
use narwhal_types::Round;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::transaction::SharedInputObject;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectCost {
    pub total_cost_to_last_write: u64,
    pub total_cost: u64,
}

impl ObjectCost {
    pub fn add_write_cost(&mut self, end_cost: u64) {
        assert!(end_cost > self.total_cost_to_last_write);
        assert!(end_cost > self.total_cost);
        self.total_cost_to_last_write = end_cost;
        self.total_cost = end_cost;
    }

    pub fn add_read_cost(&mut self, end_cost: u64) {
        assert!(end_cost >= self.total_cost_to_last_write);
        self.total_cost = core::cmp::max(end_cost, self.total_cost);
    }
}

pub fn compute_object_start_cost(
    object_total_cost: &HashMap<ObjectID, ObjectCost>,
    shared_input_objects: Vec<SharedInputObject>,
) -> u64 {
    let mut start_cost = 0;
    let default_object_cost = ObjectCost::default();
    shared_input_objects.iter().for_each(|obj| {
        let object_cost = object_total_cost
            .get(&obj.id)
            .unwrap_or(&default_object_cost);
        if obj.mutable {
            start_cost = core::cmp::max(start_cost, object_cost.total_cost);
        } else {
            start_cost = core::cmp::max(start_cost, object_cost.total_cost_to_last_write);
        }
    });
    start_cost
}

pub fn should_defer_due_to_object_congestion(
    object_total_cost: &HashMap<ObjectID, ObjectCost>,
    cert: &VerifiedExecutableTransaction,
    max_accumulated_txn_cost_per_object_in_checkpoint: u64,
    previously_deferred_tx_digests: &HashMap<TransactionDigest, DeferralKey>,
    commit_round: Round,
) -> Option<(DeferralKey, Vec<ObjectID>)> {
    let mut congested_objects = vec![];
    let start_cost =
        compute_object_start_cost(object_total_cost, cert.shared_input_objects().collect());
    for obj in cert.shared_input_objects() {
        if start_cost + cert.gas_budget() > max_accumulated_txn_cost_per_object_in_checkpoint {
            congested_objects.push(obj.id)
        }
    }

    if !congested_objects.is_empty() {
        let deferral_key = if let Some(DeferralKey::ConsensusRound {
            future_round: _,
            deferred_from_round,
        }) = previously_deferred_tx_digests.get(cert.digest())
        {
            // This transaction has been deferred before.
            DeferralKey::new_for_consensus_round(commit_round + 1, *deferred_from_round)
        } else {
            // This include previously deferred randomness transactions.
            DeferralKey::new_for_consensus_round(commit_round + 1, commit_round)
        };
        Some((deferral_key, congested_objects))
    } else {
        None
    }
}

#[cfg(test)]
mod object_cost_tests {
    use super::*;

    use sui_types::base_types::SequenceNumber;

    #[test]
    fn test_compute_object_start_cost() {
        let object_id_0 = ObjectID::random();
        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();
        let object_total_cost: HashMap<ObjectID, ObjectCost> = HashMap::from_iter(
            [
                (
                    object_id_0,
                    ObjectCost {
                        total_cost_to_last_write: 10,
                        total_cost: 20,
                    },
                ),
                (
                    object_id_1,
                    ObjectCost {
                        total_cost_to_last_write: 5,
                        total_cost: 15,
                    },
                ),
            ]
            .iter()
            .cloned(),
        );

        {
            let shared_input_objects = vec![SharedInputObject {
                id: object_id_0,
                initial_shared_version: SequenceNumber::new(),
                mutable: false,
            }];
            assert_eq!(
                compute_object_start_cost(&object_total_cost, shared_input_objects),
                10
            );
        }

        {
            let shared_input_objects = vec![SharedInputObject {
                id: object_id_0,
                initial_shared_version: SequenceNumber::new(),
                mutable: true,
            }];
            assert_eq!(
                compute_object_start_cost(&object_total_cost, shared_input_objects),
                20
            );
        }

        {
            let shared_input_objects = vec![
                SharedInputObject {
                    id: object_id_0,
                    initial_shared_version: SequenceNumber::new(),
                    mutable: false,
                },
                SharedInputObject {
                    id: object_id_1,
                    initial_shared_version: SequenceNumber::new(),
                    mutable: false,
                },
            ];
            assert_eq!(
                compute_object_start_cost(&object_total_cost, shared_input_objects),
                10
            );
        }

        {
            let shared_input_objects = vec![
                SharedInputObject {
                    id: object_id_0,
                    initial_shared_version: SequenceNumber::new(),
                    mutable: false,
                },
                SharedInputObject {
                    id: object_id_1,
                    initial_shared_version: SequenceNumber::new(),
                    mutable: true,
                },
            ];
            assert_eq!(
                compute_object_start_cost(&object_total_cost, shared_input_objects),
                15
            );
        }

        {
            let shared_input_objects = vec![
                SharedInputObject {
                    id: object_id_0,
                    initial_shared_version: SequenceNumber::new(),
                    mutable: true,
                },
                SharedInputObject {
                    id: object_id_1,
                    initial_shared_version: SequenceNumber::new(),
                    mutable: false,
                },
            ];
            assert_eq!(
                compute_object_start_cost(&object_total_cost, shared_input_objects),
                20
            );
        }

        {
            let shared_input_objects = vec![
                SharedInputObject {
                    id: object_id_0,
                    initial_shared_version: SequenceNumber::new(),
                    mutable: true,
                },
                SharedInputObject {
                    id: object_id_1,
                    initial_shared_version: SequenceNumber::new(),
                    mutable: true,
                },
            ];
            assert_eq!(
                compute_object_start_cost(&object_total_cost, shared_input_objects),
                20
            );
        }

        {
            let shared_input_objects = vec![SharedInputObject {
                id: object_id_2,
                initial_shared_version: SequenceNumber::new(),
                mutable: true,
            }];
            assert_eq!(
                compute_object_start_cost(&object_total_cost, shared_input_objects),
                0
            );
        }
    }
}
