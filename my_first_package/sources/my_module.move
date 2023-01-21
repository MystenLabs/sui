module my_first_package::my_module {
    // Part 1: imports
    use sui::object::{Self, ID, UID};
    use sui::tx_context::{Self, TxContext};
    use std::vector::{length, push_back, borrow, pop_back, destroy_empty};

    // Part 2: struct definitions

    // AuthSetup Outputs are used to define and initialize the authoritzation policy
    struct AuthSetupOutput<phantom T> { // Cannot store / drop must use
        for_auth_id: ID,
        // TODO: here bind an operation type ID?
    }

    fun drop_setup_output<T>(out: AuthSetupOutput<T>) {
        let AuthSetupOutput { for_auth_id: _for_auth_id } = out;
    }

    fun drop_setup_outputs<T>(out: vector<AuthSetupOutput<T>>) {

        while (length(&out) > 0) {
            let out_item = pop_back(&mut out); 
            let AuthSetupOutput<T> { for_auth_id: _for_auth_id } = out_item;
        };

        destroy_empty(out);
    }

    // AuthOutput is used to denote that an authorization path has been 
    // validated for an operation with a given ID
    struct AuthOutput<phantom T> has key, store { // Can store, send and drop
        id: UID,
        valid_id: ID,
        for_operation_id: ID, // the Operation ID for this authorization
    }

    fun inner_drop<T>(out: AuthOutput<T>) {
        let AuthOutput<T> { id, valid_id: _, for_operation_id: _ } = out;
        object::delete(id);
    }

    // This is a leaf of an Authorization policy that requires a signature from a sender
    struct AuthSignerApproval<phantom T> {
        id : UID,
        addr: address,
    }

    fun setup_signer_approval<T> (addr: address, ctx: &mut TxContext) : (AuthSignerApproval<T>, AuthSetupOutput<T>) {
        let id = object::new(ctx);
        let for_auth_id = object::uid_to_inner(&id);

        let auth_output = AuthSetupOutput<T> { for_auth_id, };
        let auth_object = AuthSignerApproval<T> { id, addr, };

        (auth_object, auth_output)
    }

    fun authorize_signer_approval<T>(auth: &AuthSignerApproval<T>, for_operation_id: ID, ctx: &mut TxContext) :  AuthOutput<T>{
        let sender = &tx_context::sender(ctx);
        inner_authorize_signer_approval(sender, auth, for_operation_id, ctx)
    }

    fun inner_authorize_signer_approval<T>(sender: &address, auth: &AuthSignerApproval<T>, for_operation_id: ID, ctx: &mut TxContext) :  AuthOutput<T> {
        assert!(sender ==  &auth.addr, 0);

        let id = object::new(ctx);
        let valid_id = object::uid_to_inner(&auth.id);
        
        AuthOutput {
            id, valid_id, for_operation_id,
        }
    }

    fun drop<T>(auth: AuthSignerApproval<T>) {
        let AuthSignerApproval { id, addr: _ } = auth;
        object::delete(id);
    }

    // A K out of N authorization policy that requires a threshold k to activate.
    // An AND policy has a threshold of N out of N.
    // An OR policy has a threshold of 1 out of N.
    struct AuthThreshold<phantom T> {
        id : UID,
        input_ids: vector<ID>,
        k : u64,
    }

    fun setup_threshold<T>(inputs : vector<AuthSetupOutput<T>>, k:u64, ctx: &mut TxContext) : (AuthThreshold<T>, AuthSetupOutput<T>)
    {
        // Check bounds: 1 <= k <= N
        assert!(1 <= k, 0);
        assert!(k <= length(&inputs), 0);

        // Record all the obejct IDs that need to send an output for this authorization to activate.
        let input_ids = vector<ID>[];
        let i = 0;
        let len : u64 = length(&inputs);
        while (i < len){
            let elem : &AuthSetupOutput<T> = borrow(&inputs, i);
            let inner_id : ID = *&elem.for_auth_id;
            push_back(&mut input_ids, inner_id);
            i = i + 1;
        };

        // drop by hand
        drop_setup_outputs(inputs);

        // Create authorization object and output to setup downstream authorizations.
        let id = object::new(ctx);
        let for_auth_id = object::uid_to_inner(&id);

        let auth_output = AuthSetupOutput<T> { for_auth_id, };
        let auth_object = AuthThreshold<T> { id, input_ids, k };

        (auth_object, auth_output)

    } 

    fun authorize_threshold<T>(auth: &AuthThreshold<T>, positions:vector<u64>, inputs : vector<AuthOutput<T>>, ctx: &mut TxContext) : AuthOutput<T> {
        assert!(length(&positions) == auth.k, 0);
        assert!(length(&inputs) == auth.k, 0);

        // Record the operation ID to ensure it is the same for all inputs
        let for_operation_id = *&borrow(&inputs, 0).for_operation_id;

        while(length(&positions) > 0) {
            // For each input we do checks.
            let pos = pop_back(&mut positions);
            let inp = pop_back(&mut inputs);

            // Check that input positions are increasing
            if (length(&positions) > 0) {
                // there is a previous element which must have a position
                // smaller than this one. Lets check this.
                assert!(*borrow(&positions, length(&positions) - 1) < pos, 0);
            };

            // Check that the input corresponds to the ID expected.
            assert!(*borrow(&auth.input_ids, pos) == *&inp.valid_id, 0);
            // Check the input is for the operation expected
            assert!(*&inp.valid_id == for_operation_id, 0);
            inner_drop(inp);
        };

        // This is empty by now per the condition for exiting while loop
        destroy_empty(inputs);

        // We create the output for this auth bound to the operation.
        let id = object::new(ctx);
        let valid_id = object::uid_to_inner(&auth.id);
        
        AuthOutput {
            id, valid_id, for_operation_id,
        }
    }

    struct AuthUnlockCap<phantom T, C> {
        id : UID,
        input_id: ID,
        cap: C,
    }

    fun setup_unlock_cap<T, C>(input : AuthSetupOutput<T>, cap : C, ctx: &mut TxContext) : AuthUnlockCap<T, C> {
        let id = object::new(ctx);
        let input_id = input.for_auth_id;

        drop_setup_output(input);

        AuthUnlockCap {
            id, input_id, cap
        }
    }

    fun authorize_unlock_op<T, O, CH : copy, P>(auth: &AuthUnlockCap<T, PolicyOperationExecCap<CH, P>>, input: AuthOutput<T>, op: PolicyOperation<CH, P>) : AuthorizedOperation<O, CH, P>{
        // Check this is the correct input signal to unlock capability
        assert!(input.valid_id == auth.input_id, 0);

        // Check the signal is tied to the operation ID
        let op_id =  object::uid_to_inner(&op.id);
        assert!(input.for_operation_id == op_id, 0);
        inner_drop(input);

        // Move to create an authorization for the operation
        ExecOperation(&auth.cap, op)
    }


    // TODO: extract an authorized operation once we define the controlled resources and ops hierarchy

    // Operation Checks, Controlled Operations, Controlled Objects

    // The checks procedure defines checks that an invocstion to a controlled procedure must under take. 
    struct PolicyChecks<CH : copy> has copy, drop {
        checks: CH,
    }

    struct PolicyOperation<CH : copy, P> {
        id: UID,
        init_for_policy_id: ID, 
        controlled_object_id : ID,
        checks: PolicyChecks<CH>,
        params: P,
    }

    struct PolicyOperationInitCap<CH : copy, phantom P> has copy, drop {
        init_for_policy_id: ID,
        controlled_object_id : ID,
        checks: PolicyChecks<CH>,
    }
    struct PolicyOperationExecCap<phantom CH, phantom P> {
        id: UID,
    }
    struct PolicyOperationRevokeCookie has copy, drop {}

    //struct ControlledObject<O> {
    //    id: UID,
    //    object: O,
    //}

    struct ControlledObject<phantom O>{}
    struct ControlledObjectPolicyCap<phantom O>{
        controlled_object_id: ID
    }

    struct AuthorizedOperation<phantom O, CH : copy, P> {
        op: PolicyOperation<CH, P>,
    }

    // Initialize a policy associated with the given controlled object and the given checks (implicitelly associated with an operation).
    // The revoke cooking is given, and will be required when the operation will be executed, to allow dropping the cookie to invalidate the policy
    // The function returns a capability to initiate an operation, as well as to execute and operation.
    fun InitPolicy<O, CH: drop + copy, P>(object: &ControlledObjectPolicyCap<O>, checks: PolicyChecks<CH>, _revoke_cookie: &PolicyOperationRevokeCookie, ctx: &mut TxContext) : (PolicyOperationInitCap<CH, P>, PolicyOperationExecCap<CH, P>) {
        let id = object::new(ctx);
        let init_for_policy_id = object::uid_to_inner(&id);
        let controlled_object_id = object.controlled_object_id;
        ( PolicyOperationInitCap { init_for_policy_id, controlled_object_id, checks }, PolicyOperationExecCap { id, } )
    } 

    /// Initialize an operation that should be within the policy
    /// 
    fun InitOperation<O, CH:copy, P>(init_cap: &PolicyOperationInitCap<CH, P>, params : P, ctx: &mut TxContext): PolicyOperation<CH, P>{
        let id = object::new(ctx);
        PolicyOperation<CH, P> {
            id, 
            init_for_policy_id: init_cap.init_for_policy_id,
            controlled_object_id: init_cap.controlled_object_id,
            checks: init_cap.checks,
            params
        }
    }

    fun ExecOperation<O, CH : copy, P>(exec_cap: &PolicyOperationExecCap<CH, P>, op: PolicyOperation<CH, P> ) : AuthorizedOperation<O, CH, P> {
        // Check that the capability to execute is tied to the capability to initiate the operation
        let cap_id = object::uid_to_inner(&exec_cap.id);
        assert!(cap_id == op.init_for_policy_id, 0);
        AuthorizedOperation<O, CH, P> {
            op,
        }
    }




    // Part 3: module initializer to be executed when this module is published
    fun init(_ctx: &mut TxContext) {
    }

    // Part 4: accessors required to read the struct attributes

    // part 5: public/ entry functions (introduced later in the tutorial)

    // part 6: private functions (if any)

    struct TestX {}

    #[test]
    public fun test_simple_sign() {
        use sui::tx_context;

        // Some dummy addresses
        let dummy_address_A = @0xAAAA;
        let _dummy_address_B = @0xBBBB;
        let _dummy_address_C = @0xCCCC;

        // create a dummy TxContext for testing
        let ctx = tx_context::dummy();

        // Setup a signer approval
        let (auth, output) = setup_signer_approval<TestX>(dummy_address_A, &mut ctx);

        // Test that a correct signer gets an output

        let id = object::new(&mut ctx);
        let op_id = object::uid_to_inner(&id);
        let auth_out = inner_authorize_signer_approval<TestX>(&dummy_address_A, &auth, op_id, &mut ctx);

        // clean-up

        drop(auth);
        object::delete(id);
        drop_setup_output<TestX>(output);

        // Check the output is ok
        assert!(auth_out.for_operation_id == op_id, 0);
        inner_drop(auth_out);


        // use sui::tx_context;
        // use sui::transfer;

        // // create a dummy TxContext for testing
        // let ctx = tx_context::dummy();

        // // create a sword
        // let sword = Sword {
        //     id: object::new(&mut ctx),
        //     magic: 42,
        //     strength: 7,
        // };

        // // check if accessor functions return correct values
        // assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);

        // // create a dummy address and transfer the sword
        // let dummy_address = @0xCAFE;
        // transfer::transfer(sword, dummy_address);
    }

    
}