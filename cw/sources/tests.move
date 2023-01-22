module cw::tests {

     use cw::policy;

    struct TestDummy has store { val: u64 }
    struct TestDummyChecks has copy, drop {}
    struct TestDummyParams has drop {}

    public fun controlled_inc(self : &mut policy::ControlledObject<TestDummy>, op: policy::AuthorizedOperation<TestDummy, TestDummyChecks, TestDummyParams>){
        let (obj, _ch, _param) = policy::unlock(self, op);
        obj.val = *&obj.val + 1;
    }

    #[test]
    public fun test_integration_simple() {
        use sui::tx_context;
        // use sui::object;
       

        // create a dummy TxContext for testing
        let ctx = tx_context::dummy();

        // Setup a 2-out-of-3 capabilities policy.
        let (cap1, out1) = policy::setup_capability<TestDummy>(&mut ctx);
        let (cap2, out2) = policy::setup_capability<TestDummy>(&mut ctx);
        let (cap3, out3) = policy::setup_capability<TestDummy>(&mut ctx);

        let (kn1, out4) = policy::setup_threshold(vector[out1, out2, out3], 2,&mut ctx);

        // clean-up
        policy::drop_capability(cap1);
        policy::drop_capability(cap2);
        policy::drop_capability(cap3);
        policy::drop_threshold(kn1);

        // temporary
        policy::drop_setup_output(out4);
    }

     #[test]
    public fun test_controlled_simple() {
        use sui::tx_context;
        // use sui::object;
        use cw::policy;
        use sui::transfer;

        // Some dummy addresses
        let dummy_address_A = @0xAAAA;

        // create a dummy TxContext for testing
        let ctx = tx_context::dummy();

        // Define a controlled object
        let (controlled, policy_cap) = policy::ControlObject(TestDummy { val: 0 }, &mut ctx);

        // Initialize a policy for an object operation
        let checks = policy::make_checks(TestDummyChecks {});
        let (initcap, execcap) = policy::InitPolicy<TestDummy, TestDummyChecks, TestDummyParams>(&policy_cap, checks, &mut ctx);

        // Initialize an operation on the controlled object
        let op = policy::InitOperation<TestDummy, TestDummyChecks, TestDummyParams>(&initcap, TestDummyParams {}, &mut ctx);

        // Execute the operation with the execution capability
        let granted = policy::ExecOperation<TestDummy, TestDummyChecks, TestDummyParams>(&execcap, op);

        controlled_inc(&mut controlled, granted);

        // policy::cancel_operation(op);
        policy::drop_exec_cap(execcap);
        transfer::transfer(controlled, dummy_address_A);

    }


}