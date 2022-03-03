// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::TestScenarioTests {
    use Sui::ID;
    use Sui::TestScenario;
    use Sui::Transfer;

    const ID_BYTES_MISMATCH: u64 = 0;
    const VALUE_MISMATCH: u64 = 1;

    struct Object has key, store {
        id: ID::VersionedID,
        value: u64,
    }

    struct Wrapper has key {
        id: ID::VersionedID,
        child: Object,
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
            let child = TestScenario::remove_object<Object>(&mut scenario);
            let wrapper = Wrapper { id, child };
            Transfer::transfer(wrapper, copy sender);
        };
        // wrapped object should no longer be removable, but wrapper should be
        TestScenario::next_tx(&mut scenario, &sender);
        {
            assert!(!TestScenario::can_remove_object<Object>(&scenario), 0);
            assert!(TestScenario::can_remove_object<Wrapper>(&scenario), 1);
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
            let object = TestScenario::remove_object<Object>(&mut scenario);
            TestScenario::return_object(&mut scenario, object)
        };
        // Object should remain accessible
        TestScenario::next_tx(&mut scenario, &sender);
        {
            assert!(TestScenario::can_remove_object<Object>(&scenario), 0);
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
            let obj = TestScenario::remove_object<Object>(&mut scenario);
            assert!(obj.value == 10, 0);
            obj.value = 100;
            TestScenario::return_object(&mut scenario, obj);
        };
        TestScenario::next_tx(&mut scenario, &sender);
        {
            let obj = TestScenario::remove_object<Object>(&mut scenario);
            assert!(obj.value == 100, 1);
            TestScenario::return_object(&mut scenario, obj);
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
            assert!(!TestScenario::can_remove_object<Object>(&scenario), 0)
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
            let obj1 = TestScenario::remove_object<Object>(&mut scenario);
            let obj2 = TestScenario::remove_object<Object>(&mut scenario);
            TestScenario::return_object(&mut scenario, obj1);
            TestScenario::return_object(&mut scenario, obj2);
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
            let obj = TestScenario::remove_object<Object>(&mut scenario);
            Transfer::transfer(obj, copy addr2)
        };
        // addr1 cannot access
        TestScenario::next_tx(&mut scenario, &addr1);
        {
            assert!(!TestScenario::can_remove_object<Object>(&scenario), 0);
        };
        // addr2 -> addr3
        TestScenario::next_tx(&mut scenario, &addr2);
        {
            let obj = TestScenario::remove_object<Object>(&mut scenario);
            Transfer::transfer(obj, copy addr3)
        };
        // addr1 cannot access
        TestScenario::next_tx(&mut scenario, &addr1);
        {
            assert!(!TestScenario::can_remove_object<Object>(&scenario), 0);
        };
        // addr2 cannot access
        TestScenario::next_tx(&mut scenario, &addr2);
        {
            assert!(!TestScenario::can_remove_object<Object>(&scenario), 0);
        };
        // addr3 *can* access
        TestScenario::next_tx(&mut scenario, &addr3);
        {
            assert!(TestScenario::can_remove_object<Object>(&scenario), 0);
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
            assert!(!TestScenario::can_remove_object<Object>(&scenario), 0);
        };
        // check that tx2_sender can get the object, and it's the same one
        TestScenario::next_tx(&mut scenario, &tx2_sender);
        {
            assert!(TestScenario::can_remove_object<Object>(&scenario), 1);
            let received_obj = TestScenario::remove_object<Object>(&mut scenario);
            let Object { id: received_id, value } = received_obj;
            assert!(ID::inner(&received_id) == &id_bytes, ID_BYTES_MISMATCH);
            assert!(value == 100, VALUE_MISMATCH);
            ID::delete(received_id);
        };
        // check that the object is no longer accessible after deletion
        TestScenario::next_tx(&mut scenario, &tx2_sender);
        {
            assert!(!TestScenario::can_remove_object<Object>(&scenario), 2);
        }
    }
}