// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::test_scenarioTests {
    use sui::object;
    use sui::test_scenario::{Self, Scenario};
    use sui::transfer;
    use sui::tx_context;

    const ID_BYTES_MISMATCH: u64 = 0;
    const VALUE_MISMATCH: u64 = 1;
    const OBJECT_ID_NOT_FOUND: u64 = 2;

    struct Object has key, store {
        id: object::UID,
        value: u64,
    }

    struct Wrapper has key {
        id: object::UID,
        child: Object,
    }

    struct Parent has key {
        id: object::UID,
        child: object::ID,
    }

    struct MultiChildParent has key {
        id: object::UID,
        child1: object::ID,
        child2: object::ID,
    }

    #[test]
    fun test_wrap_unwrap() {
        let sender = @0x0;
        let scenario = test_scenario::begin(&sender);
        {
            let id = test_scenario::new_object(&mut scenario);
            let obj = Object { id, value: 10 };
            transfer::transfer(obj, copy sender);
        };
        // now, object gets wrapped
        test_scenario::next_tx(&mut scenario, &sender);
        {
            let id = test_scenario::new_object(&mut scenario);
            let child = test_scenario::take_owned<Object>(&mut scenario);
            let wrapper = Wrapper { id, child };
            transfer::transfer(wrapper, copy sender);
        };
        // wrapped object should no longer be removable, but wrapper should be
        test_scenario::next_tx(&mut scenario, &sender);
        {
            assert!(!test_scenario::can_take_owned<Object>(&scenario), 0);
            assert!(test_scenario::can_take_owned<Wrapper>(&scenario), 1);
        }
    }

    #[test]
    fun test_remove_then_return() {
        let sender = @0x0;
        let scenario = test_scenario::begin(&sender);
        {
            let id = test_scenario::new_object(&mut scenario);
            let obj = Object { id, value: 10 };
            transfer::transfer(obj, copy sender);
        };
        // object gets removed, then returned
        test_scenario::next_tx(&mut scenario, &sender);
        {
            let object = test_scenario::take_owned<Object>(&mut scenario);
            test_scenario::return_owned(&mut scenario, object);
        };
        // Object should remain accessible
        test_scenario::next_tx(&mut scenario, &sender);
        {
            assert!(test_scenario::can_take_owned<Object>(&scenario), 0);
        }
    }

    #[test]
    fun test_return_and_update() {
        let sender = @0x0;
        let scenario = test_scenario::begin(&sender);
        {
            let id = test_scenario::new_object(&mut scenario);
            let obj = Object { id, value: 10 };
            transfer::transfer(obj, copy sender);
        };
        test_scenario::next_tx(&mut scenario, &sender);
        {
            let obj = test_scenario::take_owned<Object>(&mut scenario);
            assert!(obj.value == 10, 0);
            obj.value = 100;
            test_scenario::return_owned(&mut scenario, obj);
        };
        test_scenario::next_tx(&mut scenario, &sender);
        {
            let obj = test_scenario::take_owned<Object>(&mut scenario);
            assert!(obj.value == 100, 1);
            test_scenario::return_owned(&mut scenario, obj);
        }
    }

    #[test]
    fun test_remove_during_tx() {
        let sender = @0x0;
        let scenario = test_scenario::begin(&sender);
        {
            let id = test_scenario::new_object(&mut scenario);
            let obj = Object { id, value: 10 };
            transfer::transfer(obj, copy sender);
            // an object transferred during the tx shouldn't be available in that tx
            assert!(!test_scenario::can_take_owned<Object>(&scenario), 0)
        };
    }

    #[test]
    #[expected_failure(abort_code = 5 /* EALREADY_REMOVED_OBJECT */)]
    fun test_double_remove() {
        let sender = @0x0;
        let scenario = test_scenario::begin(&sender);
        {
            let id = test_scenario::new_object(&mut scenario);
            let obj = Object { id, value: 10 };
            transfer::transfer(obj, copy sender);
        };
        test_scenario::next_tx(&mut scenario, &sender);
        {
            let obj1 = test_scenario::take_owned<Object>(&mut scenario);
            let obj2 = test_scenario::take_owned<Object>(&mut scenario);
            test_scenario::return_owned(&mut scenario, obj1);
            test_scenario::return_owned(&mut scenario, obj2);
        }
    }

    #[test]
    fun test_three_owners() {
        // make sure an object that goes from addr1 -> addr2 -> addr3 can only be accessed by
        // the appropriate owner at each stage
        let addr1 = @0x0;
        let addr2 = @0x1;
        let addr3 = @0x2;
        let scenario = test_scenario::begin(&addr1);
        {
            let id = test_scenario::new_object(&mut scenario);
            let obj = Object { id, value: 10 };
            // self-transfer
            transfer::transfer(obj, copy addr1);
        };
        // addr1 -> addr2
        test_scenario::next_tx(&mut scenario, &addr1);
        {
            let obj = test_scenario::take_owned<Object>(&mut scenario);
            transfer::transfer(obj, copy addr2)
        };
        // addr1 cannot access
        test_scenario::next_tx(&mut scenario, &addr1);
        {
            assert!(!test_scenario::can_take_owned<Object>(&scenario), 0);
        };
        // addr2 -> addr3
        test_scenario::next_tx(&mut scenario, &addr2);
        {
            let obj = test_scenario::take_owned<Object>(&mut scenario);
            transfer::transfer(obj, copy addr3)
        };
        // addr1 cannot access
        test_scenario::next_tx(&mut scenario, &addr1);
        {
            assert!(!test_scenario::can_take_owned<Object>(&scenario), 0);
        };
        // addr2 cannot access
        test_scenario::next_tx(&mut scenario, &addr2);
        {
            assert!(!test_scenario::can_take_owned<Object>(&scenario), 0);
        };
        // addr3 *can* access
        test_scenario::next_tx(&mut scenario, &addr3);
        {
            assert!(test_scenario::can_take_owned<Object>(&scenario), 0);
        }
    }

    #[test]
    fun test_transfer_then_delete() {
        let tx1_sender = @0x0;
        let tx2_sender = @0x1;
        let scenario = test_scenario::begin(&tx1_sender);
        // send an object to tx2_sender
        let id_bytes;
        {
            let id = test_scenario::new_object(&mut scenario);
            id_bytes = object::uid_to_inner(&id);
            let obj = Object { id, value: 100 };
            transfer::transfer(obj, copy tx2_sender);
            // sender cannot access the object
            assert!(!test_scenario::can_take_owned<Object>(&scenario), 0);
        };
        // check that tx2_sender can get the object, and it's the same one
        test_scenario::next_tx(&mut scenario, &tx2_sender);
        {
            assert!(test_scenario::can_take_owned<Object>(&scenario), 1);
            let received_obj = test_scenario::take_owned<Object>(&mut scenario);
            let Object { id: received_id, value } = received_obj;
            assert!(object::uid_to_inner(&received_id) == id_bytes, ID_BYTES_MISMATCH);
            assert!(value == 100, VALUE_MISMATCH);
            object::delete(received_id);
        };
        // check that the object is no longer accessible after deletion
        test_scenario::next_tx(&mut scenario, &tx2_sender);
        {
            assert!(!test_scenario::can_take_owned<Object>(&scenario), 2);
        }
    }

    #[test]
    fun test_take_owned_by_id() {
        let sender = @0x0;
        let scenario = test_scenario::begin(&sender);
        let uid1 = test_scenario::new_object(&mut scenario);
        let uid2 = test_scenario::new_object(&mut scenario);
        let uid3 = test_scenario::new_object(&mut scenario);
        let id1 = object::uid_to_inner(&uid1);
        let id2 = object::uid_to_inner(&uid2);
        let id3 = object::uid_to_inner(&uid3);
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            let obj3 = Object { id: uid3, value: 30 };
            transfer::transfer(obj1, copy sender);
            transfer::transfer(obj2, copy sender);
            transfer::transfer(obj3, copy sender);
        };
        test_scenario::next_tx(&mut scenario, &sender);
        {
            assert!(
                test_scenario::can_take_owned_by_id<Object>(&mut scenario, id1),
                OBJECT_ID_NOT_FOUND
            );
            assert!(
                test_scenario::can_take_owned_by_id<Object>(&mut scenario, id2),
                OBJECT_ID_NOT_FOUND
            );
            assert!(
                test_scenario::can_take_owned_by_id<Object>(&mut scenario, id3),
                OBJECT_ID_NOT_FOUND
            );
            let obj1 = test_scenario::take_owned_by_id<Object>(&mut scenario, id1);
            let obj3 = test_scenario::take_owned_by_id<Object>(&mut scenario, id3);
            let obj2 = test_scenario::take_owned_by_id<Object>(&mut scenario, id2);
            assert!(obj1.value == 10, VALUE_MISMATCH);
            assert!(obj2.value == 20, VALUE_MISMATCH);
            assert!(obj3.value == 30, VALUE_MISMATCH);
            test_scenario::return_owned(&mut scenario, obj1);
            test_scenario::return_owned(&mut scenario, obj2);
            test_scenario::return_owned(&mut scenario, obj3);
        };
    }

    #[test]
    fun test_get_last_created_object_id() {
        let sender = @0x0;
        let scenario = test_scenario::begin(&sender);
        {
            let id = test_scenario::new_object(&mut scenario);
            let id_addr = object::uid_to_address(&id);
            let obj = Object { id, value: 10 };
            transfer::transfer(obj, copy sender);
            let ctx = test_scenario::ctx(&mut scenario);
            assert!(id_addr == tx_context::last_created_object_id(ctx), 0);
        };
    }

    #[test]
    fun test_get_last_created_owned_object() {
        let sender = @0x0;
        let scenario = &mut test_scenario::begin(&sender);
		create_and_transfer_object(scenario, 1);
		test_scenario::next_tx(scenario, &sender);
		{
			let obj = test_scenario::take_last_created_owned<Object>(scenario);
			assert!(obj.value == 1, VALUE_MISMATCH);
			test_scenario::return_owned(scenario, obj);
		};
		create_and_transfer_object(scenario, 2);
		test_scenario::next_tx(scenario, &sender);
		{
			let obj = test_scenario::take_last_created_owned<Object>(scenario);
			assert!(obj.value == 2, VALUE_MISMATCH);
			test_scenario::return_owned(scenario, obj);
		};
		create_and_transfer_object(scenario, 3);
		test_scenario::next_tx(scenario, &sender);
		{
			let obj = test_scenario::take_last_created_owned<Object>(scenario);
			assert!(obj.value == 3, VALUE_MISMATCH);
			test_scenario::return_owned(scenario, obj);
		};
    }

    #[test]
    fun test_take_child_object() {
        let sender = @0x0;
        let scenario = test_scenario::begin(&sender);
        create_parent_and_object(&mut scenario);

        test_scenario::next_tx(&mut scenario, &sender);
        {
            // sender cannot take object directly.
            assert!(!test_scenario::can_take_owned<Object>(&scenario), 0);
            // sender can take parent, however.
            assert!(test_scenario::can_take_owned<Parent>(&scenario), 0);

            let parent = test_scenario::take_owned<Parent>(&mut scenario);
            // Make sure we can take the child object with the parent object.
            let child = test_scenario::take_child_object<Parent, Object>(&mut scenario, &parent);
            test_scenario::return_owned(&mut scenario, parent);
            test_scenario::return_owned(&mut scenario, child);
        };
    }

    #[expected_failure(abort_code = 3 /* EMPTY_INVENTORY */)]
    #[test]
    fun test_take_child_object_incorrect_signer() {
        let sender = @0x0;
        let scenario = test_scenario::begin(&sender);
        create_parent_and_object(&mut scenario);

        test_scenario::next_tx(&mut scenario, &sender);
        let parent = test_scenario::take_owned<Parent>(&mut scenario);

        let another = @0x1;
        test_scenario::next_tx(&mut scenario, &another);
        // This should fail even though we have parent object here.
        // Because the signer doesn't match.
        let child = test_scenario::take_child_object<Parent, Object>(&mut scenario, &parent);
        test_scenario::return_owned(&mut scenario, child);

        test_scenario::return_owned(&mut scenario, parent);
    }

    #[test]
    fun test_take_child_object_by_id() {
        let sender = @0x0;
        let scenario = test_scenario::begin(&sender);
        // Create two children and a parent object.
        let child1 = Object {
            id: test_scenario::new_object(&mut scenario),
            value: 10,
        };
        let child1_id = object::id(&child1);
        let child2 = Object {
            id: test_scenario::new_object(&mut scenario),
            value: 20,
        };
        let child2_id = object::id(&child2);
        let parent_id = test_scenario::new_object(&mut scenario);
        transfer::transfer_to_object_id(child1, &parent_id);
        transfer::transfer_to_object_id(child2, &parent_id);

        let parent = MultiChildParent {
            id: parent_id,
            child1: child1_id,
            child2: child2_id,
        };
        transfer::transfer(parent, sender);

        test_scenario::next_tx(&mut scenario, &sender);
            {
                let parent = test_scenario::take_owned<MultiChildParent>(&mut scenario);
                let child1 = test_scenario::take_child_object_by_id<MultiChildParent, Object>(&mut scenario, &parent, child1_id);
                let child2 = test_scenario::take_child_object_by_id<MultiChildParent, Object>(&mut scenario, &parent, child2_id);
                assert!(child1.value == 10, 0);
                assert!(child2.value == 20, 0);
                test_scenario::return_owned(&mut scenario, parent);
                test_scenario::return_owned(&mut scenario, child1);
                test_scenario::return_owned(&mut scenario, child2);
            };
    }

    /// Create object and parent. object is a child of parent.
    /// parent is owned by sender of `scenario`.
    fun create_parent_and_object(scenario: &mut Scenario) {
        let parent_id = test_scenario::new_object(scenario);
        let object = Object {
            id: test_scenario::new_object(scenario),
            value: 10,
        };
        let child_id = object::id(&object);
        transfer::transfer_to_object_id(object, &parent_id);
        let parent = Parent {
            id: parent_id,
            child: child_id,
        };
        transfer::transfer(parent, test_scenario::sender(scenario));
    }

    /// Create an object and transfer it to the sender of `scenario`.
    fun create_and_transfer_object(scenario: &mut Scenario, value: u64) {
        let object = Object {
            id: test_scenario::new_object(scenario),
            value,
        };
        transfer::transfer(object, test_scenario::sender(scenario));
    }
}
