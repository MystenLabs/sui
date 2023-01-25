module cw::policy {
    // Part 1: imports
    use sui::object::{Self, ID, UID};
    use sui::tx_context::{TxContext};
    use std::vector::{length, push_back, borrow, pop_back, destroy_empty};

    /// A module for defining Clack-Wilson Security policies and access control rules on-chain
    /// 
    /// This module allows one to wrap an object as a ControlledObject, and the controlled object 
    /// is associated with a ControlledObjectPolicyCap. This capability allows its owner to 
    /// extract a &mut reference to it, as well as define combinations of checks and polices on it.
    ///
    /// A policy ties together 3 structures: a controlled object, a structure describing some checks
    /// for an operation, and the concrete parameters of the operation. A policy is initialized through
    /// InitPolicy which takes a controlled object capability and a checks structure that constrains 
    /// the operations that can happen through this policy. This yields a PolicyOperationInitCap that 
    /// can be used to initiate operations within this policy, and a PolicyOperationAuthCap that may
    /// be used to authorize operations. An operation is initated using the InitOperation using the 
    /// abovecapability and some concrete parameters. The resulting PolicyOperation becomes a
    /// an AuthorizedOperation by calling AuthorizeOperation with the PolicyOperationAuthCap capability.
    /// 
    /// Complex access control rules may also be defined to protect and unlock access to PolicyOperationAuthCap
    /// capabilities and therefore to gate authorization of operations within the policy. The set of 
    /// structures AuthCapability, AuthSignerApproval, AuthThreshold, and AuthUnlockCap may be used to 
    /// and combined to create threshold policies based on specific capabilities or signers aproving 
    /// a PolicyOperation. The access control logic is setup using their corresponding `setup_` operations
    /// and authorizations use their `authorize_` operations. Ultimately an PolicyOperationAuthCap is
    /// protected wihtin a AuthUnlockCap, which when unlocked directly yields an AuthorizeOperation.
    /// 
    /// A user of this library needs to define the type of the controlled object with function like:
    /// `public fun controlled_inc(self : &mut policy::ControlledObject<TestDummy>, op: policy::AuthorizedOperation<TestDummy, TestDummyChecks, TestDummyParams>)`
    /// that only mutate access to the object if a valid AuthorizedOperation exists. Within these
    /// function unlock may be used on the controlled object and the AuthorizedOperation to get
    /// mutable references to the object, the checks and the parameters of the operation. Note that
    /// it is the responsibility of the function to implement the checks.

    // AuthSetup Outputs are used to define and initialize the authoritzation policy
    struct AuthSetupOutput<phantom T> { // Cannot store / drop must use
        for_auth_id: ID,
        // TODO: here bind an operation type ID?
    }

    // Private drop function force outputs to be used to setup auth flows.
    public fun drop_setup_output<T>(out: AuthSetupOutput<T>) {
        let AuthSetupOutput { for_auth_id: _for_auth_id } = out;
    }

    // Private drop function force outputs to be used to setup auth flows.
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

    public fun drop_auth<T>(out: AuthOutput<T>) {
        let AuthOutput<T> { id, valid_id: _, for_operation_id: _ } = out;
        object::delete(id);
    }

    // An authentication policy that is unlocked by an owned capability.

    struct AuthCapability<phantom T> has key, store {
        id : UID,
    }

    public fun drop_capability<T>(auth: AuthCapability<T>) {
        let AuthCapability { id } = auth;
        object::delete(id);
    }

    public fun setup_capability<T> (ctx: &mut TxContext) : (AuthCapability<T>, AuthSetupOutput<T>) {
        let id = object::new(ctx);
        let auth_object = AuthCapability<T> { id, };
        let for_auth_id = object::id(&auth_object);
        let auth_output = AuthSetupOutput<T> { for_auth_id, };
        
        (auth_object, auth_output)
    }

    public fun authorize_capability<T>(auth: &AuthCapability<T>, for_operation_id: ID, ctx: &mut TxContext) :  AuthOutput<T> {
        let id = object::new(ctx);
        let valid_id = object::id(auth);
        
        AuthOutput {
            id, valid_id, for_operation_id,
        }
    }

    // A K out of N authorization policy that requires a threshold k to activate.
    // An AND policy has a threshold of N out of N.
    // An OR policy has a threshold of 1 out of N.
    struct AuthThreshold<phantom T> has key, store {
        id : UID,
        input_ids: vector<ID>,
        k : u64,
    }

    public fun drop_threshold<T>(auth: AuthThreshold<T>) {
        let AuthThreshold { id, input_ids: _, k: _} = auth;
        object::delete(id);
    }

    /// Specialize the threshold policy to represent the OR of policies
    public fun setup_or<T>(inputs : vector<AuthSetupOutput<T>>, ctx: &mut TxContext) : (AuthThreshold<T>, AuthSetupOutput<T>)
    {
        setup_threshold(inputs, 1, ctx)
    }

    /// Specialize the threshold policy to represent the AND of policies. 
    public fun setup_and<T>(inputs : vector<AuthSetupOutput<T>>, ctx: &mut TxContext) : (AuthThreshold<T>, AuthSetupOutput<T>)
    {
        let k = length(&inputs);
        setup_threshold(inputs, k, ctx)
    }

    /// Setup a threshold policy with a number of inputs and a threshold k that need to be satisfied for the policy to be satisfied
    public fun setup_threshold<T>(inputs : vector<AuthSetupOutput<T>>, k:u64, ctx: &mut TxContext) : (AuthThreshold<T>, AuthSetupOutput<T>)
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
        let auth_object = AuthThreshold<T> { id, input_ids, k };
        let for_auth_id = object::id(&auth_object);
        let auth_output = AuthSetupOutput<T> { for_auth_id, };

        (auth_object, auth_output)

    } 

    public fun authorize_threshold<T>(auth: &AuthThreshold<T>, positions:vector<u64>, inputs : vector<AuthOutput<T>>, ctx: &mut TxContext) : AuthOutput<T> {
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
            assert!(*&inp.for_operation_id == for_operation_id, 0);
            drop_auth(inp);
        };

        // This is empty by now per the condition for exiting while loop
        destroy_empty(inputs);

        // We create the output for this auth bound to the operation.
        let id = object::new(ctx);
        let valid_id = object::id(auth);
        
        AuthOutput {
            id, valid_id, for_operation_id,
        }
    }

    // A final authentication step to authorize an operation.

    struct AuthUnlockCap<phantom T, C> has key, store {
        id : UID,
        input_id: ID,
        cap: C,
    }

    public fun drop_unlock_cap<T, CH : copy+drop, P: drop>(auth: AuthUnlockCap<T, PolicyOperationAuthCap<CH, P>>) {
        let AuthUnlockCap { id, input_id: _, cap} = auth;
        object::delete(id);
        drop_exec_cap(cap);
    }

    public fun setup_unlock_cap<T, C>(input : AuthSetupOutput<T>, cap : C, ctx: &mut TxContext) : AuthUnlockCap<T, C> {
        let id = object::new(ctx);
        let input_id = input.for_auth_id;

        drop_setup_output(input);

        AuthUnlockCap {
            id, input_id, cap
        }
    }

    public fun authorize_unlock_op<T, O, CH : store+copy+drop, P: store+drop>(auth: &AuthUnlockCap<T, PolicyOperationAuthCap<CH, P>>, input: AuthOutput<T>, op: PolicyOperation<CH, P>) : AuthorizedOperation<O, CH, P>{
        // Check this is the correct input signal to unlock capability
        assert!(input.valid_id == auth.input_id, 0);

        // Check the signal is tied to the operation ID
        assert!(input.for_operation_id == object::id(&op), 0);
        drop_auth(input);

        // Move to create an authorization for the operation
        AuthorizeOperation(&auth.cap, op)
    }

    // Operation Checks, Controlled Operations, Controlled Objects

    // The checks procedure defines checks that an invocstion to a controlled procedure must under take. 
    struct PolicyChecks<CH : copy> has store, copy, drop {
        checks: CH,
    }

    // Make an PolicyChecks object from an app specific checks spec
    public fun make_checks<CH:copy>(checks : CH) : PolicyChecks<CH> {
        PolicyChecks { checks }
    }

    // Return the app specifc checks
    public fun into_inner_checks<CH:copy>(checks: PolicyChecks<CH>) : CH {
        let PolicyChecks {checks } = checks;
        checks
    }

    // Define a policy operation tied to a specific policy, a specific controlled object
    // and a specific set of checks. The parameters are application specific to the operation
    // to be performed.
    struct PolicyOperation<CH : copy + drop, P: drop> has key, store {
        id: UID,
        init_for_policy_id: ID, 
        controlled_object_id : ID,
        checks: PolicyChecks<CH>,
        params: P,
    }

    // Cancel (and drop) the operation
    public fun cancel_operation<CH : copy + drop, P: drop>(op : PolicyOperation<CH, P>) {
        let PolicyOperation { id, init_for_policy_id: _, controlled_object_id: _, 
          checks: _, params: _ } = op;
        object::delete(id);
    }

    // Extract the ID of the operation.
    public fun op_id<CH : store + copy + drop, P: store + drop>(op : &PolicyOperation<CH, P>) : ID {
        object::id(op)
    }

    // A capability to initiate an operation within the policy on a controlled object.
    struct PolicyOperationInitCap<CH : copy, phantom P> has key, store {
        id: UID,
        init_for_policy_id: ID,
        controlled_object_id : ID,
        checks: PolicyChecks<CH>,
    }

    public fun drop_init_cap<CH: copy+drop, P: drop>(cap : PolicyOperationInitCap<CH,P>) {
        let PolicyOperationInitCap {id, init_for_policy_id: _, controlled_object_id: _, checks: _ } = cap;
        object::delete(id);
    }

    // A capability to authorize execution of an operation within the policy.
    struct PolicyOperationAuthCap<phantom CH, phantom P> has store {
        id: UID,
    }

    // A capability to present to enable execution of a policy.
    struct PolicyOperationRevokeCookie has copy, drop {}

    public fun drop_exec_cap<CH, P>(cap : PolicyOperationAuthCap<CH, P>) {
        let PolicyOperationAuthCap { id } = cap;
        object::delete(id);
    }

    // A controlled object, has an ID to tie it to a policy.
    struct ControlledObject<O: store> has key, store {
        id: UID,
        object: O
    }

    // A capability that allows the definition of policies for the controlled object.
    struct ControlledObjectPolicyCap has store, copy, drop {
        controlled_object_id: ID
    }

    public fun ControlObject<O : store>(object : O, ctx: &mut TxContext) : (ControlledObject<O>, ControlledObjectPolicyCap) {
        let id = object::new(ctx);

        let controlled_object = ControlledObject {
            id, object
        };

        let policy_cap = ControlledObjectPolicyCap {
            controlled_object_id: object::id(&controlled_object)
        };

        (controlled_object, policy_cap)
    }

    public fun controlled_mut<O : store>(obj: &mut ControlledObject<O>, cap: ControlledObjectPolicyCap) : &mut O{
        assert!(cap.controlled_object_id == object::id(obj), 0);
        &mut obj.object
    }

    struct AuthorizedOperation<phantom O, CH : copy+drop, P: drop> {
        op: PolicyOperation<CH, P>,
    }

    public fun unlock<O: store, CH: copy+drop, P: drop>(obj : &mut ControlledObject<O>, op: AuthorizedOperation<O, CH, P>): (&mut O, CH, P, ID) {
        let AuthorizedOperation { op } = op;
        let PolicyOperation { id, init_for_policy_id, controlled_object_id, 
          checks, params } = op;
        object::delete(id);
        assert!(controlled_object_id == object::id(obj), 0);

        (&mut obj.object, into_inner_checks(checks), params, init_for_policy_id)   
    }

    // Initialize a policy associated with the given controlled object and the given checks (implicitelly associated with an operation).
    // The revoke cooking is given, and will be required when the operation will be executed, to allow dropping the cookie to invalidate the policy
    // The function returns a capability to initiate an operation, as well as to execute and operation.
    public fun InitPolicy<O, CH: drop + copy, P: drop>(object: &ControlledObjectPolicyCap, checks: PolicyChecks<CH>, // _revoke_cookie: &PolicyOperationRevokeCookie, 
            ctx: &mut TxContext) : (PolicyOperationInitCap<CH, P>, PolicyOperationAuthCap<CH, P>) {
        let id = object::new(ctx);
        let init_for_policy_id = object::uid_to_inner(&id);
        let controlled_object_id = object.controlled_object_id;
        ( PolicyOperationInitCap { id: object::new(ctx), init_for_policy_id, controlled_object_id, checks }, PolicyOperationAuthCap { id, } )
    } 

    /// Initialize an operation that should be within the policy
    /// 
    public fun InitOperation<O, CH:copy+drop, P: drop>(init_cap: &PolicyOperationInitCap<CH, P>, params : P, ctx: &mut TxContext): PolicyOperation<CH, P>{
        let id = object::new(ctx);
        PolicyOperation<CH, P> {
            id, 
            init_for_policy_id: init_cap.init_for_policy_id,
            controlled_object_id: init_cap.controlled_object_id,
            checks: init_cap.checks,
            params
        }
    }

    /// Authorize the PolicyOperation to create an AuthorizedOperation.
    public fun AuthorizeOperation<O, CH : copy+drop, P: drop>(exec_cap: &PolicyOperationAuthCap<CH, P>, op: PolicyOperation<CH, P> ) : AuthorizedOperation<O, CH, P> {
        // Check that the capability to execute is tied to the capability to initiate the operation
        let cap_id = object::uid_to_inner(&exec_cap.id);
        assert!(cap_id == op.init_for_policy_id, 0);
        AuthorizedOperation<O, CH, P> {
            op,
        }
    }

    // Module initializer to be executed when this module is published
    fun init(_ctx: &mut TxContext) {
    }
    
}