module cw::tests {

    use cw::policy;
    use std::option::{Self, Option};
    use std::vector::{borrow};
    use sui::transfer;
    use sui::tx_context::{TxContext};

    struct TestDummy has store { val: u64, admin_cap: Option<policy::ControlledObjectPolicyCap> }

    // define a dummy operation on the object to increment by 1 the value

    struct TestDummyChecks has store, copy, drop {}
    struct TestDummyParams has store, drop {}

    // This is an operation gated by (TestDummyChecks, TestDummyParams) to increment the value by 1
    public fun controlled_inc(self : &mut policy::ControlledObject<TestDummy>, op: policy::AuthorizedOperation<TestDummy, TestDummyChecks, TestDummyParams>){
        let (obj, _ch, _param, _policy_id) = policy::unlock(self, op);
        obj.val = *&obj.val + 1;
    }

    // define a policy to be able to add to the policy

    struct TestDummyAdminChecks has store, copy, drop {
    }
    struct TestDummyAdminParams has store, drop {
        initial : address,
        threshold : vector<address>,
    }

    // Note this call is gated through an authorized operation with (TestDummyAdminChecks, TestDummyAdminParams)
    //
    // This illustrates how the access control logic may be used to modify the access control logic. Here we specifically
    // allow someone with an authorizd operation to add a 2-out-of-3 policy for performing the (TestDummyChecks, TestDummyParams) operation.
    public fun controlled_admin(self : &mut policy::ControlledObject<TestDummy>, op: policy::AuthorizedOperation<TestDummy, TestDummyAdminChecks, TestDummyAdminParams>, ctx: &mut TxContext){
        let (obj, _ch, param, _policy_id) = policy::unlock(self, op);

        // Initialize a policy for an object operation
        let checks = policy::make_checks(TestDummyChecks {});
        let (initcap, execcap) = policy::InitPolicy<TestDummy, TestDummyChecks, TestDummyParams>(option::borrow(&obj.admin_cap), checks, ctx);

        // Initialize a policy
        
        // Setup a 2-out-of-3 capabilities policy.
        let (cap1, out1) = policy::setup_capability<TestDummy>(ctx);
        let (cap2, out2) = policy::setup_capability<TestDummy>(ctx);
        let (cap3, out3) = policy::setup_capability<TestDummy>(ctx);
        let (kn1, out4) = policy::setup_threshold(vector[out1, out2, out3], 2, ctx);
        let final1 = policy::setup_unlock_cap(out4, execcap, ctx);

        // Transfer capabilities to approvers
        transfer::transfer(cap1, *borrow(&param.threshold, 0));
        transfer::transfer(cap2, *borrow(&param.threshold, 1));
        transfer::transfer(cap3, *borrow(&param.threshold, 2));

        // Transfer capabilities to policy initiator
        transfer::transfer(kn1, param.initial);
        transfer::transfer(initcap, param.initial);
        transfer::transfer(final1, param.initial);
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
        let (controlled, policy_cap) = policy::ControlObject(TestDummy { val: 0, admin_cap: option::none() }, &mut ctx);

        // Initialize a policy for an object operation
        let checks = policy::make_checks(TestDummyChecks {});
        let (initcap, execcap) = policy::InitPolicy<TestDummy, TestDummyChecks, TestDummyParams>(&policy_cap, checks, &mut ctx);

        // Initialize an operation on the controlled object
        let op = policy::InitOperation<TestDummy, TestDummyChecks, TestDummyParams>(&initcap, TestDummyParams {}, &mut ctx);

        // Execute the operation with the execution capability
        let granted = policy::AuthorizeOperation<TestDummy, TestDummyChecks, TestDummyParams>(&execcap, op);

        // Invoke controlled operation on the object
        controlled_inc(&mut controlled, granted);

        // policy::cancel_operation(op);
        policy::drop_init_cap(initcap);
        policy::drop_exec_cap(execcap);
        transfer::transfer(controlled, dummy_address_A);

    }

     #[test]
    public fun test_controlled_combined() {
        use sui::tx_context;
        // use sui::object;
        use cw::policy;
        use sui::transfer;

        // Some dummy addresses
        let dummy_address_A = @0xAAAA;

        // create a dummy TxContext for testing
        let ctx = tx_context::dummy();

        // Define a controlled object
        let (controlled, policy_cap) = policy::ControlObject(TestDummy { val: 0, admin_cap: option::none() }, &mut ctx);
        // Inject the admin capability (that allows to define policies) within the object
        let mut_obj = policy::controlled_mut<TestDummy>(&mut controlled, policy_cap);
        mut_obj.admin_cap = option::some(policy_cap);

        // Initialize a policy for an object operation
        let checks = policy::make_checks(TestDummyChecks {});
        let (initcap, execcap) = policy::InitPolicy<TestDummy, TestDummyChecks, TestDummyParams>(&policy_cap, checks, &mut ctx);

        // Initialize a policy
        
        // Setup a 2-out-of-3 capabilities policy.
        let (cap1, out1) = policy::setup_capability<TestDummy>(&mut ctx);
        let (cap2, out2) = policy::setup_capability<TestDummy>(&mut ctx);
        let (cap3, out3) = policy::setup_capability<TestDummy>(&mut ctx);
        let (kn1, out4) = policy::setup_threshold(vector[out1, out2, out3], 2,&mut ctx);
        let final1 = policy::setup_unlock_cap(out4, execcap, &mut ctx);

        // Actually use the policy

        // Initialize an operation on the controlled object
        let op = policy::InitOperation<TestDummy, TestDummyChecks, TestDummyParams>(&initcap, TestDummyParams {}, &mut ctx);
        let op_id = policy::op_id(&op);

        // Gather signatures, 2 capabilities are enough.
        let sig1 = policy::authorize_capability(&cap1, op_id, &mut ctx);
        let sig2 = policy::authorize_capability(&cap2, op_id, &mut ctx);
        let aggr1 = policy::authorize_threshold(&kn1, vector[0,1], vector[sig1, sig2], &mut ctx);
        let granted = policy::authorize_unlock_op(&final1, aggr1, op);

        // Invoke controlled operation on the object
        controlled_inc(&mut controlled, granted);

        // clean-up
        policy::drop_init_cap(initcap);
        policy::drop_capability(cap1);
        policy::drop_capability(cap2);
        policy::drop_capability(cap3);
        policy::drop_threshold(kn1);
        policy::drop_unlock_cap<TestDummy, TestDummyChecks, TestDummyParams>(final1);

        transfer::transfer(controlled, dummy_address_A);
    }

    #[test]
    public fun test_controlled_flow() {
        use sui::test_scenario;

        // create test addresses representing users
        let admin = @0xBABE;
        let initiator = @0xCAFE;
        let approver1 = @0xFACE1;
        let approver2 = @0xFACE2;
        let approver3 = @0xFACE3;

        // first transaction to emulate module initialization
        let scenario_val = test_scenario::begin(admin);

        let scenario = &mut scenario_val;
        {
            // First the admin makes the controlled object and sets the policy associated with the operation

            let ctx = test_scenario::ctx(scenario); 

            // Define a controlled object
            let (controlled, policy_cap) = policy::ControlObject(TestDummy { val: 0, admin_cap: option::none() }, ctx);
            // Inject the admin capability (that allows to define policies) within the object
            let mut_obj = policy::controlled_mut<TestDummy>(&mut controlled, policy_cap);
            mut_obj.admin_cap = option::some(policy_cap);

            // Initialize a policy for an object operation
            let checks = policy::make_checks(TestDummyChecks {});
            let (initcap, execcap) = policy::InitPolicy<TestDummy, TestDummyChecks, TestDummyParams>(&policy_cap, checks, ctx);

            // Initialize a policy
            
            // Setup a 2-out-of-3 capabilities policy.
            let (cap1, out1) = policy::setup_capability<TestDummy>(ctx);
            let (cap2, out2) = policy::setup_capability<TestDummy>(ctx);
            let (cap3, out3) = policy::setup_capability<TestDummy>(ctx);
            let (kn1, out4) = policy::setup_threshold(vector[out1, out2, out3], 2,ctx);
            let final1 = policy::setup_unlock_cap(out4, execcap, ctx);

            // Send approver capabilities to their owners
            transfer::transfer(cap1, approver1);
            transfer::transfer(cap2, approver2);
            transfer::transfer(cap3, approver3);

            // Send auth flow logic and operation init capability to the operation initiator
            transfer::transfer(initcap, initiator);
            transfer::transfer(kn1, initiator);
            transfer::transfer(final1, initiator);
            
            // Share the controlled object for all to access
            transfer::share_object(controlled);
            
        };
        test_scenario::next_tx(scenario, initiator);
        let op_id = {
            let initcap = test_scenario::take_from_sender<policy::PolicyOperationInitCap<TestDummyChecks, TestDummyParams>>(scenario);
            let ctx = test_scenario::ctx(scenario); 

            // Initialize an operation on the controlled object
            let op = policy::InitOperation<TestDummy, TestDummyChecks, TestDummyParams>(&initcap, TestDummyParams {}, ctx);
            let op_id = policy::op_id(&op);
            transfer::transfer(op, initiator);

            test_scenario::return_to_address(initiator, initcap);
            op_id
        };
        test_scenario::next_tx(scenario, approver1);
        {

            // First approver takes the capability and authorizes the operation

            let cap = test_scenario::take_from_sender<policy::AuthCapability<TestDummy>>(scenario);
            let ctx = test_scenario::ctx(scenario); 

            let sig = policy::authorize_capability(&cap, op_id, ctx);
            transfer::transfer(sig, initiator);

            test_scenario::return_to_address(approver1, cap);

        };
        test_scenario::next_tx(scenario, approver2);
        {

            // Second approver takes the capability and authorizes the operation

            let cap = test_scenario::take_from_sender<policy::AuthCapability<TestDummy>>(scenario);
            let ctx = test_scenario::ctx(scenario); 

            let sig = policy::authorize_capability(&cap, op_id, ctx);
            transfer::transfer(sig, initiator);

            test_scenario::return_to_address(approver2, cap);

        };
        test_scenario::next_tx(scenario, initiator);
        {
            // Initiator gathers approvals + uses the auth logic to approve operation
            // then performs the operation on the controlled object.

            let sig2 = test_scenario::take_from_sender<policy::AuthOutput<TestDummy>>(scenario);
            let sig1 = test_scenario::take_from_sender<policy::AuthOutput<TestDummy>>(scenario);
            let kn1 =  test_scenario::take_from_sender<policy::AuthThreshold<TestDummy>>(scenario);
            let final1 = test_scenario::take_from_sender<policy::AuthUnlockCap<TestDummy, policy::PolicyOperationAuthCap<TestDummyChecks, TestDummyParams>>>(scenario);
            let op = test_scenario::take_from_sender<policy::PolicyOperation<TestDummyChecks, TestDummyParams>>(scenario);
            let controlled = test_scenario::take_shared<policy::ControlledObject<TestDummy>>(scenario);

            let ctx = test_scenario::ctx(scenario); 

            // Initiator gathers approvals + uses the auth logic to approve operation ... 
            let aggr1 = policy::authorize_threshold(&kn1, vector[0, 1], vector[sig1, sig2], ctx);
            let granted = policy::authorize_unlock_op(&final1, aggr1, op);

            // ... Invoke controlled operation on the object
            controlled_inc(&mut controlled, granted);

            test_scenario::return_to_address(initiator, kn1);
            test_scenario::return_to_address(initiator, final1);
            test_scenario::return_shared(controlled);


        };

        test_scenario::end(scenario_val);
    }

}