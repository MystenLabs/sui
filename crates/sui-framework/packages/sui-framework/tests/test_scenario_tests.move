// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::test_scenario_tests {
    use sui::test_scenario;

    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public struct Wrapper has key {
        id: UID,
        child: Object,
    }

    #[test]
    fun test_wrap_unwrap() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        {
            let id = scenario.new_object();
            let obj = Object { id, value: 10 };
            transfer::transfer(obj, copy sender);
        };
        // now, object gets wrapped
        scenario.next_tx(sender);
        {
            let id = scenario.new_object();
            let child = scenario.take_from_sender<Object>();
            let wrapper = Wrapper { id, child };
            transfer::transfer(wrapper, copy sender);
        };
        // wrapped object should no longer be removable, but wrapper should be
        scenario.next_tx(sender);
        {
            assert!(!scenario.has_most_recent_for_sender<Object>());
            assert!(scenario.has_most_recent_for_sender<Wrapper>());
        };
        scenario.end();
    }

    #[test]
    fun test_remove_then_return() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        {
            let id = scenario.new_object();
            let obj = Object { id, value: 10 };
            transfer::public_transfer(obj, copy sender);
        };
        // object gets removed, then returned
        scenario.next_tx(sender);
        {
            let object = scenario.take_from_sender<Object>();
            scenario.return_to_sender(object);
        };
        // Object should remain accessible
        scenario.next_tx(sender);
        {
            assert!(scenario.has_most_recent_for_sender<Object>());
        };
        scenario.end();
    }

    #[test]
    fun test_return_and_update() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        {
            let id = scenario.new_object();
            let obj = Object { id, value: 10 };
            transfer::public_transfer(obj, copy sender);
        };
        scenario.next_tx(sender);
        {
            let mut obj = scenario.take_from_sender<Object>();
            assert!(obj.value == 10);
            obj.value = 100;
            scenario.return_to_sender(obj);
        };
        scenario.next_tx(sender);
        {
            let obj = scenario.take_from_sender<Object>();
            assert!(obj.value == 100);
            scenario.return_to_sender(obj);
        };
        scenario.end();
    }

    #[test]
    fun test_remove_during_tx() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        {
            let id = scenario.new_object();
            let obj = Object { id, value: 10 };
            transfer::public_transfer(obj, copy sender);
            // an object transferred during the tx shouldn't be available in that tx
            assert!(!scenario.has_most_recent_for_sender<Object>())
        };
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EEmptyInventory)]
    fun test_double_remove() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        {
            let id = scenario.new_object();
            let obj = Object { id, value: 10 };
            transfer::public_transfer(obj, copy sender);
        };
        scenario.next_tx(sender);
        {
            let obj1 = scenario.take_from_sender<Object>();
            let obj2 = scenario.take_from_sender<Object>();
            scenario.return_to_sender(obj1);
            scenario.return_to_sender(obj2);
        };
        scenario.end();
    }

    #[test]
    fun test_three_owners() {
        // make sure an object that goes from addr1 -> addr2 -> addr3 can only be accessed by
        // the appropriate owner at each stage
        let addr1 = @0x0;
        let addr2 = @0x1;
        let addr3 = @0x2;
        let mut scenario = test_scenario::begin(addr1);
        {
            let id = scenario.new_object();
            let obj = Object { id, value: 10 };
            // self-transfer
            transfer::public_transfer(obj, copy addr1);
        };
        // addr1 -> addr2
        scenario.next_tx(addr1);
        {
            let obj = scenario.take_from_sender<Object>();
            transfer::public_transfer(obj, copy addr2)
        };
        // addr1 cannot access
        scenario.next_tx(addr1);
        {
            assert!(!scenario.has_most_recent_for_sender<Object>());
        };
        // addr2 -> addr3
        scenario.next_tx(addr2);
        {
            let obj = scenario.take_from_sender<Object>();
            transfer::public_transfer(obj, copy addr3)
        };
        // addr1 cannot access
        scenario.next_tx(addr1);
        {
            assert!(!scenario.has_most_recent_for_sender<Object>());
        };
        // addr2 cannot access
        scenario.next_tx(addr2);
        {
            assert!(!scenario.has_most_recent_for_sender<Object>());
        };
        // addr3 *can* access
        scenario.next_tx(addr3);
        {
            assert!(scenario.has_most_recent_for_sender<Object>());
        };
        scenario.end();
    }

    #[test]
    fun test_transfer_then_delete() {
        let tx1_sender = @0x0;
        let tx2_sender = @0x1;
        let mut scenario = test_scenario::begin(tx1_sender);
        // send an object to tx2_sender
        let id_bytes;
        {
            let id = scenario.new_object();
            id_bytes = id.to_inner();
            let obj = Object { id, value: 100 };
            transfer::public_transfer(obj, copy tx2_sender);
            // sender cannot access the object
            assert!(!scenario.has_most_recent_for_sender<Object>());
        };
        // check that tx2_sender can get the object, and it's the same one
        scenario.next_tx(tx2_sender);
        {
            assert!(scenario.has_most_recent_for_sender<Object>());
            let received_obj = scenario.take_from_sender<Object>();
            let Object { id: received_id, value } = received_obj;
            assert!(received_id.to_inner() == id_bytes);
            assert!(value == 100);
            received_id.delete();
        };
        // check that the object is no longer accessible after deletion
        scenario.next_tx(tx2_sender);
        {
            assert!(!scenario.has_most_recent_for_sender<Object>());
        };
        scenario.end();
    }

    #[test]
    fun test_get_owned_obj_ids() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let uid3 = scenario.new_object();
        let id1 = uid1.to_inner();
        let id2 = uid2.to_inner();
        let id3 = uid3.to_inner();
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            let obj3 = Object { id: uid3, value: 30 };
            transfer::public_transfer(obj1, copy sender);
            transfer::public_transfer(obj2, copy sender);
            transfer::public_transfer(obj3, copy sender);
        };
        scenario.next_tx(sender);
        let ids = scenario.ids_for_sender<Object>();
        assert!(ids == vector[id1, id2, id3]);
        scenario.end();
    }

    #[test]
    fun test_take_owned_by_id() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let uid3 = scenario.new_object();
        let id1 = uid1.to_inner();
        let id2 = uid2.to_inner();
        let id3 = uid3.to_inner();
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            let obj3 = Object { id: uid3, value: 30 };
            transfer::public_transfer(obj1, copy sender);
            transfer::public_transfer(obj2, copy sender);
            transfer::public_transfer(obj3, copy sender);
        };
        scenario.next_tx(sender);
        {
            let obj1 = scenario.take_from_sender_by_id<Object>(id1);
            let obj3 = scenario.take_from_sender_by_id<Object>(id3);
            let obj2 = scenario.take_from_sender_by_id<Object>(id2);
            assert!(obj1.value == 10);
            assert!(obj2.value == 20);
            assert!(obj3.value == 30);
            scenario.return_to_sender(obj1);
            scenario.return_to_sender(obj2);
            scenario.return_to_sender(obj3);
        };
        scenario.end();
    }

    #[test]
    fun test_get_last_created_object_id() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        {
            let id = scenario.new_object();
            let id_addr = id.to_address();
            let obj = Object { id, value: 10 };
            transfer::public_transfer(obj, copy sender);
            let ctx = scenario.ctx();
            assert!(id_addr == ctx.last_created_object_id());
        };
        scenario.end();
    }

    #[test]
    fun test_take_shared_by_id() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let uid3 = scenario.new_object();
        let id1 = uid1.uid_to_inner();
        let id2 = uid2.uid_to_inner();
        let id3 = uid3.uid_to_inner();
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            let obj3 = Object { id: uid3, value: 30 };
            transfer::public_share_object(obj1);
            transfer::public_share_object(obj2);
            transfer::public_share_object(obj3)
        };
        scenario.next_tx(sender);
        {
            let obj1 = scenario.take_shared_by_id<Object>(id1);
            let obj3 = scenario.take_shared_by_id<Object>(id3);
            let obj2 = scenario.take_shared_by_id<Object>(id2);
            assert!(obj1.value == 10);
            assert!(obj2.value == 20);
            assert!(obj3.value == 30);
            test_scenario::return_shared(obj1);
            test_scenario::return_shared(obj2);
            test_scenario::return_shared(obj3);
        };
        scenario.end();
    }

    #[test]
    fun test_take_shared() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        {
            let obj1 = Object { id: uid1, value: 10 };
            transfer::public_share_object(obj1);
        };
        scenario.next_tx(sender);
        {
            assert!(test_scenario::has_most_recent_shared<Object>());
            let obj1 = scenario.take_shared<Object>();
            assert!(obj1.value == 10);
            test_scenario::return_shared(obj1);
        };
        scenario.end();
    }

    #[test]
    fun test_delete_shared() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        {
            let obj1 = Object { id: uid1, value: 10 };
            transfer::public_share_object(obj1);
        };
        scenario.next_tx(sender);
        {
            assert!(test_scenario::has_most_recent_shared<Object>());
            let obj1 = scenario.take_shared<Object>();
            assert!(obj1.value == 10);
            let Object { id, value: _ } = obj1;
            id.delete();
        };
        scenario.end();
    }

    #[test]
    fun test_take_immutable_by_id() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let uid3 = scenario.new_object();
        let id1 = uid1.to_inner();
        let id2 = uid2.to_inner();
        let id3 = uid3.to_inner();
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            let obj3 = Object { id: uid3, value: 30 };
            transfer::public_freeze_object(obj1);
            transfer::public_freeze_object(obj2);
            transfer::public_freeze_object(obj3)
        };
        scenario.next_tx(sender);
        {
            let obj1 = scenario.take_immutable_by_id<Object>(id1);
            let obj3 = scenario.take_immutable_by_id<Object>(id3);
            let obj2 = scenario.take_immutable_by_id<Object>(id2);
            assert!(obj1.value == 10);
            assert!(obj2.value == 20);
            assert!(obj3.value == 30);
            test_scenario::return_immutable(obj1);
            test_scenario::return_immutable(obj2);
            test_scenario::return_immutable(obj3);
        };
        scenario.end();
    }

    #[test]
    fun test_take_immutable() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        {
            let obj1 = Object { id: uid1, value: 10 };
            transfer::public_freeze_object(obj1);
        };
        scenario.next_tx(sender);
        {
            assert!(test_scenario::has_most_recent_immutable<Object>());
            let obj1 = scenario.take_immutable<Object>();
            assert!(obj1.value == 10);
            test_scenario::return_immutable(obj1);
        };
        scenario.end();
    }

    // Happy path test: Receive two objects from the same object in the same
    // transaction.
    #[test]
    fun test_receive_object() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let uid3 = scenario.new_object();
        let id1 = uid1.uid_to_inner();
        let id2 = uid2.uid_to_inner();
        let id3 = uid3.uid_to_inner();
        let id1_addr = uid1.to_address();
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            let obj3 = Object { id: uid3, value: 30 };
            transfer::public_transfer(obj1, sender);
            transfer::public_transfer(obj2, id1_addr);
            transfer::public_transfer(obj3, id1_addr)
        };
        test_scenario::next_tx(&mut scenario, sender);
        {

            let mut parent = scenario.take_from_sender_by_id<Object>(id1);
            let t2 = test_scenario::receiving_ticket_by_id<Object>(id2);
            let t3 = test_scenario::receiving_ticket_by_id<Object>(id3);
            let obj2 = transfer::receive(&mut parent.id, t2);
            let obj3 = transfer::receive(&mut parent.id, t3);
            assert!(parent.value == 10);
            assert!(obj2.value == 20);
            assert!(obj3.value == 30);
            scenario.return_to_sender(parent);
            transfer::public_transfer(obj2, id1_addr);
            transfer::public_transfer(obj3, id1_addr)
        };
        scenario.end();
    }

    // Happy path test: Receive a single object from an object in a transaction.
    #[test]
    fun test_receive_for_object() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let id1 = uid1.uid_to_inner();
        let id1_addr = uid1.to_address();
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            transfer::public_transfer(obj1, sender);
            transfer::public_transfer(obj2, id1_addr);
        };
        scenario.next_tx(sender);
        {

            let mut parent = scenario.take_from_sender_by_id<Object>(id1);
            let t2 = test_scenario::most_recent_receiving_ticket<Object>(&id1);
            let obj2 = transfer::receive(&mut parent.id, t2);
            assert!(parent.value == 10);
            assert!(obj2.value == 20);
            scenario.return_to_sender(parent);
            transfer::public_transfer(obj2, id1_addr);
        };
        scenario.end();
    }

    // Make sure that we properly handle the case where we receive an object
    // and don't need to deallocate the receiving ticket and underlying object
    // at the end of the transaction.
    #[test]
    fun test_receive_object_multiple_in_row() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let id1 = uid1.uid_to_inner();
        let id1_addr = uid1.to_address();
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            transfer::public_transfer(obj1, sender);
            transfer::public_transfer(obj2, id1_addr);
        };
        scenario.next_tx(sender);
        {
            let mut parent: Object = scenario.take_from_sender_by_id(id1);
            let t2: transfer::Receiving<Object> = test_scenario::most_recent_receiving_ticket(&id1);
            let obj2 = transfer::receive(&mut parent.id, t2);
            assert!(parent.value == 10);
            assert!(obj2.value == 20);
            scenario.return_to_sender(parent);
            transfer::public_transfer(obj2, id1_addr);
        };
        scenario.next_tx(sender);
        {
            let mut parent: Object = scenario.take_from_sender_by_id(id1);
            let t2: transfer::Receiving<Object> = test_scenario::most_recent_receiving_ticket(&id1);
            let obj2 = transfer::receive(&mut parent.id, t2);
            assert!(parent.value == 10);
            assert!(obj2.value == 20);
            scenario.return_to_sender(parent);
            transfer::public_transfer(obj2, id1_addr);
        };
        scenario.end();
    }

    // Make sure that we properly handle the case where we don't receive an
    // object after allocating a ticket, and then receiving it in the next
    // transaction.
    #[test]
    fun test_no_receive_object_then_use_next_tx() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let id1 = uid1.uid_to_inner();
        let id1_addr = uid1.to_address();
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            transfer::public_transfer(obj1, sender);
            transfer::public_transfer(obj2, id1_addr);
        };
        scenario.next_tx(sender);
        {
            // allocate a receiving ticket in this transaction, but don't use it or return it.
            test_scenario::most_recent_receiving_ticket<Object>(&id1);
        };
        scenario.next_tx(sender);
        {
            let mut parent: Object = scenario.take_from_sender_by_id(id1);
            // Get the receiving ticket that was allocated in the previous
            // transaction, again. If we failed to return unused receiving
            // tickets at the end of the transaction above this will fail.
            let t2: transfer::Receiving<Object> = test_scenario::most_recent_receiving_ticket(&id1);
            let obj2 = transfer::receive(&mut parent.id, t2);
            assert!(parent.value == 10);
            assert!(obj2.value == 20);
            scenario.return_to_sender(parent);
            transfer::public_transfer(obj2, id1_addr);
        };
        scenario.end();
    }

    // Try to receive an object that has been shared. We should be unable to
    // allocate the receiving ticket for this object.
    #[test]
    #[expected_failure(abort_code = test_scenario::EObjectNotFound)]
    fun test_receive_object_shared() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let id1 = uid1.uid_to_inner();
        {
            let obj1 = Object { id: uid1, value: 20 };
            transfer::public_share_object(obj1);
        };
        scenario.next_tx(sender);
        {
            let _t2 = test_scenario::receiving_ticket_by_id<Object>(id1);
        };
        scenario.end();
    }

    // Try to allocate multiple receiving tickets for the same object in a
    // single transaction. We should be unable to allocate the second ticket.
    #[test]
    #[expected_failure(abort_code = test_scenario::EReceivingTicketAlreadyAllocated)]
    fun test_receive_object_double_allocate_ticket() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let id1 = uid1.uid_to_inner();
        {
            let obj1 = Object { id: uid1, value: 20 };
            transfer::public_transfer(obj1, sender);
        };
        scenario.next_tx(sender);
        {
            let _t2 = test_scenario::receiving_ticket_by_id<Object>(id1);
            let _t2 = test_scenario::receiving_ticket_by_id<Object>(id1);
        };
        scenario.end();
    }

    // Test that we can allocate a receiving ticket, return it, and then
    // allocate it again within the same transaction.
    #[test]
    fun test_receive_double_allocate_ticket_return_between() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let id1 = uid1.uid_to_inner();
        {
            let obj1 = Object { id: uid1, value: 20 };
            transfer::public_transfer(obj1, sender);
        };
        scenario.next_tx(sender);
        {
            let t2 = test_scenario::receiving_ticket_by_id<Object>(id1);
            test_scenario::return_receiving_ticket(t2);
            let _t2 = test_scenario::receiving_ticket_by_id<Object>(id1);
        };
        scenario.end();
    }

    // Test that we can allocate a receiving ticket, return it, and then
    // allocate it again, and the resulting ticket is valid and works as
    // expected.
    #[test]
    fun test_receive_double_allocate_ticket_return_between_then_use() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let uid3 = scenario.new_object();
        let id1 = uid1.uid_to_inner();
        let id2 = uid2.uid_to_inner();
        let id3 = uid3.uid_to_inner();
        let id1_addr = uid1.to_address();
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            let obj3 = Object { id: uid3, value: 30 };
            transfer::public_transfer(obj1, sender);
            transfer::public_transfer(obj2, id1_addr);
            transfer::public_transfer(obj3, id1_addr)
        };
        scenario.next_tx(sender);
        {

            let mut parent = scenario.take_from_sender_by_id<Object>(id1);
            let t2 = test_scenario::receiving_ticket_by_id<Object>(id2);
            test_scenario::return_receiving_ticket(t2);
            let t2 = test_scenario::receiving_ticket_by_id<Object>(id2);
            let t3 = test_scenario::receiving_ticket_by_id<Object>(id3);
            let obj2 = transfer::receive(&mut parent.id, t2);
            let obj3 = transfer::receive(&mut parent.id, t3);
            assert!(parent.value == 10);
            assert!(obj2.value == 20);
            assert!(obj3.value == 30);
            scenario.return_to_sender(parent);
            transfer::public_transfer(obj2, id1_addr);
            transfer::public_transfer(obj3, id1_addr)
        };
        scenario.end();
    }

    // Test that we can allocate a receiving ticket, return it, allocate it
    // again, then allocate a different ticket. Mutate one of them, then
    // return, and then transfer the objects.
    // Then read the mutated object and verify that the mutation persisted to the object.
    #[test]
    fun test_receive_double_allocate_ticket_return_between_then_use_then_check() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let uid3 = scenario.new_object();
        let id1 = uid1.uid_to_inner();
        let id2 = uid2.uid_to_inner();
        let id3 = uid3.uid_to_inner();
        let id1_addr = uid1.to_address();
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            let obj3 = Object { id: uid3, value: 30 };
            transfer::public_transfer(obj1, sender);
            transfer::public_transfer(obj2, id1_addr);
            transfer::public_transfer(obj3, id1_addr)
        };
        scenario.next_tx(sender);
        {

            let mut parent = scenario.take_from_sender_by_id<Object>(id1);
            let t2 = test_scenario::receiving_ticket_by_id<Object>(id2);
            test_scenario::return_receiving_ticket(t2);
            let t2 = test_scenario::receiving_ticket_by_id<Object>(id2);
            let t3 = test_scenario::receiving_ticket_by_id<Object>(id3);
            let mut obj2 = transfer::receive(&mut parent.id, t2);
            let obj3 = transfer::receive(&mut parent.id, t3);
            assert!(parent.value == 10);
            assert!(obj2.value == 20);
            assert!(obj3.value == 30);
            obj2.value = 42;
            scenario.return_to_sender(parent);
            transfer::public_transfer(obj2, sender);
            transfer::public_transfer(obj3, sender)
        };
        scenario.next_tx(sender);
        {
            let obj = scenario.take_from_sender_by_id<Object>(id2);
            assert!(obj.value == 42);
            scenario.return_to_sender(obj);
        };
        scenario.end();
    }

    // Test that we can allocate a receiving ticket, and then drop it.
    #[test]
    fun test_unused_receive_ticket() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let uid3 = scenario.new_object();
        let id2 = uid2.uid_to_inner();
        let id3 = uid3.uid_to_inner();
        let id1_addr = uid1.to_address();
        {
            let obj1 = Object { id: uid1, value: 10 };
            let obj2 = Object { id: uid2, value: 20 };
            let obj3 = Object { id: uid3, value: 30 };
            transfer::public_transfer(obj1, sender);
            transfer::public_transfer(obj2, id1_addr);
            transfer::public_transfer(obj3, id1_addr)
        };
        scenario.next_tx(sender);
        {
            let _t2 = test_scenario::receiving_ticket_by_id<Object>(id2);
            let _t3 = test_scenario::receiving_ticket_by_id<Object>(id3);
        };
        scenario.end();
    }


    #[test]
    fun test_unreturned_objects() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid1 = scenario.new_object();
        let uid2 = scenario.new_object();
        let uid3 = scenario.new_object();
        {
            transfer::public_share_object(Object { id: uid1, value: 10 });
            transfer::public_freeze_object(Object { id: uid2, value: 10 });
            transfer::public_transfer(Object { id: uid3, value: 10 }, sender);
        };
        scenario.next_tx(sender);
        let shared = scenario.take_shared<Object>();
        let imm = scenario.take_immutable<Object>();
        let owned = scenario.take_from_sender<Object>();
        scenario.next_tx(sender);
        scenario.next_epoch(sender);
        scenario.next_tx(sender);
        scenario.next_epoch(sender);
        scenario.end();
        transfer::public_share_object(shared);
        transfer::public_freeze_object(imm);
        transfer::public_transfer(owned, sender);
    }

    #[test]
    fun test_later_epoch() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);

        let ts0 = scenario.ctx().epoch_timestamp_ms();

        // epoch timestamp doesn't change between transactions
        scenario.next_tx(sender);
        let ts1 = scenario.ctx().epoch_timestamp_ms();
        assert!(ts1 == ts0);

        // ...or between epochs when `next_epoch` is used
        scenario.next_epoch(sender);
        let ts2 = scenario.ctx().epoch_timestamp_ms();
        assert!(ts2 == ts1);

        // ...but does change when `later_epoch` is used
        scenario.later_epoch(42, sender);
        let ts3 = scenario.ctx().epoch_timestamp_ms();
        assert!(ts3 == ts2 + 42);

        // ...and persists across further transactions
        scenario.next_tx(sender);
        let ts4 = scenario.ctx().epoch_timestamp_ms();
        assert!(ts4 == ts3);

        // ...and epochs
        scenario.next_epoch(sender);
        let ts5 = scenario.ctx().epoch_timestamp_ms();
        assert!(ts5 == ts4);

        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EInvalidSharedOrImmutableUsage)]
    fun test_invalid_shared_usage() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        {
            let id = scenario.new_object();
            let obj1 = Object { id, value: 10 };
            transfer::public_share_object(obj1);
        };
        scenario.next_tx(sender);
        {
            let obj1 = scenario.take_shared<Object>();
            transfer::public_freeze_object(obj1);
        };
        scenario.next_tx(sender);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EInvalidSharedOrImmutableUsage)]
    fun test_invalid_immutable_usage() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        {
            let id = scenario.new_object();
            let obj1 = Object { id, value: 10 };
            transfer::public_freeze_object(obj1);
        };
        scenario.next_tx(sender);
        {
            let obj1 = scenario.take_immutable<Object>();
            transfer::public_transfer(obj1, @0x0);
        };
        scenario.next_tx(sender);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EInvalidSharedOrImmutableUsage)]
    fun test_modify_immutable() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        {
            let id = scenario.new_object();
            let obj1 = Object { id, value: 10 };
            transfer::public_freeze_object(obj1);
        };
        scenario.next_tx(sender);
        let mut obj1 = scenario.take_immutable<Object>();
        scenario.next_tx(sender);
        obj1.value = 100;
        scenario.next_tx(sender);
        test_scenario::return_immutable(obj1);
        scenario.next_tx(sender);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::ECantReturnObject)]
    fun test_invalid_address_return() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let id = scenario.new_object();
        scenario.return_to_sender(Object { id, value: 10 });
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::ECantReturnObject)]
    fun test_invalid_shared_return() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let id = scenario.new_object();
        test_scenario::return_shared(Object { id, value: 10 });
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::ECantReturnObject)]
    fun test_invalid_immutable_return() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let id = scenario.new_object();
        test_scenario::return_immutable(Object { id, value: 10 });
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EEmptyInventory)]
    fun test_empty_inventory() {
        let sender = @0x0;
        let scenario = test_scenario::begin(sender);
        scenario.return_to_sender(scenario.take_from_sender<Object>());
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EEmptyInventory)]
    fun test_empty_inventory_shared() {
        let sender = @0x0;
        let scenario = test_scenario::begin(sender);
        scenario.return_to_sender(scenario.take_shared<Object>());
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EEmptyInventory)]
    fun test_empty_inventory_immutable() {
        let sender = @0x0;
        let scenario = test_scenario::begin(sender);
        scenario.return_to_sender(scenario.take_immutable<Object>());
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EObjectNotFound)]
    fun test_object_not_found() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid = scenario.new_object();
        let id = uid.to_inner();
        scenario.return_to_sender(scenario.take_from_sender_by_id<Object>(id));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EObjectNotFound)]
    fun test_object_not_found_shared() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid = scenario.new_object();
        let id = uid.to_inner();
        scenario.return_to_sender(scenario.take_shared_by_id<Object>(id));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EObjectNotFound)]
    fun test_object_not_found_immutable() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid = scenario.new_object();
        let id = uid.to_inner();
        scenario.return_to_sender(scenario.take_immutable_by_id<Object>(id));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EObjectNotFound)]
    fun test_wrong_object_type() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid = scenario.new_object();
        let id = uid.to_inner();
        transfer::public_transfer(Object { id: uid, value: 10 }, sender);
        scenario.return_to_sender(scenario.take_from_sender_by_id<Wrapper>(id));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EObjectNotFound)]
    fun test_wrong_object_type_shared() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid = scenario.new_object();
        let id = uid.to_inner();
        transfer::public_share_object(Object { id: uid, value: 10 });
        test_scenario::return_shared(scenario.take_shared_by_id<Wrapper>(id));
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EObjectNotFound)]
    fun test_wrong_object_type_immutable() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let uid = scenario.new_object();
        let id = uid.to_inner();
        transfer::public_freeze_object(Object { id: uid, value: 10 });
        test_scenario::return_immutable(scenario.take_immutable_by_id<Wrapper>(id));
        abort 42
    }

    #[test]
    fun test_dynamic_field_still_borrowed() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut parent = scenario.new_object();
        sui::dynamic_field::add(&mut parent, b"", 10);
        let r = sui::dynamic_field::borrow<vector<u8>, u64>(&parent, b"");
        scenario.end();
        assert!(*r == 10);
        parent.delete();
    }

    #[test]
    fun test_dynamic_object_field_still_borrowed() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut parent = scenario.new_object();
        let id = scenario.new_object();
        sui::dynamic_object_field::add(&mut parent, b"", Object { id, value: 10});
        let obj = sui::dynamic_object_field::borrow<vector<u8>, Object>(&parent, b"");
        scenario.end();
        assert!(obj.value == 10);
        parent.delete();
    }

    #[test]
    fun test_dynamic_object_field_not_retrievable() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut parent = scenario.new_object();
        let uid = scenario.new_object();
        let obj = Object { id: uid, value: 10};
        let id = object::id(&obj);
        transfer::public_transfer(obj, sender);
        scenario.next_tx(sender);
        assert!(test_scenario::has_most_recent_for_address<Object>(sender));
        let obj = scenario.take_from_sender<Object>();
        assert!(object::id(&obj) == id);
        assert!(!test_scenario::has_most_recent_for_address<Object>(sender));
        sui::dynamic_object_field::add(&mut parent, b"", obj);
        scenario.next_tx(sender);
        assert!(!test_scenario::has_most_recent_for_address<Object>(sender));
        scenario.end();
        parent.delete();
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EInvalidSharedOrImmutableUsage)]
    fun test_dynamic_field_shared_misuse() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut parent = scenario.new_object();
        let uid = scenario.new_object();
        let obj = Object { id: uid, value: 10};
        let id = object::id(&obj);
        transfer::public_share_object(obj);
        scenario.next_tx(sender);
        let obj = scenario.take_shared<Object>();
        assert!(object::id(&obj) == id);
        // wraps the object
        sui::dynamic_field::add(&mut parent, b"", obj);
        scenario.next_tx(sender);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EInvalidSharedOrImmutableUsage)]
    fun test_dynamic_field_immutable_misuse() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut parent = scenario.new_object();
        let uid = scenario.new_object();
        let obj = Object { id: uid, value: 10};
        let id = object::id(&obj);
        transfer::public_freeze_object(obj);
        scenario.next_tx(sender);
        let obj = scenario.take_immutable<Object>();
        assert!(object::id(&obj) == id);
        // wraps the object
        sui::dynamic_field::add(&mut parent, b"", obj);
        scenario.next_tx(sender);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EInvalidSharedOrImmutableUsage)]
    fun test_dynamic_object_field_shared_misuse() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut parent = scenario.new_object();
        let uid = scenario.new_object();
        let obj = Object { id: uid, value: 10};
        let id = object::id(&obj);
        transfer::public_share_object(obj);
        scenario.next_tx(sender);
        let obj = scenario.take_shared<Object>();
        assert!(object::id(&obj) == id);
        sui::dynamic_object_field::add(&mut parent, b"", obj);
        scenario.next_tx(sender);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = test_scenario::EInvalidSharedOrImmutableUsage)]
    fun test_dynamic_object_field_immutable_misuse() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut parent = scenario.new_object();
        let uid = scenario.new_object();
        let obj = Object { id: uid, value: 10};
        let id = object::id(&obj);
        transfer::public_freeze_object(obj);
        scenario.next_tx(sender);
        let obj = scenario.take_immutable<Object>();
        assert!(object::id(&obj) == id);
        sui::dynamic_object_field::add(&mut parent, b"", obj);
        scenario.next_tx(sender);
        abort 42
    }

    public struct E1(u64) has copy, drop;

    #[test]
    fun test_events() {
        use sui::event;
        use sui::test_utils::assert_eq;

        // calling test_scenario::end should dump events emitted during previous txes
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let e0 = E1(0);
        event::emit(e0);
        event::emit(e0);
        assert_eq(event::num_events(), 2);

        // test scenario users should make assertions about events here, before calling
        // next_tx
        let effects = scenario.next_tx(sender);
        assert_eq(effects.num_user_events(), 2);
        assert_eq(event::num_events(), 0);

        let e1 = E1(1);
        event::emit(e1);
        assert_eq(event::num_events(), 1);
        assert_eq(event::events_by_type<E1>()[0], e1);
        let effects = scenario.end();
        // end should also dump events
        assert_eq(effects.num_user_events(), 1);
        assert_eq(event::num_events(), 0);
    }
}
