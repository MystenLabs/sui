// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::TestScenarioTests {
    use Sui::ID;
    use Sui::TestScenario::{Self, Scenario};
    use Sui::Transfer::{Self, ChildRef};
    use Sui::TxContext;

    const ID_BYTES_MISMATCH: u64 = 0;
    const VALUE_MISMATCH: u64 = 1;
    const OBJECT_ID_NOT_FOUND: u64 = 2;

    struct Object has key, store {
        id: ID::VersionedID,
        value: u64,
    }

    struct Wrapper has key {
        id: ID::VersionedID,
        child: Object,
    }

    struct Parent has key {
        id: ID::VersionedID,
        child: ChildRef<Object>,
    }

    struct MultiChildParent has key {
        id: ID::VersionedID,
        child1: ChildRef<Object>,
        child2: ChildRef<Object>,
    }

    #[test]
    fun test_wrap_unwrap() {
        let sender = @0x0;
        let scenario = TestScenario::begin(&sender);
        {
            let id = TestScenario::new_id(&mut scenario);
            let obj = Object { id, value: 10 };
            Transfer::transfer(obj, copy sender);
        };
        // now, object gets wrapped
        TestScenario::next_tx(&mut scenario, &sender);
        {
            let id = TestScenario::new_id(&mut scenario);
            let child = TestScenario::take_owned<Object>(&mut scenario);
            let wrapper = Wrapper { id, child };
            Transfer::transfer(wrapper, copy sender);
        };
        // wrapped object should no longer be removable, but wrapper should be
        TestScenario::next_tx(&mut scenario, &sender);
        {
            assert!(!TestScenario::can_take_owned<Object>(&scenario), 0);
            assert!(TestScenario::can_take_owned<Wrapper>(&scenario), 1);
        }
    }

    #[test]
    fun test_remove_then_return() {
        let sender = @0x0;
        let scenario = TestScenario::begin(&sender);
        {
            let id = TestScenario::new_id(&mut scenario);
            let obj = Object { id, value: 10 };
            Transfer::transfer(obj, copy sender);
        };
        // object gets removed, then returned
        TestScenario::next_tx(&mut scenario, &sender);
        {
            let object = TestScenario::take_owned<Object>(&mut scenario);
            TestScenario::return_owned(&mut scenario, object);
        };
        // Object should remain accessible
        TestScenario::next_tx(&mut scenario, &sender);
        {
            assert!(TestScenario::can_take_owned<Object>(&scenario), 0);
        }
    }

    #[test]
    fun test_return_and_update() {
        let sender = @0x0;
        let scenario = TestScenario::begin(&sender);
        {
            let id = TestScenario::new_id(&mut scenario);
            let obj = Object { id, value: 10 };
            Transfer::transfer(obj, copy sender);
        };
        TestScenario::next_tx(&mut scenario, &sender);
        {
            let obj = TestScenario::take_owned<Object>(&mut scenario);
            assert!(obj.value == 10, 0);
            obj.value = 100;
            TestScenario::return_owned(&mut scenario, obj);
        };
        TestScenario::next_tx(&mut scenario, &sender);
        {
            let obj = TestScenario::take_owned<Object>(&mut scenario);
            assert!(obj.value == 100, 1);
            TestScenario::return_owned(&mut scenario, obj);
        }
    }

    #[test]
    fun test_remove_during_tx() {
        let sender = @0x0;
        let scenario = TestScenario::begin(&sender);
        {
            let id = TestScenario::new_id(&mut scenario);
            let obj = Object { id, value: 10 };
            Transfer::transfer(obj, copy sender);
            // an object transferred during the tx shouldn't be available in that tx
            assert!(!TestScenario::can_take_owned<Object>(&scenario), 0)
        };
    }

    #[test]
    #[expected_failure(abort_code = 5 /* EALREADY_REMOVED_OBJECT */)]
    fun test_double_remove() {
        let sender = @0x0;
        let scenario = TestScenario::begin(&sender);
        {
            let id = TestScenario::new_id(&mut scenario);
            let obj = Object { id, value: 10 };
            Transfer::transfer(obj, copy sender);
        };
        TestScenario::next_tx(&mut scenario, &sender);
        {
            let obj1 = TestScenario::take_owned<Object>(&mut scenario);
            let obj2 = TestScenario::take_owned<Object>(&mut scenario);
            TestScenario::return_owned(&mut scenario, obj1);
            TestScenario::return_owned(&mut scenario, obj2);
        }
    }

    #[test]
    fun test_three_owners() {
        // make sure an object that goes from addr1 -> addr2 -> addr3 can only be accessed by
        // the appropriate owner at each stage
        let addr1 = @0x0;
        let addr2 = @0x1;
        let addr3 = @0x2;
        let scenario = TestScenario::begin(&addr1);
        {
            let id = TestScenario::new_id(&mut scenario);
            let obj = Object { id, value: 10 };
            // self-transfer
            Transfer::transfer(obj, copy addr1);
        };
        // addr1 -> addr2
        TestScenario::next_tx(&mut scenario, &addr1);
        {
            let obj = TestScenario::take_owned<Object>(&mut scenario);
            Transfer::transfer(obj, copy addr2)
        };
        // addr1 cannot access
        TestScenario::next_tx(&mut scenario, &addr1);
        {
            assert!(!TestScenario::can_take_owned<Object>(&scenario), 0);
        };
        // addr2 -> addr3
        TestScenario::next_tx(&mut scenario, &addr2);
        {
            let obj = TestScenario::take_owned<Object>(&mut scenario);
            Transfer::transfer(obj, copy addr3)
        };
        // addr1 cannot access
        TestScenario::next_tx(&mut scenario, &addr1);
        {
            assert!(!TestScenario::can_take_owned<Object>(&scenario), 0);
        };
        // addr2 cannot access
        TestScenario::next_tx(&mut scenario, &addr2);
        {
            assert!(!TestScenario::can_take_owned<Object>(&scenario), 0);
        };
        // addr3 *can* access
        TestScenario::next_tx(&mut scenario, &addr3);
        {
            assert!(TestScenario::can_take_owned<Object>(&scenario), 0);
        }
    }

    #[test]
    fun test_transfer_then_delete() {
        let tx1_sender = @0x0;
        let tx2_sender = @0x1;
        let scenario = TestScenario::begin(&tx1_sender);
        // send an object to tx2_sender
        let id_bytes;
        {
            let id = TestScenario::new_id(&mut scenario);
            id_bytes = *ID::inner(&id);
            let obj = Object { id, value: 100 };
            Transfer::transfer(obj, copy tx2_sender);
            // sender cannot access the object
            assert!(!TestScenario::can_take_owned<Object>(&scenario), 0);
        };
        // check that tx2_sender can get the object, and it's the same one
        TestScenario::next_tx(&mut scenario, &tx2_sender);
        {
            assert!(TestScenario::can_take_owned<Object>(&scenario), 1);
            let received_obj = TestScenario::take_owned<Object>(&mut scenario);
            let Object { id: received_id, value } = received_obj;
            assert!(ID::inner(&received_id) == &id_bytes, ID_BYTES_MISMATCH);
            assert!(value == 100, VALUE_MISMATCH);
            ID::delete(received_id);
        };
        // check that the object is no longer accessible after deletion
        TestScenario::next_tx(&mut scenario, &tx2_sender);
        {
            assert!(!TestScenario::can_take_owned<Object>(&scenario), 2);
        }
    }

    #[test]
    fun test_take_owned_by_id() {
        let sender = @0x0;
        let scenario = TestScenario::begin(&sender);
        let versioned_id1 = TestScenario::new_id(&mut scenario);
        let versioned_id2 = TestScenario::new_id(&mut scenario);
        let versioned_id3 = TestScenario::new_id(&mut scenario);
        let id1 = *ID::inner(&versioned_id1);
        let id2 = *ID::inner(&versioned_id2);
        let id3 = *ID::inner(&versioned_id3);
        {
            let obj1 = Object { id: versioned_id1, value: 10 };
            let obj2 = Object { id: versioned_id2, value: 20 };
            let obj3 = Object { id: versioned_id3, value: 30 };
            Transfer::transfer(obj1, copy sender);
            Transfer::transfer(obj2, copy sender);
            Transfer::transfer(obj3, copy sender);
        };
        TestScenario::next_tx(&mut scenario, &sender);
        {
            assert!(
                TestScenario::can_take_owned_by_id<Object>(&mut scenario, id1),
                OBJECT_ID_NOT_FOUND
            );
            assert!(
                TestScenario::can_take_owned_by_id<Object>(&mut scenario, id2),
                OBJECT_ID_NOT_FOUND
            );
            assert!(
                TestScenario::can_take_owned_by_id<Object>(&mut scenario, id3),
                OBJECT_ID_NOT_FOUND
            );
            let obj1 = TestScenario::take_owned_by_id<Object>(&mut scenario, id1);
            let obj3 = TestScenario::take_owned_by_id<Object>(&mut scenario, id3);
            let obj2 = TestScenario::take_owned_by_id<Object>(&mut scenario, id2);
            assert!(obj1.value == 10, VALUE_MISMATCH);
            assert!(obj2.value == 20, VALUE_MISMATCH);
            assert!(obj3.value == 30, VALUE_MISMATCH);
            TestScenario::return_owned(&mut scenario, obj1);
            TestScenario::return_owned(&mut scenario, obj2);
            TestScenario::return_owned(&mut scenario, obj3);
        };
    }

    #[test]
    fun test_get_last_created_object_id() {
        let sender = @0x0;
        let scenario = TestScenario::begin(&sender);
        {
            let versioned_id = TestScenario::new_id(&mut scenario);
            let id = *ID::inner(&versioned_id);
            let obj = Object { id: versioned_id, value: 10 };
            Transfer::transfer(obj, copy sender);
            let ctx = TestScenario::ctx(&mut scenario);
            assert!(id == TxContext::last_created_object_id(ctx), 0);
        };
    }

    #[test]
    fun test_take_child_object() {
        let sender = @0x0;
        let scenario = TestScenario::begin(&sender);
        create_parent_and_object(&mut scenario);

        TestScenario::next_tx(&mut scenario, &sender);
        {
            // sender cannot take object directly.
            assert!(!TestScenario::can_take_owned<Object>(&scenario), 0);
            // sender can take parent, however.
            assert!(TestScenario::can_take_owned<Parent>(&scenario), 0);

            let parent = TestScenario::take_owned<Parent>(&mut scenario);
            // Make sure we can take the child object with the parent object.
            let child = TestScenario::take_child_object<Parent, Object>(&mut scenario, &parent);
            TestScenario::return_owned(&mut scenario, parent);
            TestScenario::return_owned(&mut scenario, child);
        };
    }

    #[expected_failure(abort_code = 3 /* EMPTY_INVENTORY */)]
    #[test]
    fun test_take_child_object_incorrect_signer() {
        let sender = @0x0;
        let scenario = TestScenario::begin(&sender);
        create_parent_and_object(&mut scenario);

        TestScenario::next_tx(&mut scenario, &sender);
        let parent = TestScenario::take_owned<Parent>(&mut scenario);

        let another = @0x1;
        TestScenario::next_tx(&mut scenario, &another);
        // This should fail even though we have parent object here.
        // Because the signer doesn't match.
        let child = TestScenario::take_child_object<Parent, Object>(&mut scenario, &parent);
        TestScenario::return_owned(&mut scenario, child);

        TestScenario::return_owned(&mut scenario, parent);
    }

    #[test]
    fun test_take_child_object_by_id() {
        let sender = @0x0;
        let scenario = TestScenario::begin(&sender);
        // Create two children and a parent object.
        let child1 = Object {
            id: TestScenario::new_id(&mut scenario),
            value: 10,
        };
        let child1_id = *ID::id(&child1);
        let child2 = Object {
            id: TestScenario::new_id(&mut scenario),
            value: 20,
        };
        let child2_id = *ID::id(&child2);
        let parent_id = TestScenario::new_id(&mut scenario);
        let (parent_id, child1_ref) = Transfer::transfer_to_object_id(child1, parent_id);
        let (parent_id, child2_ref) = Transfer::transfer_to_object_id(child2, parent_id);

        let parent = MultiChildParent {
            id: parent_id,
            child1: child1_ref,
            child2: child2_ref,
        };
        Transfer::transfer(parent, sender);

        TestScenario::next_tx(&mut scenario, &sender);
            {
                let parent = TestScenario::take_owned<MultiChildParent>(&mut scenario);
                let child1 = TestScenario::take_child_object_by_id<MultiChildParent, Object>(&mut scenario, &parent, child1_id);
                let child2 = TestScenario::take_child_object_by_id<MultiChildParent, Object>(&mut scenario, &parent, child2_id);
                assert!(child1.value == 10, 0);
                assert!(child2.value == 20, 0);
                TestScenario::return_owned(&mut scenario, parent);
                TestScenario::return_owned(&mut scenario, child1);
                TestScenario::return_owned(&mut scenario, child2);
            };
    }

    /// Create object and parent. object is a child of parent.
    /// parent is owned by sender of `scenario`.
    fun create_parent_and_object(scenario: &mut Scenario) {
        let parent_id = TestScenario::new_id(scenario);
        let object = Object {
            id: TestScenario::new_id(scenario),
            value: 10,
        };
        let (parent_id, child) = Transfer::transfer_to_object_id(object, parent_id);
        let parent = Parent {
            id: parent_id,
            child,
        };
        Transfer::transfer(parent, TestScenario::sender(scenario));
    }
}
