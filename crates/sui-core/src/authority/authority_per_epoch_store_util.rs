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
    let start_cost =
        compute_object_start_cost(object_total_cost, cert.shared_input_objects().collect());
    if start_cost + cert.gas_budget() <= max_accumulated_txn_cost_per_object_in_checkpoint {
        return None;
    }

    let mut congested_objects = vec![];
    for obj in cert.shared_input_objects() {
        if obj.mutable {
            if start_cost
                == object_total_cost
                    .get(&obj.id)
                    .map_or(0, |cost| cost.total_cost)
            {
                congested_objects.push(obj.id);
            }
        } else {
            if start_cost
                == object_total_cost
                    .get(&obj.id)
                    .map_or(0, |cost| cost.total_cost_to_last_write)
            {
                congested_objects.push(obj.id);
            }
        }
    }

    assert!(!congested_objects.is_empty());

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
}

#[cfg(test)]
mod object_cost_tests {
    use super::*;

    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::base_types::{random_object_ref, SequenceNumber};
    use sui_types::crypto::{get_key_pair, AccountKeyPair};
    use sui_types::transaction::{CallArg, ObjectArg, TransactionDataAPI, VerifiedTransaction};

    fn init_object_total_cost(
        init_values: &[(ObjectID, u64, u64)],
    ) -> HashMap<ObjectID, ObjectCost> {
        let mut object_total_cost = HashMap::new();
        for (object_id, total_cost_to_last_write, total_cost) in init_values {
            object_total_cost.insert(
                *object_id,
                ObjectCost {
                    total_cost_to_last_write: *total_cost_to_last_write,
                    total_cost: *total_cost,
                },
            );
        }
        object_total_cost
    }

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

    #[test]
    fn test_update_read_write_cost() {
        let mut object_cost = ObjectCost {
            total_cost_to_last_write: 0,
            total_cost: 0,
        };
        object_cost.add_write_cost(10);
        assert_eq!(object_cost.total_cost_to_last_write, 10);
        assert_eq!(object_cost.total_cost, 10);

        object_cost.add_read_cost(20);
        assert_eq!(object_cost.total_cost_to_last_write, 10);
        assert_eq!(object_cost.total_cost, 20);

        object_cost.add_read_cost(15);
        assert_eq!(object_cost.total_cost, 20);

        object_cost.add_write_cost(30);
        assert_eq!(object_cost.total_cost_to_last_write, 30);
        assert_eq!(object_cost.total_cost, 30);
    }

    #[test]
    fn test_should_defer_return_correct_congested_objects() {
        let (sender, keypair): (_, AccountKeyPair) = get_key_pair();

        let shared_obj_0 = ObjectID::random();
        let shared_object_0_write = ObjectArg::SharedObject {
            id: shared_obj_0,
            initial_shared_version: 0.into(),
            mutable: true,
        };
        let shared_object_0_read = ObjectArg::SharedObject {
            id: shared_obj_0,
            initial_shared_version: 0.into(),
            mutable: false,
        };

        let shared_obj_1 = ObjectID::random();
        let shared_object_1_write = ObjectArg::SharedObject {
            id: shared_obj_1,
            initial_shared_version: 0.into(),
            mutable: true,
        };
        let shared_object_1_read = ObjectArg::SharedObject {
            id: shared_obj_1,
            initial_shared_version: 0.into(),
            mutable: false,
        };

        let gas_object = random_object_ref();
        let tx_write_0 = VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_unchecked(
                TestTransactionBuilder::new(sender, gas_object, 1000)
                    .move_call(
                        ObjectID::random(),
                        "unimportant_module",
                        "unimportant_function",
                        vec![CallArg::Object(shared_object_0_write)],
                    )
                    .build_and_sign(&keypair),
            ),
            0,
        );

        let tx_read_0_read_1 = VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_unchecked(
                TestTransactionBuilder::new(sender, gas_object, 1000)
                    .move_call(
                        ObjectID::random(),
                        "unimportant_module",
                        "unimportant_function",
                        vec![
                            CallArg::Object(shared_object_0_read),
                            CallArg::Object(shared_object_1_read),
                        ],
                    )
                    .build_and_sign(&keypair),
            ),
            0,
        );
        let tx_read_0_write_1 = VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_unchecked(
                TestTransactionBuilder::new(sender, gas_object, 1000)
                    .move_call(
                        ObjectID::random(),
                        "unimportant_module",
                        "unimportant_function",
                        vec![
                            CallArg::Object(shared_object_0_read),
                            CallArg::Object(shared_object_1_write),
                        ],
                    )
                    .build_and_sign(&keypair),
            ),
            0,
        );
        let max_accumulated_txn_cost_per_object_in_checkpoint =
            tx_write_0.transaction_data().gas_budget() + 1;

        let object_total_cost =
            init_object_total_cost(&[(shared_obj_0, 1, 10), (shared_obj_1, 1, 10)]);
        if let Some((_, congested_objects)) = should_defer_due_to_object_congestion(
            &object_total_cost,
            &tx_write_0,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &HashMap::new(),
            0,
        ) {
            assert_eq!(congested_objects.len(), 1);
            assert_eq!(congested_objects[0], shared_obj_0);
        } else {
            panic!("should defer");
        }

        assert!(should_defer_due_to_object_congestion(
            &object_total_cost,
            &tx_read_0_read_1,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &HashMap::new(),
            0,
        )
        .is_none());

        let object_total_cost =
            init_object_total_cost(&[(shared_obj_0, 10, 10), (shared_obj_1, 1, 10)]);
        if let Some((_, congested_objects)) = should_defer_due_to_object_congestion(
            &object_total_cost,
            &tx_read_0_read_1,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &HashMap::new(),
            0,
        ) {
            assert_eq!(congested_objects.len(), 1);
            assert_eq!(congested_objects[0], shared_obj_0);
        } else {
            panic!("should defer");
        }

        let object_total_cost =
            init_object_total_cost(&[(shared_obj_0, 1, 10), (shared_obj_1, 1, 10)]);
        if let Some((_, congested_objects)) = should_defer_due_to_object_congestion(
            &object_total_cost,
            &tx_read_0_write_1,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &HashMap::new(),
            0,
        ) {
            assert_eq!(congested_objects.len(), 1);
            assert_eq!(congested_objects[0], shared_obj_1);
        } else {
            panic!("should defer");
        }
    }

    #[test]
    fn test_should_defer_return_correct_deferral_key() {
        let (sender, keypair): (_, AccountKeyPair) = get_key_pair();

        let shared_obj_0 = ObjectID::random();
        let shared_object_0_write = ObjectArg::SharedObject {
            id: shared_obj_0,
            initial_shared_version: 0.into(),
            mutable: true,
        };
        let gas_object = random_object_ref();
        let tx = VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_unchecked(
                TestTransactionBuilder::new(sender, gas_object, 1000)
                    .move_call(
                        ObjectID::random(),
                        "unimportant_module",
                        "unimportant_function",
                        vec![CallArg::Object(shared_object_0_write)],
                    )
                    .build_and_sign(&keypair),
            ),
            10,
        );
        let max_accumulated_txn_cost_per_object_in_checkpoint = 1;

        let mut previously_deferred_tx_digests = HashMap::new();
        previously_deferred_tx_digests.insert(
            TransactionDigest::random(),
            DeferralKey::ConsensusRound {
                future_round: 10,
                deferred_from_round: 5,
            },
        );
        if let Some((
            DeferralKey::ConsensusRound {
                future_round,
                deferred_from_round,
            },
            _,
        )) = should_defer_due_to_object_congestion(
            &HashMap::new(),
            &tx,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &previously_deferred_tx_digests,
            10,
        ) {
            assert_eq!(future_round, 11);
            assert_eq!(deferred_from_round, 10);
        } else {
            panic!("should defer");
        }

        previously_deferred_tx_digests.insert(
            tx.digest().clone(),
            DeferralKey::RandomnessDkg {
                deferred_from_round: 5,
            },
        );
        if let Some((
            DeferralKey::ConsensusRound {
                future_round,
                deferred_from_round,
            },
            _,
        )) = should_defer_due_to_object_congestion(
            &HashMap::new(),
            &tx,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &previously_deferred_tx_digests,
            10,
        ) {
            assert_eq!(future_round, 11);
            assert_eq!(deferred_from_round, 10);
        } else {
            panic!("should defer");
        }

        previously_deferred_tx_digests.insert(
            tx.digest().clone(),
            DeferralKey::ConsensusRound {
                future_round: 10,
                deferred_from_round: 5,
            },
        );
        if let Some((
            DeferralKey::ConsensusRound {
                future_round,
                deferred_from_round,
            },
            _,
        )) = should_defer_due_to_object_congestion(
            &HashMap::new(),
            &tx,
            max_accumulated_txn_cost_per_object_in_checkpoint,
            &previously_deferred_tx_digests,
            10,
        ) {
            assert_eq!(future_round, 11);
            assert_eq!(deferred_from_round, 5);
        } else {
            panic!("should defer");
        }
    }
}
