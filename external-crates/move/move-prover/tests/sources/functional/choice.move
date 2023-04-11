// separate_baseline: cvc5
// TODO(cvc5): this test requires a separate baseline because cvc5 produces false positives for some of choices
// separate_baseline: simplify
module 0x42::TestSome {
    use std::signer;
    use std::vector;

    // Basic tests
    // ===========

    fun simple(): u64 { 4 }
    spec simple {
        ensures result <= (choose x: u64 where x >= 4);
    }

    fun simple_incorrect(b: bool): u64 {
        if (b) 4 else 5
    }
    spec simple_incorrect {
        // This fails because the interpretation is not influenced by an assertion.
        // The choice freely selects a value and not a 'fitting' one.
        ensures result == TRACE(choose x: u64 where x >= 4 && x <= 5);
    }

    // Testing choices in spec funs
    // =============================

    spec fun spec_fun_choice(x: u64): u64 {
        choose y: u64 where y >= x
    }
    fun with_spec_fun_choice(x: u64): u64 {
        x + 42
    }
    spec with_spec_fun_choice {
        //ensures result <= TRACE(spec_fun_choice(x + 42)); // TODO(tengzhang): the prover will fail with TRACE
        ensures result <= spec_fun_choice(x + 42);
    }

    // Testing choices using memory
    // ============================

    struct R has key {
        x: u64
    }

    fun populate_R(s1: &signer, s2: &signer) {
        move_to<R>(s1, R{x: 1});
        move_to<R>(s2, R{x: 2});
    }
    spec populate_R {
        let a1 = signer::address_of(s1);
        let a2 = signer::address_of(s2);
        /// The requires guarantees that there is no other address which can satisfy the choice below.
        requires forall a: address: !exists<R>(a);
        let choice = choose a: address where exists<R>(a) && global<R>(a).x == 2;
        ensures choice == signer::address_of(s2);
    }

    // Testing min choice
    // ==================

    fun test_min(): vector<u64> {
        let v = vector::empty<u64>();
        let v_ref = &mut v;
        vector::push_back(v_ref, 1);
        vector::push_back(v_ref, 2);
        vector::push_back(v_ref, 3);
        vector::push_back(v_ref, 2);
        v
    }
    spec test_min {
        ensures (choose min i in 0..len(result) where result[i] == 2) == 1;
    }

    fun test_not_using_min_incorrect(): vector<u64> {
        let v = vector::empty<u64>();
        let v_ref = &mut v;
        vector::push_back(v_ref, 1);
        vector::push_back(v_ref, 2);
        vector::push_back(v_ref, 3);
        vector::push_back(v_ref, 2);
        vector::push_back(v_ref, 2);
        v
    }
    spec test_not_using_min_incorrect {
        // This fails because we do not necessary select the smallest i
        ensures TRACE(choose i in 0..len(result) where result[i] == 2) == 1;
    }

    // Testing choice duplication
    // ==========================

    // This is only a compilation test. It fails verification.

    fun test_choice_dup_expected_fail(x: u64): u64 {
        x + 1
    }
    spec test_choice_dup_expected_fail {
        pragma opaque; // making this opaque lets the choice be injected at each call
        ensures result == TRACE(choose y: u64 where y > x);
    }

    fun test_choice_use1(a: u64): u64 {
        test_choice_dup_expected_fail(a)
    }

    fun test_choice_use2(_a: vector<u64>, b: u64): u64 {
        // with incorrect use of parameters, this would use $t0 as a parameter to the choice
        // function, which leads to a type error in boogie.
        test_choice_dup_expected_fail(b)
    }

    // Testing using the same choice in multiple verification targets
    // ==============================================================

    struct S has drop {
        x: u64
    }

    fun test_less_than_1(x: u64): u64 {
        x - 1
    }

    fun test_less_than_2(s: S): u64 {
        s.x - 1
    }

    spec test_less_than_1 {
        include EnsuresLessThan;
    }

    spec test_less_than_2 {
        include EnsuresLessThan { x: s.x };
    }

    spec schema EnsuresLessThan {
        x: u64;
        result: u64;
        ensures result != (choose i: u64 where i >= x);
    }

    // Semantics when the same-choice operator is referred
    // ===================================================

    // Refer to choice operators via let

    fun test_same_choice_via_let() {}
    spec test_same_choice_via_let {
        let evidence = (choose i: u64 where i > 0);
        ensures evidence == evidence;
        // expect to pass
    }

    fun test_different_choice_via_let() {}
    spec test_different_choice_via_let {
        let evidence1 = (choose i: u64 where i > 0);
        let evidence2 = (choose i: u64 where i > 0);
        ensures evidence1 == evidence2;
        // expect to fail, even though the choices are the same text-wise
    }

    // Refer to choice operators via spec fun

    spec fun choose_some_positive_u64(): u64 {
        choose i: u64 where i > 0
    }
    spec fun choose_another_positive_u64(): u64 {
        choose i: u64 where i > 0
    }
    spec fun choose_some_positive_u64_indirect(): u64 {
        choose_some_positive_u64()
    }

    fun test_same_choice_via_spec_fun() {}
    spec test_same_choice_via_spec_fun {
        ensures choose_some_positive_u64() == choose_some_positive_u64();
        // expect to pass
    }

    fun test_different_choice_via_spec_fun() {}
    spec test_different_choice_via_spec_fun {
        ensures choose_some_positive_u64() == choose_another_positive_u64();
        // expect to fail
    }

    fun test_same_choice_via_spec_fun_indirect() {}
    spec test_same_choice_via_spec_fun_indirect {
        ensures choose_some_positive_u64() == choose_some_positive_u64_indirect();
        // expect to pass
    }

    // Refer to choice operators w/ arguments via spec fun

    spec fun choose_a_larger_num(n: u64): u64 {
        choose i: u64 where i > n
    }

    fun test_same_choice_same_args_via_spec_fun(n: u64): bool { n == 0 }
    spec test_same_choice_same_args_via_spec_fun {
        let evidence1 = choose_a_larger_num(n);
        let evidence2 = choose_a_larger_num(n);
        ensures evidence1 == evidence2;
        // expect to pass
    }

    fun test_same_choice_different_args_via_spec_fun(x: u64, y: u64): bool { x == y }
    spec test_same_choice_different_args_via_spec_fun {
        let evidence1 = choose_a_larger_num(x);
        let evidence2 = choose_a_larger_num(y);
        ensures evidence1 == evidence2;
        // expect to fail
    }

    // Refer to choice operators w/ arguments via schema

    spec schema ResultLessThanK {
        k: u64;
        result: u64;
        ensures result != (choose i: u64 where i >= k);
    }

    fun test_same_choice_different_args_via_schema(x: u64, y: u64): u64 {
        if (x >= y) {
            y - 1
        } else {
            x - 1
        }
    }
    spec test_same_choice_different_args_via_schema {
        include ResultLessThanK {k: x};
        include ResultLessThanK {k: y};
        // expect to pass
    }

    fun test_same_choice_different_args_via_schema_2(): u64 {
        42
    }
    spec test_same_choice_different_args_via_schema_2 {
        include ResultLessThanK { k: 100 };
        include ResultLessThanK { k: 10 };
        // expect to fail
    }

    // A simple test for checking whether the reference has been removed in generated boogie code
    fun remove_ref<TokenType: store>(_id: &u64) {}

    spec remove_ref {
        let min_token_id = choose min i: u64 where _id == _id;
    }

    // Testing using the same choice in verification targets with multiple type instantiations
    // =======================================================================================

    struct BallotID has store, copy, drop {
        counter: u64,
    }

    struct Ballot<Proposal: store + drop> has store, copy, drop {
        proposal: Proposal,
        ballot_id: BallotID,
        expiration_timestamp_secs: u64,
    }

    struct Ballots<Proposal: store + drop> has key {
        ballots: vector<Ballot<Proposal>>,
    }

    struct BallotCounter has key {
        counter: u64,
    }

    public fun new_ballot_id(
        counter: u64,
    ): BallotID {
        BallotID {
            counter,
        }
    }

    public fun create_ballot<Proposal: store + copy + drop>(
        ballot_account: &signer
    ): BallotID {
        move_to(ballot_account, BallotCounter {
                counter: 0,
            });
        let ballot_id = new_ballot_id(0);
        ballot_id
    }

    spec module {
        invariant<Proposal> forall ballot_address: address
            where exists<Ballots<Proposal>>(ballot_address):
                exists<BallotCounter>(ballot_address);
    }

    spec fun get_ballot<Proposal>(ballot_address: address, ballot_id: BallotID): Ballot<Proposal> {
         let ballots = global<Ballots<Proposal>>(ballot_address).ballots;
         global<Ballots<Proposal>>(ballot_address).ballots[choose min i in 0..len(ballots) where ballots[i].ballot_id == ballot_id]
    }

    spec create_ballot {
        ensures !(get_ballot<Proposal>(signer::address_of(ballot_account), result).expiration_timestamp_secs
            <= 0);
    }

}
