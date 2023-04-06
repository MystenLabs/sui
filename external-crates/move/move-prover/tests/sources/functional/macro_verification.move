// flag: --trace

// CVC5 does currently not terminate on this:
// exclude_for: cvc5

/// This file contains some simulated expansions of functional macros and sketches how they can be verified.
module 0x42::FunMacros {
    use std::vector;

    /// Simulates `foreach!(v, |x| *x = *x + 1 spec  { ensures x == old(x) + 1; })`
    /// where `macro foreach!<T>(v: &mut vector<T>, action: |&mut T|)`
    ///
    /// For the foreach macro the loop invariant can be synthesized from a post condition associated
    /// with the lambda.
    fun foreach(v: &mut vector<u64>) {
        let i = 0;
        while (i < vector::length(v)) {
            let x = vector::borrow_mut(v, i);
            *x = *x + 1;
            i = i + 1;
        }
        spec {
            invariant i >= 0 && i <= len(v);
            invariant len(v) == len(old(v));
            invariant forall j in 0..i: v[j] == old(v)[j] + 1; // lambda substituted
            invariant forall j in i..len(v): v[j] == old(v)[j];
        };
    }
    spec foreach {
        ensures len(v) == len(old(v));
        ensures forall i in range(v): v[i] == old(v)[i] + 1; // succeeds
        ensures forall i in range(v): v[i] == old(v)[i] + 2; // fails
    }

    /// Simulates `reduce!(v, 0, |sum, x| *sum = *sum + *x ) spec { invariant sum == old(sum) + x; })`
    /// where `macro reduce!<T, R>(v: &vector<T>, neutral: R, reducer: |&mut R, &T|): R`.
    ///
    /// Because the elements of the vector are combined via the reducer, we cannot specify this with
    /// a quantifier, however, we can use a recursive helper function.
    fun reduce(v: &vector<u64>) : u64 {
        let i = 0;
        let sum = 0;
        while (i < vector::length(v)) {
            let x = vector::borrow(v, i);
            sum = sum + *x;
            i = i + 1;
        }
        spec {
            invariant i >= 0 && i <= len(v);
            invariant sum == spec_sum(v, i);
        };
        sum
    }
    spec reduce {
        ensures result == spec_sum(v, len(v));     // succeeds
        // Below we use a premise for the failure case to constraint model size.
        ensures len(v) <= 4 ==> result == spec_sum(v, len(v)) + 1; // fails
    }
    spec fun spec_sum(v: vector<u64>, end: num): num {
        if (end <= 0 || end > len(v))
            0
        else
            // lambda substituted, where old(sum) == spec_sum(v, end - 1)
            spec_sum(v, end - 1) + v[end - 1]
    }

    fun reduce_test(x: u64, y: u64, z: u64): u64 {
        let v = vector::empty();
        vector::push_back(&mut v, x);
        vector::push_back(&mut v, y);
        vector::push_back(&mut v, z);
        reduce(&v)
    }
    spec reduce_test {
        ensures result == x + y + z; // succeeds
        ensures result == x + y + y; // fails
    }

    /// Simulates `index_of!(v, |x| x > 2)`
    /// where `macro index_of!<T>(v: &vector<T>, pred: |&T|bool): u64`.
    ///
    /// For index_of, we do not need any invariant at the lambda, as it's spec can be fully derived.
    fun index_of(v: &vector<u64>): u64 {
        let i = 0;
        while (i < vector::length(v)) {
            let x = vector::borrow(v, i);
            if (*x > 2) return i;
            i = i + 1;
        }
        spec {
            invariant i >= 0 && i <= len(v);
            invariant forall j in 0..i: !(v[j] > 2);
        };
        i
    }
    spec index_of {
        ensures result >= 0 && result <= len(v);
        ensures forall j in 0..result: !(v[j] > 2);
    }
}
