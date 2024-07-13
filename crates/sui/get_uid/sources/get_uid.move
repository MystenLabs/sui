/// Module: get_uid
module get_uid::get_uid {
    use sui::test_scenario;

    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    #[test]
    fun start_end_test_scenario() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        scenario.end();
    }

    #[test]
    fun test_transfer() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let id = scenario.new_object();
        let obj = Object {id, value: 10};
        transfer::transfer(obj, sender);
        scenario.end();
    }

    #[test]
    fun test_get_id() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let id = scenario.new_object();
        let obj = Object {id, value: 10};
        let id = object::id(&obj);
        transfer::transfer(obj, sender);
        scenario.end();
    }

    #[test]
    fun test_global_references() {
        let sender = @0x0;
        let mut scenario = test_scenario::begin(sender);
        let mut parent = scenario.new_object();
        sui::dynamic_field::add(&mut parent, b"", 10);
        let r = sui::dynamic_field::borrow<vector<u8>, u64>(&parent, b"");
        scenario.end();
        assert!(*r == 10);
        parent.delete();
    }
}
