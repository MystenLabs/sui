// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::{SuiAddress, VersionDigest},
    digests::{Digest, ObjectDigest},
    object::{Object, Owner},
};
use move_core_types::language_storage::TypeTag;
use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};

#[cfg(test)]
use nonempty::nonempty;

use super::IDOperation;

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct EffectsObjectChange {
    // input_state and output_state are the core fields that's required by
    // the protocol as it tells how an object changes on-chain.
    /// State of the object in the store prior to this transaction.
    pub(crate) input_state: ObjectIn,
    /// State of the object in the store after this transaction.
    pub(crate) output_state: ObjectOut,

    /// Whether this object ID is created or deleted in this transaction.
    /// This information isn't required by the protocol but is useful for providing more detailed
    /// semantics on object changes.
    pub(crate) id_operation: IDOperation,
}

impl EffectsObjectChange {
    pub fn new(
        modified_at: Option<(VersionDigest, Owner)>,
        written: Option<&Object>,
        id_created: bool,
        id_deleted: bool,
    ) -> Self {
        debug_assert!(
            !id_created || !id_deleted,
            "Object ID can't be created and deleted at the same time."
        );
        Self {
            input_state: modified_at.map_or(ObjectIn::NotExist, ObjectIn::Exist),
            output_state: written.map_or(ObjectOut::NotExist, |o| {
                if o.is_package() {
                    ObjectOut::PackageWrite((o.version(), o.digest()))
                } else {
                    ObjectOut::ObjectWrite((o.digest(), o.owner.clone()))
                }
            }),
            id_operation: if id_created {
                IDOperation::Created
            } else if id_deleted {
                IDOperation::Deleted
            } else {
                IDOperation::None
            },
        }
    }

    pub fn new_from_accumulator_write(write: AccumulatorWriteV1) -> Self {
        Self {
            input_state: ObjectIn::NotExist,
            output_state: ObjectOut::AccumulatorWriteV1(write),
            id_operation: IDOperation::None,
        }
    }
}

/// If an object exists (at root-level) in the store prior to this transaction,
/// it should be Exist, otherwise it's NonExist, e.g. wrapped objects should be
/// NotExist.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum ObjectIn {
    NotExist,
    /// The old version, digest and owner.
    Exist((VersionDigest, Owner)),
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum AccumulatorOperation {
    /// Merge the value into the accumulator.
    Merge,
    /// Split the value from the accumulator.
    Split,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum AccumulatorValue {
    Integer(u64),
    IntegerTuple(u64, u64),
    EventDigest(NonEmpty<(u64 /* event index in the transaction */, Digest)>),
}

/// Accumulator objects are named by an address (can be an account address or a UID)
/// and a type tag.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct AccumulatorAddress {
    pub address: SuiAddress,
    pub ty: TypeTag,
}

impl AccumulatorAddress {
    pub fn new(address: SuiAddress, ty: TypeTag) -> Self {
        Self { address, ty }
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct AccumulatorWriteV1 {
    pub address: AccumulatorAddress,
    /// The operation to be applied to the accumulator.
    pub operation: AccumulatorOperation,
    /// The value to be merged into or split from the accumulator.
    pub value: AccumulatorValue,
}

impl AccumulatorWriteV1 {
    pub fn merge(mut writes: Vec<Self>) -> Self {
        if writes.len() == 1 {
            return writes.pop().unwrap();
        }

        let address = writes[0].address.clone();

        if mysten_common::in_test_configuration() {
            for write in &writes[1..] {
                if write.address.address != address.address {
                    mysten_common::debug_fatal!(
                        "All writes must have the same accumulator address: {} != {}",
                        write.address.address,
                        address.address
                    );
                }
                if write.address.ty != address.ty {
                    mysten_common::debug_fatal!(
                        "All writes must have the same accumulator type: {:?} != {:?}",
                        write.address.ty,
                        address.ty
                    );
                }
            }
        }
        let (merged_value, net_operation) = match &writes[0].value {
            AccumulatorValue::Integer(_) => {
                let (merge_amount, split_amount) =
                    writes.iter().fold((0u64, 0u64), |(merge, split), w| {
                        if let AccumulatorValue::Integer(v) = w.value {
                            match w.operation {
                                AccumulatorOperation::Merge => (
                                    merge.checked_add(v).expect("validated in object runtime"),
                                    split,
                                ),
                                AccumulatorOperation::Split => (
                                    merge,
                                    split.checked_add(v).expect("validated in object runtime"),
                                ),
                            }
                        } else {
                            mysten_common::fatal!(
                                "mismatched accumulator value types for same object"
                            );
                        }
                    });
                let (amount, operation) = if merge_amount >= split_amount {
                    (merge_amount - split_amount, AccumulatorOperation::Merge)
                } else {
                    (split_amount - merge_amount, AccumulatorOperation::Split)
                };
                (AccumulatorValue::Integer(amount), operation)
            }
            AccumulatorValue::IntegerTuple(_, _) => {
                todo!("IntegerTuple netting-out logic not yet implemented")
            }
            AccumulatorValue::EventDigest(first_digests) => {
                let mut event_digests = first_digests.clone();
                for write in &writes[1..] {
                    if let AccumulatorValue::EventDigest(digests) = &write.value {
                        event_digests.extend(digests.iter().copied());
                    } else {
                        mysten_common::fatal!("mismatched accumulator value types for same object");
                    }
                }
                (
                    AccumulatorValue::EventDigest(event_digests),
                    AccumulatorOperation::Merge,
                )
            }
        };
        AccumulatorWriteV1 {
            address,
            operation: net_operation,
            value: merged_value,
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum ObjectOut {
    /// Same definition as in ObjectIn.
    NotExist,
    /// Any written object, including all of mutated, created, unwrapped today.
    ObjectWrite((ObjectDigest, Owner)),
    /// Packages writes need to be tracked separately with version because
    /// we don't use lamport version for package publish and upgrades.
    PackageWrite(VersionDigest),
    /// This isn't an object write, but a special write to an accumulator.
    AccumulatorWriteV1(AccumulatorWriteV1),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base_types::SuiAddress;

    fn test_accumulator_address() -> AccumulatorAddress {
        AccumulatorAddress::new(
            SuiAddress::random_for_testing_only(),
            "0x2::balance::Balance<0x2::sui::SUI>".parse().unwrap(),
        )
    }

    #[test]
    fn test_merge_single_item() {
        let addr = test_accumulator_address();
        let write = AccumulatorWriteV1 {
            address: addr.clone(),
            operation: AccumulatorOperation::Merge,
            value: AccumulatorValue::Integer(100),
        };

        let result = AccumulatorWriteV1::merge(vec![write.clone()]);
        assert_eq!(result, write);
    }

    #[test]
    fn test_merge_positive_balance() {
        let addr = test_accumulator_address();
        let writes = vec![
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::Integer(100),
            },
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::Integer(50),
            },
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Split,
                value: AccumulatorValue::Integer(30),
            },
        ];

        let result = AccumulatorWriteV1::merge(writes);
        assert_eq!(result.operation, AccumulatorOperation::Merge);
        assert_eq!(result.value, AccumulatorValue::Integer(120));
    }

    #[test]
    fn test_merge_negative_balance() {
        let addr = test_accumulator_address();
        let writes = vec![
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::Integer(50),
            },
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Split,
                value: AccumulatorValue::Integer(100),
            },
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Split,
                value: AccumulatorValue::Integer(30),
            },
        ];

        let result = AccumulatorWriteV1::merge(writes);
        assert_eq!(result.operation, AccumulatorOperation::Split);
        assert_eq!(result.value, AccumulatorValue::Integer(80));
    }

    #[test]
    fn test_merge_zero_balance() {
        let addr = test_accumulator_address();
        let writes = vec![
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::Integer(100),
            },
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Split,
                value: AccumulatorValue::Integer(100),
            },
        ];

        let result = AccumulatorWriteV1::merge(writes);
        assert_eq!(result.operation, AccumulatorOperation::Merge);
        assert_eq!(result.value, AccumulatorValue::Integer(0));
    }

    #[test]
    fn test_merge_all_merges() {
        let addr = test_accumulator_address();
        let writes = vec![
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::Integer(100),
            },
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::Integer(200),
            },
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::Integer(300),
            },
        ];

        let result = AccumulatorWriteV1::merge(writes);
        assert_eq!(result.operation, AccumulatorOperation::Merge);
        assert_eq!(result.value, AccumulatorValue::Integer(600));
    }

    #[test]
    fn test_merge_all_splits() {
        let addr = test_accumulator_address();
        let writes = vec![
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Split,
                value: AccumulatorValue::Integer(100),
            },
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Split,
                value: AccumulatorValue::Integer(200),
            },
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Split,
                value: AccumulatorValue::Integer(50),
            },
        ];

        let result = AccumulatorWriteV1::merge(writes);
        assert_eq!(result.operation, AccumulatorOperation::Split);
        assert_eq!(result.value, AccumulatorValue::Integer(350));
    }

    #[test]
    fn test_merge_event_digests_single() {
        let addr = test_accumulator_address();
        let digest1 = Digest::random();
        let digest2 = Digest::random();

        let write = AccumulatorWriteV1 {
            address: addr.clone(),
            operation: AccumulatorOperation::Merge,
            value: AccumulatorValue::EventDigest(nonempty![(0, digest1), (1, digest2)]),
        };

        let result = AccumulatorWriteV1::merge(vec![write.clone()]);
        assert_eq!(result, write);
    }

    #[test]
    fn test_merge_event_digests_multiple() {
        let addr = test_accumulator_address();
        let digest1 = Digest::random();
        let digest2 = Digest::random();
        let digest3 = Digest::random();
        let digest4 = Digest::random();

        let writes = vec![
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::EventDigest(nonempty![(0, digest1), (1, digest2)]),
            },
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::EventDigest(nonempty![(2, digest3)]),
            },
            AccumulatorWriteV1 {
                address: addr.clone(),
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::EventDigest(nonempty![(3, digest4)]),
            },
        ];

        let result = AccumulatorWriteV1::merge(writes);
        assert_eq!(result.operation, AccumulatorOperation::Merge);
        if let AccumulatorValue::EventDigest(digests) = result.value {
            assert_eq!(digests.len(), 4);
            assert_eq!(digests[0], (0, digest1));
            assert_eq!(digests[1], (1, digest2));
            assert_eq!(digests[2], (2, digest3));
            assert_eq!(digests[3], (3, digest4));
        } else {
            panic!("Expected EventDigest value");
        }
    }

    #[test]
    #[should_panic(expected = "All writes must have the same accumulator address")]
    fn test_merge_mismatched_addresses() {
        let addr1 = test_accumulator_address();
        let addr2 = test_accumulator_address();

        let writes = vec![
            AccumulatorWriteV1 {
                address: addr1,
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::Integer(100),
            },
            AccumulatorWriteV1 {
                address: addr2,
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::Integer(50),
            },
        ];

        AccumulatorWriteV1::merge(writes);
    }

    #[test]
    #[should_panic(expected = "All writes must have the same accumulator type")]
    fn test_merge_mismatched_types() {
        let addr = SuiAddress::random_for_testing_only();
        let addr1 = AccumulatorAddress::new(
            addr,
            "0x2::balance::Balance<0x2::sui::SUI>".parse().unwrap(),
        );
        let addr2 = AccumulatorAddress::new(
            addr,
            "0x2::accumulator_settlement::EventStreamHead"
                .parse()
                .unwrap(),
        );

        let writes = vec![
            AccumulatorWriteV1 {
                address: addr1,
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::Integer(100),
            },
            AccumulatorWriteV1 {
                address: addr2,
                operation: AccumulatorOperation::Merge,
                value: AccumulatorValue::Integer(50),
            },
        ];

        AccumulatorWriteV1::merge(writes);
    }
}
