// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{ObjectID, ObjectRef, SequenceNumber};
use crate::transaction::{ObjectArg, SharedObjectMutability};

#[test]
fn test_shared_object_backward_compatibility() {
    use bcs;

    // Old version of SharedObject with bool field
    #[derive(serde::Serialize, serde::Deserialize)]
    struct OldSharedObject {
        id: ObjectID,
        initial_shared_version: SequenceNumber,
        mutable: bool,
    }

    // Old version of ObjectArg enum
    #[derive(serde::Serialize, serde::Deserialize)]
    enum OldObjectArg {
        ImmOrOwnedObject(ObjectRef),
        SharedObject {
            id: ObjectID,
            initial_shared_version: SequenceNumber,
            mutable: bool,
        },
        Receiving(ObjectRef),
    }

    let test_id = ObjectID::ZERO;
    let test_version = SequenceNumber::from_u64(1);

    // Test immutable (false -> Immutable)
    let old_immutable = OldObjectArg::SharedObject {
        id: test_id,
        initial_shared_version: test_version,
        mutable: false,
    };
    let encoded_immutable = bcs::to_bytes(&old_immutable).unwrap();
    let decoded: ObjectArg = bcs::from_bytes(&encoded_immutable).unwrap();

    match decoded {
        ObjectArg::SharedObject {
            id,
            initial_shared_version,
            mutability,
        } => {
            assert_eq!(id, test_id);
            assert_eq!(initial_shared_version, test_version);
            assert_eq!(mutability, SharedObjectMutability::Immutable);
        }
        _ => panic!("Expected SharedObject variant"),
    }

    // Test mutable (true -> Mutable)
    let old_mutable = OldObjectArg::SharedObject {
        id: test_id,
        initial_shared_version: test_version,
        mutable: true,
    };
    let encoded_mutable = bcs::to_bytes(&old_mutable).unwrap();
    let decoded: ObjectArg = bcs::from_bytes(&encoded_mutable).unwrap();

    match decoded {
        ObjectArg::SharedObject {
            id,
            initial_shared_version,
            mutability,
        } => {
            assert_eq!(id, test_id);
            assert_eq!(initial_shared_version, test_version);
            assert_eq!(mutability, SharedObjectMutability::Mutable);
        }
        _ => panic!("Expected SharedObject variant"),
    }
}
